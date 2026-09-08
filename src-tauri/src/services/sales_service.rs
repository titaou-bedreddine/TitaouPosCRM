use crate::database::DbState;


use crate::models::{CartItem, CreateSaleInput, HeldSale, Sale};
use rusqlite::Result;

pub fn process_sale(db: &DbState, input: CreateSaleInput) -> Result<String, String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let now = chrono::Local::now();
    let sale_number = format!("POS-{}", now.format("%Y%m%d%H%M%S"));

    let payment_status = if input.paid_amount >= input.total_amount {
        "paid"
    } else if input.paid_amount > 0 {
        "partial"
    } else {
        "credit"
    };

    tx.execute(
        "INSERT INTO sales (sale_number, session_id, user_id, customer_id, subtotal, discount_amount, discount_percentage, discount_reason, tax_amount, total_amount, paid_amount, change_amount, payment_status, status, notes, created_at, terminal_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'completed', ?14, datetime('now','localtime'), ?15)",
        rusqlite::params![
            sale_number, input.session_id, input.user_id, input.customer_id,
            input.subtotal, input.discount_amount, input.discount_percentage,
            input.discount_reason, input.tax_amount, input.total_amount,
            input.paid_amount, input.change_amount, payment_status, input.notes,
            crate::network::current_stamp_terminal()
        ],
    )
    .map_err(|e| e.to_string())?;

    let sale_id = tx.last_insert_rowid();

    let is_versement = input
        .payment_method
        .as_deref()
        .map(|m| m == "versement")
        .unwrap_or(false);
    // Versement/layaway: goods physically stay at the shop, stock is only
    // decremented when the sale is fully paid and released.
    let skip_stock = input.skip_stock || is_versement;

    // NOTE: the live table is app_settings (key/value). Querying a wrong
    // "settings" table made this ALWAYS read false — turning ON the
    // negative-stock toggle then failed every over-stock checkout.
    let allow_negative_stock: bool = tx
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'allow_negative_stock'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map(|v| v == "true")
        .unwrap_or(false);

    // Process each cart item & update stock
    for item in &input.items {
        if !skip_stock && !item.is_refund && !allow_negative_stock {
            let (curr_stock, prod_name): (f64, String) = tx
                .query_row(
                    "SELECT current_stock, coalesce(name_fr, name_ar, 'Produit') FROM products WHERE id = ?1",
                    rusqlite::params![item.product_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|e| e.to_string())?;

            if curr_stock < item.quantity {
                return Err(format!(
                    "Stock insuffisant pour '{}' (Stock: {}, Demandé: {}). Vente en stock négatif désactivée.",
                    prod_name, curr_stock, item.quantity
                ));
            }
        }

        tx.execute(
            "INSERT INTO sale_items (sale_id, product_id, quantity, unit_price, discount_amount, tax_amount, total_price, is_refunded, refunded_quantity)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                sale_id, item.product_id, item.quantity, item.unit_price,
                item.discount_amount, item.tax_amount, item.total_price,
                item.is_refund, if item.is_refund { item.quantity } else { 0.0 }
            ],
        )
        .map_err(|e| e.to_string())?;

        if skip_stock {
            continue;
        }

        let movement_type = if item.is_refund { "sale_refund" } else { "sale" };
        let stock_change = if item.is_refund { item.quantity } else { -item.quantity };

        tx.execute(
            "UPDATE products SET current_stock = current_stock + ?1 WHERE id = ?2",
            rusqlite::params![stock_change, item.product_id],
        )
        .map_err(|e| e.to_string())?;

        tx.execute(
            "INSERT INTO inventory_movements (product_id, quantity, type, reference_type, reference_id, user_id)
             VALUES (?1, ?2, ?3, 'sale', ?4, ?5)",
            rusqlite::params![item.product_id, stock_change, movement_type, sale_id, input.user_id],
        )
        .map_err(|e| e.to_string())?;
    }

    // Process payment breakdown
    let mut total_cash_paid: i64 = 0;

    let payments_list: Vec<crate::models::SalePaymentInput> = if input.payments.is_empty() {
        let method = input.payment_method.clone().unwrap_or_else(|| "cash".to_string());
        vec![crate::models::SalePaymentInput {
            payment_method: method,
            amount: input.total_amount,
            reference_code: None,
        }]
    } else {
        input.payments.clone()
    };

    for payment in &payments_list {
        tx.execute(
            "INSERT INTO sale_payments (sale_id, payment_method, amount, reference_code)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![sale_id, payment.payment_method, payment.amount, payment.reference_code],
        )
        .map_err(|e| e.to_string())?;

        if payment.payment_method == "cash" {
            total_cash_paid += payment.amount;
        }
    }

    let net_cash_change = total_cash_paid - input.change_amount;
    if net_cash_change != 0 {
        tx.execute(
            "INSERT INTO cash_movements (session_id, user_id, type, amount, reason, reference_type, reference_id)
             VALUES (?1, ?2, 'cash_sale', ?3, ?4, 'sale', ?5)",
            rusqlite::params![
                input.session_id, input.user_id, net_cash_change,
                format!("POS Sale / بيع {}", sale_number), sale_id
            ],
        )
        .map_err(|e| e.to_string())?;

        tx.execute(
            "UPDATE cash_sessions SET expected_cash = expected_cash + ?1 WHERE id = ?2",
            rusqlite::params![net_cash_change, input.session_id],
        )
        .map_err(|e| e.to_string())?;
    }

    // Credit & versement both track the unpaid remainder on the customer's
    // account. Credit: goods leave now, total owed minus paid now. Versement:
    // goods stay at the shop, same remainder math. Refund carts (negative
    // total) never touch balances — clamp(min, max) would panic on negatives.
    let owed_remainder = if input.total_amount > 0 {
        (input.total_amount - input.paid_amount.clamp(0, input.total_amount)).max(0)
    } else {
        0
    };
    if owed_remainder > 0 {
        if let Some(cust_id) = input.customer_id {
            tx.execute(
                "UPDATE customers SET balance = balance + ?1 WHERE id = ?2",
                rusqlite::params![owed_remainder, cust_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    let mut total_cost: i64 = 0;
    for item in &input.items {
        let p_cost: i64 = tx
            .query_row(
                "SELECT purchase_price FROM products WHERE id = ?1",
                rusqlite::params![item.product_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let item_cost = (p_cost as f64 * item.quantity).round() as i64;
        if item.is_refund {
            total_cost -= item_cost;
        } else {
            total_cost += item_cost;
        }
    }
    let gross_profit: i64 = input.total_amount - total_cost;

    // Cloud sync outbox (inside the SAME transaction — a rolled-back sale
    // never syncs). Refund carts (all items is_refund) push as Refund events
    // (return movements only); normal carts push as Sale events. Payload
    // references LOCAL ids; push.rs resolves them through sync_map at send
    // time.
    {
        let is_refund_cart = !input.items.is_empty() && input.items.iter().all(|i| i.is_refund);
        let items_json: Vec<serde_json::Value> = input
            .items
            .iter()
            .map(|it| {
                serde_json::json!({
                    "product_local_id": it.product_id,
                    "quantity": it.quantity,
                    "unit_price": it.unit_price,
                    "line_total": it.total_price,
                })
            })
            .collect();
        let payload = if is_refund_cart {
            // Refund: only the stock truth crosses to the CRM.
            serde_json::json!({
                "sale_number": sale_number,
                "items": items_json,
            })
        } else {
            let payments_json: Vec<serde_json::Value> = payments_list
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "method": p.payment_method,
                        "amount": p.amount,
                    })
                })
                .collect();
            serde_json::json!({
                "sale_number": sale_number,
                "items": items_json,
                "subtotal": input.subtotal,
                "tax_amount": input.tax_amount,
                "total": input.total_amount,
                "paid": input.paid_amount,
                "skip_stock": skip_stock,
                "payments": payments_json,
                "customer_local_id": input.customer_id,
                "created_at": now.to_rfc3339(),
            })
        };
        let entity = if is_refund_cart {
            crate::cloudsync::outbox::OutboxEntity::Refund
        } else {
            crate::cloudsync::outbox::OutboxEntity::Sale
        };
        let _ = crate::cloudsync::outbox::enqueue_tx(
            &tx,
            entity,
            sale_id,
            &payload.to_string(),
        );
    }

    tx.commit().map_err(|e| e.to_string())?;

    // Release the connection lock BEFORE the notifier: notify_if_enabled
    // reads settings through the same DbState and would self-deadlock on
    // the non-reentrant mutex — after commit, freezing the invoke forever.
    drop(conn);

    // Telegram "each sale" alert (fire-and-forget on a background thread),
    // localized to the UI language (en/ar/fr), attributed to the cashier.
    {
        let lang = crate::services::notifier_service::ui_language(db);
        let actor = crate::services::notifier_service::actor_label(db, Some(input.user_id));
        let date_str = now.format("%Y-%m-%d %H:%M").to_string();
        let terminal = crate::network::current_stamp_terminal();
        let text = crate::services::notifier_service::tr(
            &lang,
            (
                format!(
                    "🧾 *Receipt #{}* ({})\n💰 Total: *{} DZD*\n📈 Gross Profit: *{} DZD*\n👤 Cashier: {}\n🖥 Terminal: {}\n📅 Date: {}",
                    sale_id, sale_number, input.total_amount, gross_profit, actor, terminal, date_str
                ),
                format!(
                    "🧾 *وصل رقم #{}* ({})\n💰 المجموع: *{} دج*\n📈 إجمالي الربح: *{} دج*\n👤 الكاشير: {}\n🖥 الجهاز: {}\n📅 التاريخ: {}",
                    sale_id, sale_number, input.total_amount, gross_profit, actor, terminal, date_str
                ),
                format!(
                    "🧾 *Reçu N°{}* ({})\n💰 Total : *{} DZD*\n📈 Marge brute : *{} DZD*\n👤 Caissier : {}\n🖥 Caisse : {}\n📅 Date : {}",
                    sale_id, sale_number, input.total_amount, gross_profit, actor, terminal, date_str
                ),
            ),
        );
        crate::services::notifier_service::notify_if_enabled(db, "notify_each_sale", text);
    }

    Ok(sale_number)
}

pub fn list_sales(
    db: &DbState,
    start_date: Option<String>,
    end_date: Option<String>,
    user_id: Option<i64>,
    limit: i64,
) -> Result<Vec<Sale>, String> {
    let conn = db.conn.lock().unwrap();
    let mut sql = String::from(
        "SELECT s.id, s.sale_number, s.session_id, s.user_id, u.display_name,
                s.customer_id, c.name, s.subtotal, s.discount_amount, s.tax_amount,
                s.total_amount, s.paid_amount, s.change_amount, s.payment_status, s.status, s.created_at,
                (SELECT sp.payment_method FROM sale_payments sp WHERE sp.sale_id = s.id ORDER BY sp.amount DESC LIMIT 1) as payment_method,
                (SELECT COUNT(*) FROM sale_items si WHERE si.sale_id = s.id) as lines_sold,
                (SELECT COALESCE(SUM(si.quantity), 0) FROM sale_items si WHERE si.sale_id = s.id) as units_sold,
                CASE WHEN s.notes LIKE '%MODIFIED%' THEN 1 ELSE 0 END as is_edited,
                COALESCE(s.terminal_name, '') as terminal_name
         FROM sales s
         LEFT JOIN users u ON s.user_id = u.id
         LEFT JOIN customers c ON s.customer_id = c.id
         WHERE 1=1"
    );

    if let Some(ref sd) = start_date {
        if !sd.is_empty() {
            sql.push_str(&format!(" AND DATE(s.created_at) >= '{}'", sd));
        }
    }

    if let Some(ref ed) = end_date {
        if !ed.is_empty() {
            sql.push_str(&format!(" AND DATE(s.created_at) <= '{}'", ed));
        }
    }

    if let Some(uid) = user_id {
        sql.push_str(&format!(" AND s.user_id = {}", uid));
    }

    sql.push_str(&format!(" ORDER BY s.id DESC LIMIT {}", limit));

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Sale {
                id: row.get(0)?,
                sale_number: row.get(1)?,
                session_id: row.get(2)?,
                user_id: row.get(3)?,
                user_name: row.get(4)?,
                customer_id: row.get(5)?,
                customer_name: row.get(6)?,
                subtotal: row.get(7)?,
                discount_amount: row.get(8)?,
                tax_amount: row.get(9)?,
                total_amount: row.get(10)?,
                paid_amount: row.get(11)?,
                change_amount: row.get(12)?,
                payment_status: row.get(13)?,
                status: row.get(14)?,
                created_at: row.get(15)?,
                payment_method: row.get(16)?,
                items: None,
                lines_sold: row.get(17)?,
                units_sold: row.get(18)?,
                is_edited: row.get::<_, i64>(19)? == 1,
                terminal_name: {
                    let t: String = row.get(20)?;
                    if t.is_empty() { None } else { Some(t) }
                },
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<Sale> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

/// Look up ONE sale by its receipt number (the receipt QR payload is
/// `SALE:<sale_number>` — scanning it in the POS opens the sale for
/// reprint/edit). None when no sale carries that number.
pub fn get_sale_by_number(db: &DbState, sale_number: &str) -> Result<Option<(Sale, Vec<CartItem>)>, String> {
    get_sale_by_query(db, "WHERE s.sale_number = ?1", &[&sale_number])
}

/// The most recent completed sale, with its items — powers "Reopen/Reprint
/// Last Receipt" straight from the POS. None until the first sale exists.
pub fn get_last_sale(db: &DbState) -> Result<Option<(Sale, Vec<CartItem>)>, String> {
    get_sale_by_query(db, "", &[])
}

/// Shared sale+items loader for get_last_sale / get_sale_by_number.
fn get_sale_by_query(
    db: &DbState,
    where_clause: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Option<(Sale, Vec<CartItem>)>, String> {
    let conn = db.conn.lock().unwrap();
    let sale: Option<Sale> = {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT s.id, s.sale_number, s.session_id, s.user_id, u.display_name,
                        s.customer_id, c.name, s.subtotal, s.discount_amount, s.tax_amount,
                        s.total_amount, s.paid_amount, s.change_amount, s.payment_status, s.status, s.created_at,
                        (SELECT sp.payment_method FROM sale_payments sp WHERE sp.sale_id = s.id ORDER BY sp.amount DESC LIMIT 1),
                        0, 0, CASE WHEN s.notes LIKE '%MODIFIED%' THEN 1 ELSE 0 END,
                        COALESCE(s.terminal_name, '')
                 FROM sales s
                 LEFT JOIN users u ON s.user_id = u.id
                 LEFT JOIN customers c ON s.customer_id = c.id
                 {}
                 ORDER BY s.id DESC LIMIT 1",
                where_clause
            ))
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params, |row| {
                Ok(Sale {
                    id: row.get(0)?,
                    sale_number: row.get(1)?,
                    session_id: row.get(2)?,
                    user_id: row.get(3)?,
                    user_name: row.get(4)?,
                    customer_id: row.get(5)?,
                    customer_name: row.get(6)?,
                    subtotal: row.get(7)?,
                    discount_amount: row.get(8)?,
                    tax_amount: row.get(9)?,
                    total_amount: row.get(10)?,
                    paid_amount: row.get(11)?,
                    change_amount: row.get(12)?,
                    payment_status: row.get(13)?,
                    status: row.get(14)?,
                    created_at: row.get(15)?,
                    payment_method: row.get(16)?,
                    items: None,
                    lines_sold: 0,
                    units_sold: 0.0,
                    is_edited: false,
                    terminal_name: {
                        let t: String = row.get(20)?;
                        if t.is_empty() { None } else { Some(t) }
                    },
                })
            })
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(Ok(s)) => Some(s),
            Some(Err(e)) => return Err(e.to_string()),
            None => None,
        }
    };

    let Some(mut sale) = sale else {
        return Ok(None);
    };

    let items: Vec<CartItem> = {
        let mut stmt = conn
            .prepare(
                "SELECT si.product_id, p.sku, '', p.name_ar, p.name_fr, p.name_en, p.image_path,
                        si.unit_price, si.quantity, si.discount_amount, si.tax_amount, si.total_price, si.is_refunded
                 FROM sale_items si
                 JOIN products p ON si.product_id = p.id
                 WHERE si.sale_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([sale.id], |row| {
                Ok(CartItem {
                    product_id: row.get(0)?,
                    sku: row.get(1)?,
                    barcode: None,
                    name_ar: row.get(3)?,
                    name_fr: row.get(4)?,
                    name_en: row.get(5)?,
                    image_path: row.get(6)?,
                    unit_price: row.get(7)?,
                    quantity: row.get(8)?,
                    discount_amount: row.get(9)?,
                    tax_amount: row.get(10)?,
                    total_price: row.get(11)?,
                    is_refund: row.get(12)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    sale.lines_sold = items.len() as i64;
    sale.units_sold = items.iter().map(|i| i.quantity).sum();
    sale.items = Some(items.clone());

    Ok(Some((sale, items)))
}

pub fn get_sale_items(db: &DbState, sale_id: i64) -> Result<Vec<CartItem>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT si.product_id, p.sku, '', p.name_ar, p.name_fr, p.name_en, p.image_path,
                    si.unit_price, si.quantity, si.discount_amount, si.tax_amount, si.total_price, si.is_refunded
             FROM sale_items si
             JOIN products p ON si.product_id = p.id
             WHERE si.sale_id = ?1",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([sale_id], |row| {
            Ok(CartItem {
                product_id: row.get(0)?,
                sku: row.get(1)?,
                barcode: None,
                name_ar: row.get(3)?,
                name_fr: row.get(4)?,
                name_en: row.get(5)?,
                image_path: row.get(6)?,
                unit_price: row.get(7)?,
                quantity: row.get(8)?,
                discount_amount: row.get(9)?,
                tax_amount: row.get(10)?,
                total_price: row.get(11)?,
                is_refund: row.get(12)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<CartItem> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

pub fn hold_sale(db: &DbState, user_id: i64, customer_id: Option<i64>, cart_json: &str, note: Option<String>) -> Result<i64, String> {
    let conn = db.conn.lock().unwrap();
    let now = chrono::Local::now();
    let sale_reference = format!("HELD-{}", now.format("%H%M%S"));

    conn.execute(
        "INSERT INTO held_sales (sale_reference, user_id, customer_id, cart_json, note)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![sale_reference, user_id, customer_id, cart_json, note],
    )
    .map_err(|e| e.to_string())?;

    Ok(conn.last_insert_rowid())
}

pub fn list_held_sales(db: &DbState) -> Result<Vec<HeldSale>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, sale_reference, user_id, customer_id, cart_json, note, created_at FROM held_sales ORDER BY id DESC")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(HeldSale {
                id: row.get(0)?,
                sale_reference: row.get(1)?,
                user_id: row.get(2)?,
                customer_id: row.get(3)?,
                cart_json: row.get(4)?,
                note: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<HeldSale> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

pub fn delete_held_sale(db: &DbState, held_id: i64) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute("DELETE FROM held_sales WHERE id = ?1", [held_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_sale(db: &DbState, sale_id: i64, user_id: Option<i64>) -> Result<(), String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    // Restore stock for each item
    {
        let mut stmt = tx
            .prepare("SELECT product_id, quantity, is_refunded FROM sale_items WHERE sale_id = ?1")
            .map_err(|e| e.to_string())?;
        let items: Vec<(i64, f64, bool)> = stmt
            .query_map([sale_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        for (product_id, qty, is_refund) in items {
            let stock_reversal = if is_refund { -qty } else { qty };
            tx.execute(
                "UPDATE products SET current_stock = current_stock + ?1 WHERE id = ?2",
                rusqlite::params![stock_reversal, product_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    // Delete sale (cascade deletes sale_items and sale_payments)
    tx.execute("DELETE FROM sales WHERE id = ?1", [sale_id])
        .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    {
        let lang = crate::services::notifier_service::ui_language(db);
        let actor = crate::services::notifier_service::actor_label(db, user_id);
        let text = crate::services::notifier_service::tr(
            &lang,
            (
                format!("🗑 *Sale Deleted* — Sale #{} removed from history
👤 By: {}", sale_id, actor),
                format!("🗑 *حذف عملية بيع* — العملية #{} حُذفت من السجل
👤 بواسطة: {}", sale_id, actor),
                format!("🗑 *Vente Supprimée* — Vente #{} annulée et supprimée de l'historique
👤 Par : {}", sale_id, actor),
            ),
        );
        crate::services::notifier_service::notify_if_enabled(db, "notify_history_change", text);
    }

    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SalePaymentInput;
    use rusqlite::Connection;
    use std::sync::mpsc;
    use std::time::Duration;

    fn create_test_db() -> DbState {
        let conn = Connection::open_in_memory().unwrap();
        let state = DbState { conn: std::sync::Mutex::new(conn) };
        state.run_migrations().unwrap();
        state.seed_default_admin().unwrap();
        // An open cash session for the sale's session_id.
        {
            let conn = state.conn.lock().unwrap();
            let _ = conn.execute(
                "INSERT INTO cash_sessions (register_id, user_id, opening_amount, expected_cash, status)
                 VALUES (1, 1, 0, 0, 'open')",
                [],
            );
        }
        state
    }

    fn sample_sale_input(db: &DbState) -> CreateSaleInput {
        let conn = db.conn.lock().unwrap();
        // A sellable product with stock, and an open cash session.
        conn.execute(
            "INSERT INTO products (sku, name_ar, name_fr, name_en, sale_price, current_stock)
             VALUES ('TEST-1', 'اختبار', 'Test', 'Test', 100, 10)",
            [],
        )
        .unwrap();
        let product_id = conn.last_insert_rowid();
        let session_id: i64 = conn
            .query_row("SELECT id FROM cash_sessions WHERE status='open'", [], |r| r.get(0))
            .unwrap();
        drop(conn);

        CreateSaleInput {
            session_id,
            user_id: 1,
            customer_id: None,
            items: vec![CartItem {
                product_id,
                sku: None, barcode: None, name_ar: None, name_fr: None,
                name_en: None, image_path: None,
                unit_price: 100, quantity: 1.0, discount_amount: 0,
                tax_amount: 0, total_price: 100, is_refund: false,
            }],
            subtotal: 100,
            discount_amount: 0,
            discount_percentage: 0.0,
            discount_reason: None,
            tax_amount: 0,
            total_amount: 100,
            paid_amount: 100,
            change_amount: 0,
            payments: vec![SalePaymentInput { payment_method: "cash".into(), amount: 100, reference_code: None }],
            payment_method: Some("cash".into()),
            is_refund: None,
            notes: None,
            skip_stock: false,
        }
    }

    // process_sale must never re-enter the db lock: it used to call the
    // Telegram notifier (which reads settings) while still holding the
    // connection mutex, self-deadlocking AFTER commit — the sale landed but
    // the invoke never returned, freezing the POS UI. This runs it on a
    // worker thread with a hard timeout so a regression hangs the test, not
    // the suite.
    #[test]
    fn test_process_sale_completes_without_deadlock() {
        let db = std::sync::Arc::new(create_test_db());
        let input = sample_sale_input(&db);

        let (tx, rx) = mpsc::channel();
        let db2 = std::sync::Arc::clone(&db);
        let handle = std::thread::spawn(move || {
            let result = process_sale(&db2, input);
            let _ = tx.send(result);
        });

        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(sale_number)) => {
                handle.join().unwrap();
                // Sale fully landed: sale row + payment + stock movement.
                let conn = db.conn.lock().unwrap();
                let sales: i64 = conn
                    .query_row("SELECT COUNT(*) FROM sales", [], |r| r.get(0))
                    .unwrap();
                let payments: i64 = conn
                    .query_row("SELECT COUNT(*) FROM sale_payments", [], |r| r.get(0))
                    .unwrap();
                let stock: f64 = conn
                    .query_row("SELECT current_stock FROM products LIMIT 1", [], |r| r.get(0))
                    .unwrap();
                assert_eq!(sales, 1, "sale row missing");
                assert_eq!(payments, 1, "payment row missing");
                assert!((stock - 9.0).abs() < f64::EPSILON, "stock not decremented, got {}", stock);
                assert!(sale_number.starts_with("POS-"));
            }
            Ok(Err(e)) => panic!("process_sale failed: {}", e),
            Err(_) => {
                // Do NOT join: the worker is parked on the mutex forever.
                panic!("DEADLOCK in process_sale: invoke never returned (notifier re-locked db.conn after commit)");
            }
        }
    }
}

/// Re-checkout of a sale edited from history ("Edit in POS"): the original
/// sale keeps its id, sale_number and created_at (no duplicate row), and is
/// tagged MODIFIED. Inventory movements and the drawer's cash_sale entry
/// are reversed for the old contents and rebooked for the new ones, so the
/// stock and register statistics always reflect the current cart.
pub fn replace_sale(
    db: &DbState,
    original_sale_id: i64,
    input: CreateSaleInput,
) -> Result<String, String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let (sale_number, session_id, _created_at): (String, i64, String) = tx
        .query_row(
            "SELECT sale_number, COALESCE(session_id, 0), COALESCE(created_at, '') FROM sales WHERE id = ?1",
            [original_sale_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| format!("Original sale not found: {}", e))?;

    // Reverse the old stock movements & sale items.
    let old_items = {
        let mut stmt = tx
            .prepare("SELECT product_id, quantity FROM sale_items WHERE sale_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([original_sale_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|x| x.ok()).collect::<Vec<_>>()
    };
    for (pid, qty) in &old_items {
        // Old sale took qty out of stock; put it back.
        let _ = tx.execute(
            "UPDATE products SET current_stock = current_stock + ?1 WHERE id = ?2",
            rusqlite::params![qty, pid],
        );
    }
    let _ = tx.execute("DELETE FROM inventory_movements WHERE reference_type = 'sale' AND reference_id = ?1", [original_sale_id]);
    let _ = tx.execute("DELETE FROM sale_payments WHERE sale_id = ?1", [original_sale_id]);
    let _ = tx.execute("DELETE FROM sale_items WHERE sale_id = ?1", [original_sale_id]);
    // Reverse the old drawer effect.
    let _ = tx.execute(
        "UPDATE cash_sessions SET expected_cash = expected_cash - COALESCE((SELECT amount FROM cash_movements WHERE reference_type = 'sale' AND reference_id = ?1), 0) WHERE id = ?2",
        rusqlite::params![original_sale_id, session_id],
    );
    let _ = tx.execute("DELETE FROM cash_movements WHERE reference_type = 'sale' AND reference_id = ?1", [original_sale_id]);

    // Rewrite the sale row in place: same id/number/date, MODIFIED tag.
    tx.execute(
        "UPDATE sales SET
            session_id = ?2, user_id = ?3, customer_id = ?4,
            subtotal = ?5, discount_amount = ?6, discount_percentage = ?7, discount_reason = ?8,
            tax_amount = ?9, total_amount = ?10, paid_amount = ?11, change_amount = ?12,
            payment_status = ?13, notes = COALESCE(notes, '') || ' | MODIFIED ' || datetime('now','localtime')
         WHERE id = ?1",
        rusqlite::params![
            original_sale_id, input.session_id, input.user_id, input.customer_id,
            input.subtotal, input.discount_amount, input.discount_percentage, input.discount_reason,
            input.tax_amount, input.total_amount, input.paid_amount, input.change_amount,
            "paid",
        ],
    )
    .map_err(|e| e.to_string())?;

    // NOTE: the live table is app_settings (key/value). Querying a wrong
    // "settings" table made this ALWAYS read false — turning ON the
    // negative-stock toggle then failed every over-stock checkout.
    let allow_negative_stock: bool = tx
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'allow_negative_stock'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map(|v| v == "true")
        .unwrap_or(false);

    // New items + movements.
    for item in &input.items {
        if !item.is_refund && !allow_negative_stock {
            let (curr_stock, prod_name): (f64, String) = tx
                .query_row(
                    "SELECT current_stock, coalesce(name_fr, name_ar, 'Produit') FROM products WHERE id = ?1",
                    rusqlite::params![item.product_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(|e| e.to_string())?;

            if curr_stock < item.quantity {
                return Err(format!(
                    "Stock insuffisant pour '{}' (Stock: {}, Demandé: {}). Vente en stock négatif désactivée.",
                    prod_name, curr_stock, item.quantity
                ));
            }
        }

        let stock_change = if item.is_refund { item.quantity } else { -item.quantity };
        tx.execute(
            "UPDATE products SET current_stock = current_stock + ?1 WHERE id = ?2",
            rusqlite::params![stock_change, item.product_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO inventory_movements (product_id, quantity, type, reference_type, reference_id, user_id)
             VALUES (?1, ?2, ?3, 'sale', ?4, ?5)",
            rusqlite::params![
                item.product_id, stock_change,
                if item.is_refund { "sale_refund" } else { "sale" },
                original_sale_id, input.user_id
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    // New drawer effect (cash sales only).
    if input.payment_method.as_deref() == Some("cash") {
        let net_cash = input.paid_amount - input.change_amount;
        if net_cash != 0 && input.session_id > 0 {
            tx.execute(
                "INSERT INTO cash_movements (session_id, user_id, type, amount, reason, reference_type, reference_id)
                 VALUES (?1, ?2, 'cash_sale', ?3, ?4, 'sale', ?5)",
                rusqlite::params![
                    input.session_id, input.user_id, net_cash,
                    format!("Edited sale / بيع معدل {}", sale_number), original_sale_id
                ],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE cash_sessions SET expected_cash = expected_cash + ?1 WHERE id = ?2",
                rusqlite::params![net_cash, input.session_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    tx.commit().map_err(|e| e.to_string())?;
    Ok(sale_number)
}

#[cfg(test)]
mod checkout_fk_tests {
    use super::*;

    fn fresh_db() -> DbState {
        let dir = std::env::temp_dir().join("titaou_fk_tests");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!(
            "fk_{}_{}.sqlite",
            std::process::id(),
            std::thread::current().name().unwrap_or("t").replace("::", "_")
        ));
        let _ = std::fs::remove_file(&path);
        let state = DbState { conn: std::sync::Mutex::new(rusqlite::Connection::open(&path).unwrap()) };
        state.run_migrations().unwrap();
        state.seed_default_admin().unwrap();
        state
    }

    fn sale_input(session_id: i64, user_id: i64, customer_id: Option<i64>) -> CreateSaleInput {
        CreateSaleInput {
            session_id,
            user_id,
            customer_id,
            subtotal: 100,
            discount_amount: 0,
            discount_percentage: 0.0,
            discount_reason: None,
            tax_amount: 0,
            total_amount: 100,
            paid_amount: 100,
            change_amount: 0,
            payment_method: Some("cash".into()),
            is_refund: Some(false),
            notes: None,
            skip_stock: false,
            payments: vec![],
            items: vec![],
        }
    }

    #[test]
    fn checkout_with_nonexistent_session_fails_fk() {
        let db = fresh_db();
        // The POS sends `session_id: activeSession?.id || 1` — session 1
        // does NOT exist in a fresh DB (sessions start at their own ids).
        let r = process_sale(&db, sale_input(1, 1, Some(1)));
        assert!(r.is_err(), "session 1 must not silently pass");
        println!("error was: {:?}", r.err());
    }

    #[test]
    fn checkout_with_real_session_succeeds() {
        let db = fresh_db();
        crate::services::cash_service::open_session(&db, 1, 1, 1000, None).unwrap();
        let session_id: i64 = {
            let conn = db.conn.lock().unwrap();
            conn.query_row("SELECT id FROM cash_sessions WHERE status='open' ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap()
        };
        let r = process_sale(&db, sale_input(session_id, 1, Some(1)));
        assert!(r.is_ok(), "real session must work: {:?}", r.err());
    }
}
