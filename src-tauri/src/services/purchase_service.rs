use crate::database::DbState;
use crate::models::{CreatePurchaseInput, Purchase, PurchaseItem};
use rusqlite::Result;

pub fn create_purchase(db: &DbState, input: CreatePurchaseInput) -> Result<String, String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    tx.execute(
        "INSERT INTO purchases (invoice_number, supplier_id, user_id, date, subtotal, discount, tax, total, paid_amount, payment_method, status, notes, terminal_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'received', ?11, ?12)",
        rusqlite::params![
            input.invoice_number, input.supplier_id, input.user_id, input.date,
            input.subtotal, input.discount, input.tax, input.total, input.paid_amount,
            input.payment_method, input.notes,
            crate::network::current_stamp_terminal()
        ],
    )
    .map_err(|e| e.to_string())?;

    let purchase_id = tx.last_insert_rowid();

    // Process purchase items & increment stock
    for item in &input.items {
        tx.execute(
            "INSERT INTO purchase_items (purchase_id, product_id, quantity, unit_cost, discount, tax, total)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                purchase_id, item.product_id, item.quantity, item.unit_cost,
                item.discount, item.tax, item.total
            ],
        )
        .map_err(|e| e.to_string())?;

        // Increase product stock & update purchase cost
        tx.execute(
            "UPDATE products SET current_stock = current_stock + ?1, purchase_price = ?2 WHERE id = ?3",
            rusqlite::params![item.quantity, item.unit_cost, item.product_id],
        )
        .map_err(|e| e.to_string())?;

        tx.execute(
            "INSERT INTO inventory_movements (product_id, quantity, type, reference_type, reference_id, user_id, cost_at_time)
             VALUES (?1, ?2, 'purchase', 'purchase', ?3, ?4, ?5)",
            rusqlite::params![item.product_id, item.quantity, purchase_id, input.user_id, item.unit_cost],
        )
        .map_err(|e| e.to_string())?;
    }

    // Update supplier balance if remaining unpaid
    let remaining = input.total - input.paid_amount;
    if remaining > 0 {
        tx.execute(
            "UPDATE suppliers SET balance = balance + ?1 WHERE id = ?2",
            rusqlite::params![remaining, input.supplier_id],
        )
        .map_err(|e| e.to_string())?;
    }

    // Cloud sync outbox (same transaction): purchase → CRM bon_achat.
    {
        let items_json: Vec<serde_json::Value> = input
            .items
            .iter()
            .map(|it| {
                serde_json::json!({
                    "product_local_id": it.product_id,
                    "quantity": it.quantity,
                    "unit_cost": it.unit_cost,
                })
            })
            .collect();
        let payload = serde_json::json!({
            "invoice_number": input.invoice_number,
            "supplier_local_id": input.supplier_id,
            "items": items_json,
        });
        let _ = crate::cloudsync::outbox::enqueue_tx(
            &tx,
            crate::cloudsync::outbox::OutboxEntity::Purchase,
            purchase_id,
            &payload.to_string(),
        );
    }

    tx.commit().map_err(|e| e.to_string())?;
    Ok(input.invoice_number)
}

pub fn list_purchases(db: &DbState) -> Result<Vec<Purchase>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.invoice_number, p.supplier_id, s.name, p.user_id, u.display_name,
                    p.date, p.subtotal, p.discount, p.tax, p.total, p.paid_amount, p.payment_method, p.status, p.notes, p.created_at,
                    COALESCE(p.terminal_name, '')
             FROM purchases p
             LEFT JOIN suppliers s ON p.supplier_id = s.id
             LEFT JOIN users u ON p.user_id = u.id
             ORDER BY p.id DESC LIMIT 100",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Purchase {
                id: row.get(0)?,
                invoice_number: row.get(1)?,
                supplier_id: row.get(2)?,
                supplier_name: row.get(3)?,
                user_id: row.get(4)?,
                user_name: row.get(5)?,
                date: row.get(6)?,
                subtotal: row.get(7)?,
                discount: row.get(8)?,
                tax: row.get(9)?,
                total: row.get(10)?,
                paid_amount: row.get(11)?,
                payment_method: row.get(12)?,
                status: row.get(13)?,
                notes: row.get(14)?,
                created_at: row.get(15)?,
                terminal_name: {
                    let t: String = row.get(16)?;
                    if t.is_empty() { None } else { Some(t) }
                },
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<Purchase> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}
/// Purchase items for the view/details modal.
pub fn get_purchase_items(db: &DbState, purchase_id: i64) -> Result<Vec<PurchaseItem>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT pi.id, pi.purchase_id, pi.product_id, p.name_fr, p.name_ar,
                    pi.quantity, pi.unit_cost, pi.discount, pi.tax, pi.total
             FROM purchase_items pi
             LEFT JOIN products p ON pi.product_id = p.id
             WHERE pi.purchase_id = ?1 ORDER BY pi.id ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([purchase_id], |row| {
            Ok(PurchaseItem {
                id: row.get(0)?,
                purchase_id: row.get(1)?,
                product_id: row.get(2)?,
                product_name: row.get(3)?,
                product_name_ar: row.get(4)?,
                quantity: row.get(5)?,
                unit_cost: row.get(6)?,
                discount: row.get(7)?,
                tax: row.get(8)?,
                total: row.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Delete a purchase invoice: returns the stock it added, reverses the
/// supplier balance for the unpaid part, and removes its movements.
pub fn delete_purchase(db: &DbState, purchase_id: i64, user_id: Option<i64>) -> Result<(), String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    // Reverse stock & remove inventory movements line by line.
    let items: Vec<(i64, f64)> = {
        let mut stmt = tx
            .prepare("SELECT product_id, quantity FROM purchase_items WHERE purchase_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([purchase_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    for (product_id, qty) in items {
        tx.execute(
            "UPDATE products SET current_stock = current_stock - ?1 WHERE id = ?2",
            rusqlite::params![qty, product_id],
        )
        .map_err(|e| e.to_string())?;
    }

    tx.execute(
        "DELETE FROM inventory_movements WHERE reference_type = 'purchase' AND reference_id = ?1",
        [purchase_id],
    )
    .map_err(|e| e.to_string())?;

    // Reverse the unpaid part still on the supplier's balance.
    let (total, paid, supplier_id): (i64, i64, i64) = tx
        .query_row(
            "SELECT total, paid_amount, supplier_id FROM purchases WHERE id = ?1",
            [purchase_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    let remaining = (total - paid).max(0);
    if remaining > 0 {
        tx.execute(
            "UPDATE suppliers SET balance = balance - ?1 WHERE id = ?2",
            rusqlite::params![remaining, supplier_id],
        )
        .map_err(|e| e.to_string())?;
    }

    tx.execute("DELETE FROM purchases WHERE id = ?1", [purchase_id])
        .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    {
        let lang = crate::services::notifier_service::ui_language(db);
        let actor = crate::services::notifier_service::actor_label(db, user_id);
        let text = crate::services::notifier_service::tr(
            &lang,
            (
                format!("🗑 *Purchase Deleted* — Purchase #{} cancelled, stock & supplier balance reverted
👤 By: {}", purchase_id, actor),
                format!("🗑 *حذف عملية شراء* — الفاتورة #{} أُلغيت، تم تصحيح المخزون ورصيد المورد
👤 بواسطة: {}", purchase_id, actor),
                format!("🗑 *Achat Supprimé* — Achat #{} annulé, stock et balance fournisseur corrigés
👤 Par : {}", purchase_id, actor),
            ),
        );
        crate::services::notifier_service::notify_if_enabled(db, "notify_history_change", text);
    }

    Ok(())
}
