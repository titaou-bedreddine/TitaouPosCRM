//! Pull Supabase → POS: clients/products/field orders/payments, cursor-based.
//! Runs on the coordinator after each push cycle. The CRM is the balance
//! authority (customer.balance is SET from CRM, never merged). Field orders
//! land in the read-only crm_orders mirror + apply 'sale' stock movements
//! exactly once per order (crm_orders.stock_applied flag).

use super::http::SupabaseClient;
use super::mapping::centimes_to_dzd;
use super::outbox;
use rusqlite::Connection;
use serde_json::{json, Value};

const BATCH: &str = "200";

/// One pull cycle: products (link by SKU/barcode), clients, field orders,
/// payments. Each stream is independent — a failure in one doesn't stop
/// the others (errors reported per stream).
pub fn pull_cycle(conn: &mut Connection, client: &SupabaseClient, org_id: &str) -> Vec<(String, String)> {
    let mut errors: Vec<(String, String)> = vec![];

    if let Err(e) = pull_products(conn, client) {
        errors.push(("products".into(), e));
    }
    if let Err(e) = pull_clients(conn, client, org_id) {
        errors.push(("clients".into(), e));
    }
    if let Err(e) = pull_field_orders(conn, client, org_id) {
        errors.push(("orders".into(), e));
    }
    if let Err(e) = pull_stock_movements(conn, client) {
        errors.push(("stock".into(), e));
    }
    errors
}

/// CRM stock_movements → POS inventory: THE stock channel. Every non-POS
/// movement (field orders' loads/transfers/returns, admin adjustments,
/// purchases created in the CRM) mirrors into the local ledger exactly once
/// (sync_map on the movement uuid). Movements the POS itself caused (whose
/// reference starts with 'POS') are echoes — the local ledger already has
/// them.
fn pull_stock_movements(conn: &mut Connection, client: &SupabaseClient) -> Result<(), String> {
    let cursor =
        outbox::get_cursor(conn, "stock_movements").unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
    let rows = client.select(
        "stock_movements",
        "id, product_id, type, quantity, reference, reason, created_at, product:products(name)",
        &[
            ("created_at", format!("gt.{cursor}")),
            ("order", "created_at.asc".into()),
            ("limit", BATCH.into()),
        ],
    )?;
    if rows.is_empty() {
        return Ok(());
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for row in &rows {
        let mv_id = row["id"].as_str().unwrap_or_default().to_string();
        if mv_id.is_empty() {
            continue;
        }
        // Echo filter: movements the POS itself caused.
        let reference = row["reference"].as_str().unwrap_or_default();
        if reference.starts_with("POS") {
            continue;
        }
        // Exactly-once per movement uuid.
        if outbox::local_id(&tx, "stock_movement", &mv_id).is_some() {
            continue;
        }

        let product_crm = row["product_id"].as_str().unwrap_or_default().to_string();
        let Some(local_product) = outbox::local_id(&tx, "product", &product_crm) else {
            // Unlinked product: skipped; the movement is revisited while the
            // cursor stays behind it (movements pull in created_at order and
            // the cursor only advances past fully-processed batches — a later
            // full-window re-scan catches it; acceptable v1 behavior).
            continue;
        };

        let qty = row["quantity"].as_f64().unwrap_or(0.0);
        let mv_type = row["type"].as_str().unwrap_or_default().to_string();
        let (local_type, signed): (&'static str, f64) = match mv_type.as_str() {
            "entry" => ("purchase", qty),
            "sale" => ("sale", -qty.abs()),
            "return" => ("sale_refund", qty.abs()),
            "transfer" => ("adjustment_dec", -qty.abs()), // truck load: warehouse → truck
            "adjustment" => (
                if qty >= 0.0 {
                    "adjustment_inc"
                } else {
                    "adjustment_dec"
                },
                qty,
            ),
            other => {
                return Err(format!("unknown CRM movement type '{other}'"));
            }
        };

        tx.execute(
            "UPDATE products SET current_stock = current_stock + ?1 WHERE id = ?2",
            rusqlite::params![signed, local_product],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO inventory_movements (product_id, quantity, type, reference_type, notes)
             VALUES (?1, ?2, ?3, 'crm_movement', ?4)",
            rusqlite::params![local_product, signed, local_type, row["reason"].as_str().unwrap_or("CRM stock movement")],
        )
        .map_err(|e| e.to_string())?;
        outbox::map_local(&tx, "stock_movement", tx.last_insert_rowid(), &mv_id)
            .map_err(|e| e.to_string())?;
    }

    let last = rows
        .last()
        .and_then(|r| r["created_at"].as_str())
        .unwrap_or_default()
        .to_string();
    tx.commit().map_err(|e| e.to_string())?;
    if !last.is_empty() {
        outbox::set_cursor(conn, "stock_movements", &last).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Products changed since the cursor: new CRM products insert into POS;
/// existing (linked or SKU/barcode-matched) get price updates. The FIRST
/// ever pull also runs the linking pass (see link pass inside).
fn pull_products(conn: &mut Connection, client: &SupabaseClient) -> Result<(), String> {
    let cursor = outbox::get_cursor(conn, "products").unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
    let rows = client.select(
        "products",
        "id, name, sku, barcode, category, unit_price, cost_price, is_active, min_stock_alert, updated_at",
        &[
            ("updated_at", format!("gt.{cursor}")),
            ("order", "updated_at.asc".into()),
            ("limit", BATCH.into()),
        ],
    )?;
    if rows.is_empty() {
        return Ok(());
    }

    // First-ever pull = linking pass opportunity: match by SKU then barcode
    // BEFORE inserting anything, so existing catalog rows link instead of
    // duplicating.
    let first_pull = cursor == "1970-01-01T00:00:00Z";

    // Resolve member names (preseller/seller → full_name) without a
    // PostgREST embed: FKs point at auth.users, so we fetch the small org
    // profiles table once per cycle (admin org-read, 0016).
    let mut member_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if !rows.is_empty() {
        if let Ok(profs) = client.select("profiles", "id, full_name", &[]) {
            for pr in profs {
                if let (Some(id), Some(name)) =
                    (pr["id"].as_str(), pr["full_name"].as_str())
                {
                    member_names.insert(id.to_string(), name.to_string());
                }
            }
        }
    }
    let member_name_of = |row: &Value| -> String {
        row["preseller_id"]
            .as_str()
            .or_else(|| row["seller_id"].as_str())
            .and_then(|id| member_names.get(id))
            .cloned()
            .unwrap_or_else(|| "—".to_string())
    };

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for row in &rows {
        let crm_id = row["id"].as_str().unwrap_or_default().to_string();
        let sku = row["sku"].as_str().unwrap_or_default();
        let barcode = row["barcode"].as_str().unwrap_or_default();
        let name = row["name"].as_str().unwrap_or("Produit").to_string();
        let category = row["category"].as_str().unwrap_or_default().to_string();
        let sale_price = centimes_to_dzd(row["unit_price"].as_i64().unwrap_or(0));
        let purchase_price = centimes_to_dzd(row["cost_price"].as_i64().unwrap_or(0));
        let min_stock = row["min_stock_alert"].as_f64().unwrap_or(0.0).to_string();
        let is_active = row["is_active"].as_bool().unwrap_or(true);

        // Skip our own echo: product already mapped to this CRM id.
        if let Some(_local) = outbox::local_id(&tx, "product", &crm_id) {
            // Update prices on the existing row.
            let _ = tx.execute(
                "UPDATE products SET sale_price = ?2, purchase_price = ?3, min_stock = ?4,
                        is_active = ?5, category_id = (SELECT id FROM categories WHERE name_fr = ?6 LIMIT 1)
                  WHERE id = (SELECT local_id FROM sync_map WHERE entity='product' AND remote_id = ?1)",
                rusqlite::params![crm_id, sale_price, purchase_price, min_stock, is_active, category],
            );
            continue;
        }

        if first_pull {
            // Link by SKU, then by primary barcode (product_barcodes table).
            let linked: Option<i64> = if !sku.is_empty() {
                tx.query_row(
                    "SELECT id FROM products WHERE sku = ?1 LIMIT 1",
                    rusqlite::params![sku],
                    |r| r.get(0),
                )
                .optional_ok()
                .flatten()
            } else {
                None
            }
            .or_else(|| {
                if !barcode.is_empty() {
                    tx.query_row(
                        "SELECT p.id FROM products p
                          JOIN product_barcodes b ON b.product_id = p.id
                          WHERE b.barcode = ?1 LIMIT 1",
                        rusqlite::params![barcode],
                        |r| r.get(0),
                    )
                    .optional_ok()
                    .flatten()
                } else {
                    None
                }
            });

            if let Some(local) = linked {
                outbox::map_local(&tx, "product", local, &crm_id).map_err(|e| e.to_string())?;
                continue; // linked — no duplicate insert
            }
        }

        // Unmapped CRM product → insert (pos-side name = CRM name in all
        // three languages; the catalog is shared now).
        let _ = tx.execute(
            "INSERT INTO products (sku, name_ar, name_fr, name_en, purchase_price, sale_price,
                                   min_stock, is_active, current_stock, created_at)
             VALUES (?1, ?2, ?2, ?2, ?3, ?4, ?5, ?6, 0, datetime('now','localtime'))",
            rusqlite::params![sku, name, purchase_price, sale_price, min_stock, is_active],
        );
        let local = tx.last_insert_rowid();
        outbox::map_local(&tx, "product", local, &crm_id).map_err(|e| e.to_string())?;

        // Primary barcode → product_barcodes row (POS scanner lookup).
        if !barcode.is_empty() {
            let _ = tx.execute(
                "INSERT OR IGNORE INTO product_barcodes (product_id, barcode, is_primary)
                 VALUES (?1, ?2, 1)",
                rusqlite::params![local, barcode],
            );
        }
    }
    let last = rows
        .last()
        .and_then(|r| r["updated_at"].as_str())
        .unwrap_or_default()
        .to_string();
    tx.commit().map_err(|e| e.to_string())?;
    if !last.is_empty() {
        outbox::set_cursor(conn, "products", &last).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Clients → POS customers. Balance SET from CRM (authority). Skip the
/// walk-in client (it has its own mapping) and skip rows already mapped that
/// only changed balance (patch).
fn pull_clients(conn: &mut Connection, client: &SupabaseClient, org_id: &str) -> Result<(), String> {
    let cursor = outbox::get_cursor(conn, "clients").unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
    let rows = client.select(
        "clients",
        "id, name, phone, owner_name, address, latitude, longitude, current_balance, is_active, updated_at",
        &[
            ("updated_at", format!("gt.{cursor}")),
            ("order", "updated_at.asc".into()),
            ("limit", BATCH.into()),
        ],
    )?;
    if rows.is_empty() {
        return Ok(());
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let _ = org_id;
    for row in &rows {
        let crm_id = row["id"].as_str().unwrap_or_default().to_string();
        let name = row["name"].as_str().unwrap_or("—").to_string();
        let balance = centimes_to_dzd(row["current_balance"].as_i64().unwrap_or(0));
        let is_active = row["is_active"].as_bool().unwrap_or(true);

        // Walk-in client: mark its mapping on first sight, never create a
        // second POS customer for it (POS customer id 1 already exists).
        if name == "Client Comptoir (POS)" {
            if outbox::local_id(&tx, "customer", &crm_id).is_none() {
                outbox::map_local(&tx, "customer", 1, &crm_id).map_err(|e| e.to_string())?;
            }
            continue;
        }

        if let Some(local) = outbox::local_id(&tx, "customer", &crm_id) {
            let _ = tx.execute(
                "UPDATE customers SET name = ?2, phone = ?3, address = ?4, balance = ?5,
                        is_active = ?6
                  WHERE id = ?1",
                rusqlite::params![
                    local,
                    name,
                    row["phone"].as_str(),
                    row["address"].as_str(),
                    balance,
                    is_active
                ],
            );
        } else {
            let _ = tx.execute(
                "INSERT INTO customers (name, phone, email, address, balance, is_active,
                                        qr_code, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now','localtime'))",
                rusqlite::params![
                    name,
                    row["phone"].as_str(),
                    row["email"].as_str().unwrap_or(""),
                    row["address"].as_str(),
                    balance,
                    is_active,
                    crm_id
                ],
            );
            outbox::map_local(&tx, "customer", tx.last_insert_rowid(), &crm_id).map_err(|e| e.to_string())?;
        }
    }
    let last = rows
        .last()
        .and_then(|r| r["updated_at"].as_str())
        .unwrap_or_default()
        .to_string();
    tx.commit().map_err(|e| e.to_string())?;
    if !last.is_empty() {
        outbox::set_cursor(conn, "clients", &last).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Field orders (source != 'pos') → crm_orders mirror + stock movements.
/// Exactly-once: crm_orders.stock_applied flag flips once per order; the
/// movement reference `crm_order:<uuid>` is unique-checked as belt+braces.
fn pull_field_orders(conn: &mut Connection, client: &SupabaseClient, _org_id: &str) -> Result<(), String> {
    // Orders don't carry a sync cursor column of their own yet — we track
    // high-water by created_at; updates to older orders are rare and picked
    // up by the periodic full re-check below (window of 7 days re-scanned).
    let cursor = outbox::get_cursor(conn, "orders").unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
    let rows = client.select(
        "orders",
        "*, client:clients(name)",
        &[
            ("created_at", format!("gt.{cursor}")),
            ("source", "neq.pos".into()),
            ("order", "created_at.asc".into()),
            ("limit", BATCH.into()),
        ],
    )?;

    // Resolve member names (preseller/seller → full_name) without a
    // PostgREST embed: FKs point at auth.users, so we fetch the small org
    // profiles table once per cycle (admin org-read, 0016).
    let mut member_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if !rows.is_empty() {
        if let Ok(profs) = client.select("profiles", "id, full_name", &[]) {
            for pr in profs {
                if let (Some(id), Some(name)) =
                    (pr["id"].as_str(), pr["full_name"].as_str())
                {
                    member_names.insert(id.to_string(), name.to_string());
                }
            }
        }
    }
    let member_name_of = |row: &Value| -> String {
        row["preseller_id"]
            .as_str()
            .or_else(|| row["seller_id"].as_str())
            .and_then(|id| member_names.get(id))
            .cloned()
            .unwrap_or_else(|| "—".to_string())
    };

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for row in &rows {
        let crm_id = row["id"].as_str().unwrap_or_default().to_string();
        if crm_id.is_empty() {
            continue;
        }
        let total = centimes_to_dzd(row["total_amount"].as_i64().unwrap_or(0));
        let paid = centimes_to_dzd(row["amount_paid"].as_i64().unwrap_or(0));
        let created_at = row["created_at"].as_str().unwrap_or_default().to_string();
        let updated_at = row["updated_at"].as_str().unwrap_or_default().to_string();

        let already: Option<i64> = tx
            .query_row(
                "SELECT stock_applied FROM crm_orders WHERE crm_id = ?1",
                rusqlite::params![crm_id],
                |r| r.get(0),
            )
            .optional_ok()
            .flatten();

        if already.is_some() {
            // Row seen before — just refresh payment/status fields.
            let _ = tx.execute(
                "UPDATE crm_orders SET status = ?2, payment_status = ?3, total_amount = ?4,
                        amount_paid = ?5, notes = ?6, updated_at = ?7, client_name = ?8, member_name = ?9
                  WHERE crm_id = ?1",
                rusqlite::params![
                    crm_id,
                    row["status"].as_str().unwrap_or(""),
                    row["payment_status"].as_str().unwrap_or(""),
                    total,
                    paid,
                    row["notes"].as_str(),
                    updated_at,
                    row["client"]["name"].as_str().unwrap_or("—"),
                    member_name_of(row),
                ],
            );
            continue;
        }

        // Fetch the order's items + apply stock movements.
        let items = client.select(
            "order_items",
            "product_id, quantity, unit_price, line_total, product:products(name)",
            &[("order_id", format!("eq.{crm_id}"))],
        )?;

        tx.execute(
            "INSERT OR REPLACE INTO crm_orders (crm_id, client_name, member_name, source, status,
                payment_status, total_amount, amount_paid, notes, created_at, updated_at, stock_applied)
             VALUES (?1, ?2, ?3, 'field', ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
            rusqlite::params![
                crm_id,
                row["client"]["name"].as_str().unwrap_or("—"),
                row["preseller"]["full_name"].as_str().unwrap_or("—"),
                row["status"].as_str().unwrap_or(""),
                row["payment_status"].as_str().unwrap_or(""),
                total,
                paid,
                row["notes"].as_str(),
                created_at,
                updated_at,
            ],
        )
        .map_err(|e| e.to_string())?;

        for item in &items {
            let product_crm = item["product_id"].as_str().unwrap_or_default();
            let local_product = outbox::local_id(&tx, "product", product_crm);
            let qty = item["quantity"].as_f64().unwrap_or(0.0);
            let unit_price = centimes_to_dzd(item["unit_price"].as_i64().unwrap_or(0));
            let line_total = centimes_to_dzd(item["line_total"].as_i64().unwrap_or(0));
            let product_name = item["product"]["name"].as_str().unwrap_or("—").to_string();

            // Mirror line (product_id nullable until linked).
            let _ = tx.execute(
                "INSERT OR REPLACE INTO crm_order_items (crm_order_id, product_id, product_name,
                    quantity, unit_price, line_total)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![crm_id, local_product, product_name, qty, unit_price, line_total],
            );

            // Stock for field orders moves via the CRM ledger's own
            // movements (pull_stock_movements mirrors them: order-time = no
            // movement, load = transfer, delivery = payment only). The
            // mirror row here is VISIBILITY ONLY.
        }

        // Flip the applied flag only when every line had a linked product.
        let unlinked: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM crm_order_items WHERE crm_order_id = ?1 AND product_id IS NULL",
                rusqlite::params![crm_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if unlinked == 0 {
            let _ = tx.execute(
                "UPDATE crm_orders SET stock_applied = 1 WHERE crm_id = ?1",
                rusqlite::params![crm_id],
            );
        }
    }
    let last = rows
        .last()
        .and_then(|r| r["created_at"].as_str())
        .unwrap_or_default()
        .to_string();
    tx.commit().map_err(|e| e.to_string())?;
    if !last.is_empty() {
        outbox::set_cursor(conn, "orders", &last).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Read-only helper for the Field Orders view (Svelte via Tauri command).
pub fn list_field_orders(conn: &Connection, limit: i64) -> Result<Value, String> {
    let mut stmt = conn
        .prepare(
            "SELECT crm_id, client_name, member_name, status, payment_status,
                    total_amount, amount_paid, notes, created_at,
                    (SELECT COUNT(*) FROM crm_order_items ci WHERE ci.crm_order_id = crm_orders.crm_id) AS lines
               FROM crm_orders ORDER BY created_at DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![limit], |r| {
            Ok(json!({
                "crm_id": r.get::<_, String>(0)?,
                "client_name": r.get::<_, String>(1)?,
                "member_name": r.get::<_, String>(2)?,
                "status": r.get::<_, String>(3)?,
                "payment_status": r.get::<_, String>(4)?,
                "total_amount": r.get::<_, i64>(5)?,
                "amount_paid": r.get::<_, i64>(6)?,
                "notes": r.get::<_, Option<String>>(7)?,
                "created_at": r.get::<_, String>(8)?,
                "lines": r.get::<_, i64>(9)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    let mut out = vec![];
    for row in rows.flatten() {
        out.push(row);
    }
    Ok(Value::Array(out))
}

/// Details (lines) for one field order.
pub fn field_order_lines(conn: &Connection, crm_id: &str) -> Result<Value, String> {
    let mut stmt = conn
        .prepare(
            "SELECT product_name, quantity, unit_price, line_total
               FROM crm_order_items WHERE crm_order_id = ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![crm_id], |r| {
            Ok(json!({
                "product_name": r.get::<_, String>(0)?,
                "quantity": r.get::<_, f64>(1)?,
                "unit_price": r.get::<_, i64>(2)?,
                "line_total": r.get::<_, i64>(3)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    let mut out = vec![];
    for row in rows.flatten() {
        out.push(row);
    }
    Ok(Value::Array(out))
}

// Small helper: Option-friendly query_row (rusqlite's OptionalExtension
// returns Result<Option<T>>; .optional_ok() flattens both layers).
trait OptionalOk<T> {
    fn optional_ok(self) -> Option<T>;
}

impl<T> OptionalOk<T> for rusqlite::Result<Option<T>> {
    fn optional_ok(self) -> Option<T> {
        self.ok().flatten()
    }
}
