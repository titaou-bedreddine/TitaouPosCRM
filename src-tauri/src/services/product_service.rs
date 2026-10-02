use crate::database::DbState;
use crate::models::{Category, Product, ProductInput, QuantityHistoryEntry, Unit, ProductPackaging, PackagingInput};
use rusqlite::Result;

pub fn search_products(
    db: &DbState,
    query: &str,
    category_id: Option<i64>,
    search_type: &str,
) -> Result<Vec<Product>, String> {
    let conn = db.conn.lock().unwrap();
    let q = query.trim();

    let mut sql = String::from(
        "SELECT DISTINCT p.id, p.sku, p.name_ar, p.name_fr, p.name_en, p.category_id, c.name_ar,
                p.unit_id, u.name, p.purchase_price, p.sale_price, p.min_sale_price, p.tax_rate,
                p.current_stock, p.min_stock, p.max_stock, p.image_path, p.expiry_date,
                COALESCE(p.is_scalable, 0), p.scale_code, p.scale_plu, COALESCE(p.scale_barcode_type, 97),
                COALESCE(p.scale_department_id, 1), COALESCE(p.scale_sync_status, 'pending'),
                p.is_bundle, p.is_active, COALESCE(p.pinned, 0), COALESCE(p.pin_order, 0),
                COALESCE(p.unloading_fee, 0)
         FROM products p
         LEFT JOIN categories c ON p.category_id = c.id
         LEFT JOIN units u ON p.unit_id = u.id
         LEFT JOIN product_barcodes pb ON p.id = pb.product_id
         WHERE p.is_active = 1"
    );

    if let Some(cid) = category_id {
        sql.push_str(&format!(" AND p.category_id = {}", cid));
    }

    if !q.is_empty() {
        match search_type {
            "name" => {
                sql.push_str(&format!(" AND (p.name_ar LIKE '%{0}%' OR p.name_fr LIKE '%{0}%' OR p.name_en LIKE '%{0}%')", q));
            }
            "barcode" => {
                sql.push_str(&format!(" AND (p.sku LIKE '%{0}%' OR pb.barcode LIKE '%{0}%')", q));
            }
            "price" => {
                if let Ok(price_val) = q.parse::<i64>() {
                    sql.push_str(&format!(" AND p.sale_price = {}", price_val));
                }
            }
            _ => {
                sql.push_str(&format!(
                    " AND (p.name_ar LIKE '%{0}%' OR p.name_fr LIKE '%{0}%' OR p.name_en LIKE '%{0}%' OR p.sku LIKE '%{0}%' OR pb.barcode LIKE '%{0}%')",
                    q
                ));
            }
        }
    }

    sql.push_str(" ORDER BY COALESCE(p.pinned, 0) DESC, COALESCE(p.pin_order, 0) ASC, p.id ASC LIMIT 1000");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let product_rows = stmt
        .query_map([], |row| {
            let is_scalable_int: i64 = row.get(18)?;
            Ok(Product {
                id: row.get(0)?,
                sku: row.get(1)?,
                name_ar: row.get(2)?,
                name_fr: row.get(3)?,
                name_en: row.get(4)?,
                category_id: row.get(5)?,
                category_name: row.get(6)?,
                unit_id: row.get(7)?,
                unit_name: row.get(8)?,
                purchase_price: row.get(9)?,
                sale_price: row.get(10)?,
                min_sale_price: row.get(11)?,
                tax_rate: row.get(12)?,
                current_stock: row.get(13)?,
                min_stock: row.get(14)?,
                max_stock: row.get(15)?,
                image_path: row.get(16)?,
                expiry_date: row.get(17)?,
                is_scalable: is_scalable_int == 1,
                scale_code: row.get(19)?,
                scale_plu: row.get(20)?,
                scale_barcode_type: row.get(21)?,
                scale_department_id: row.get(22)?,
                scale_sync_status: row.get(23)?,
                is_bundle: row.get(24)?,
                is_active: row.get(25)?,
                barcodes: Vec::new(),
                total_sold: None,
                pinned: row.get::<_, i64>(26)? == 1,
                pin_order: row.get(27)?,
                unloading_fee: row.get(28)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut products = Vec::new();
    for p_res in product_rows {
        if let Ok(mut p) = p_res {
            let mut b_stmt = conn
                .prepare("SELECT barcode FROM product_barcodes WHERE product_id = ?1 ORDER BY is_primary DESC")
                .map_err(|e| e.to_string())?;
            let barcodes = b_stmt
                .query_map([p.id], |r| r.get(0))
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            p.barcodes = barcodes;

            // Lifetime net units sold (sales minus refunds) for sorting.
            p.total_sold = conn
                .query_row(
                    "SELECT COALESCE(SUM(CASE WHEN si.is_refunded = 1 THEN -si.quantity ELSE si.quantity END), 0)
                     FROM sale_items si WHERE si.product_id = ?1",
                    [p.id],
                    |r| r.get(0),
                )
                .ok();

            products.push(p);
        }
    }

    Ok(products)
}

pub fn save_product(db: &DbState, input: ProductInput, product_id: Option<i64>, user_id: Option<i64>) -> Result<i64, String> {
    if input.purchase_price <= 0 {
        return Err("Purchase price must be greater than 0 / سعر الشراء إجباري وأكبر من الصفر".to_string());
    }
    if input.sale_price <= 0 {
        return Err("Sale price must be greater than 0 / سعر البيع إجباري وأكبر من الصفر".to_string());
    }
    if input.sale_price < input.purchase_price {
        return Err("Sale price cannot be less than purchase price / لا يمكن أن يكون سعر البيع أقل من سعر الشراء".to_string());
    }

    let mut conn = db.conn.lock().unwrap();

    // Reject duplicate barcodes and duplicate product names across the
    // catalog. When editing, the product's own rows are excluded.
    let clean_barcodes: Vec<String> = input
        .barcodes
        .iter()
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
        .collect();

    for b in &clean_barcodes {
        // "No rows" simply means no other product owns this barcode.
        let owner: Option<i64> = match conn.query_row(
            "SELECT product_id FROM product_barcodes WHERE barcode = ?1
             UNION SELECT id FROM products WHERE sku = ?1",
            [b],
            |r| r.get(0),
        ) {
            Ok(id) => Some(id),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.to_string()),
        };
        if let Some(owner_id) = owner {
            if Some(owner_id) != product_id {
                return Err(format!(
                    "Barcode {} is already used by another product / الباركود {} مستعمل من قبل منتج آخر",
                    b, b
                ));
            }
        }
    }

    for (field, name) in [
        ("name_ar", input.name_ar.trim()),
        ("name_fr", input.name_fr.trim()),
        ("name_en", input.name_en.trim()),
    ] {
        if name.is_empty() {
            continue;
        }
        let owner: Option<i64> = match conn.query_row(
            &format!("SELECT id FROM products WHERE {} = ?1 AND id != ?2", field),
            rusqlite::params![name, product_id.unwrap_or(-1)],
            |r| r.get(0),
        ) {
            Ok(id) => Some(id),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.to_string()),
        };
        if owner.is_some() {
            return Err(format!(
                "Product name \"{}\" already exists / اسم المنتج \"{}\" موجود مسبقاً",
                name, name
            ));
        }
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let is_scalable_int = if input.is_scalable { 1 } else { 0 };

    // For the Telegram change alerts: everything the user can alter that we
    // want to diff on edit (prices, stock, names, barcodes).
    let mut old_state: Option<(i64, i64, f64)> = None; // (purchase, sale, stock)
    let mut old_names: Option<(String, String)> = None; // (name_fr, name_ar)
    let mut old_barcodes: Vec<String> = Vec::new();

    // Packaging price snapshot BEFORE the save (for the price history):
    // captured only when this save manages packagings, so legacy callers
    // never produce packaging history rows.
    let old_packagings_json: Option<String> = if input.packagings.is_some() {
        match product_id {
            Some(pid) => Some(packagings_snapshot(&tx, pid)?),
            None => Some("[]".to_string()),
        }
    } else {
        None
    };

    let id = if let Some(pid) = product_id {
        // Fetch old prices for price history
        let old_prices: Option<(i64, i64)> = tx.query_row(
            "SELECT purchase_price, sale_price FROM products WHERE id = ?1",
            [pid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).ok();
        old_state = tx.query_row(
            "SELECT purchase_price, sale_price, current_stock FROM products WHERE id = ?1",
            [pid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).ok();
        old_names = tx.query_row(
            "SELECT COALESCE(name_fr, ''), COALESCE(name_ar, '') FROM products WHERE id = ?1",
            [pid],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        ).ok();
        {
            let mut stmt = tx
                .prepare("SELECT barcode FROM product_barcodes WHERE product_id = ?1")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([pid], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            for row in rows {
                old_barcodes.push(row.map_err(|e| e.to_string())?);
            }
        }

        // Packaging sale prices travel with the product in the SAME
        // transaction (pricing spec: never a half-saved product). Written
        // BEFORE the history row so the "new" snapshot reflects this save.
        if let Some(packagings) = &input.packagings {
            write_packagings_tx(&tx, pid, packagings, input.purchase_price)?;
        }
        let new_packagings_json: Option<String> = if input.packagings.is_some() {
            Some(packagings_snapshot(&tx, pid)?)
        } else {
            None
        };

        if let Some((old_pur, old_sale)) = old_prices {
            let prices_changed = old_pur != input.purchase_price || old_sale != input.sale_price;
            let packagings_changed =
                old_packagings_json.is_some() && old_packagings_json != new_packagings_json;
            if prices_changed || packagings_changed {
                let _ = tx.execute(
                    "INSERT INTO product_price_history (product_id, old_purchase_price, new_purchase_price, old_sale_price, new_sale_price, packagings_old, packagings_new)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        pid, old_pur, input.purchase_price, old_sale, input.sale_price,
                        old_packagings_json, new_packagings_json
                    ],
                );
            }
        }

        tx.execute(
            "UPDATE products SET
                sku = ?1, name_ar = ?2, name_fr = ?3, name_en = ?4,
                category_id = ?5, unit_id = ?6, purchase_price = ?7,
                sale_price = ?8, min_sale_price = ?9, tax_rate = ?10,
                current_stock = ?11, min_stock = ?12, image_path = ?13,
                expiry_date = ?14, is_scalable = ?15, scale_code = ?16,
                scale_plu = ?17, scale_barcode_type = ?18, scale_department_id = ?19,
                scale_sync_status = ?20, is_bundle = ?21, unloading_fee = ?22
             WHERE id = ?23",
            rusqlite::params![
                input.sku,
                input.name_ar,
                input.name_fr,
                input.name_en,
                input.category_id,
                input.unit_id,
                input.purchase_price,
                input.sale_price,
                input.min_sale_price,
                input.tax_rate,
                input.current_stock,
                input.min_stock,
                input.image_path,
                input.expiry_date,
                is_scalable_int,
                input.scale_code,
                input.scale_plu,
                input.scale_barcode_type,
                input.scale_department_id,
                input.scale_sync_status.unwrap_or_else(|| "pending".to_string()),
                input.is_bundle,
                input.unloading_fee,
                pid
            ],
        )
        .map_err(|e| e.to_string())?;

        tx.execute(
            "DELETE FROM product_barcodes WHERE product_id = ?1",
            [pid],
        )
        .map_err(|e| e.to_string())?;

        pid
    } else {
        tx.execute(
            "INSERT INTO products (
                sku, name_ar, name_fr, name_en, category_id, unit_id,
                purchase_price, sale_price, min_sale_price, tax_rate,
                current_stock, min_stock, image_path, expiry_date,
                is_scalable, scale_code, scale_plu, scale_barcode_type,
                scale_department_id, scale_sync_status, is_bundle, is_active, unloading_fee
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, 1, ?22)",
            rusqlite::params![
                input.sku,
                input.name_ar,
                input.name_fr,
                input.name_en,
                input.category_id,
                input.unit_id,
                input.purchase_price,
                input.sale_price,
                input.min_sale_price,
                input.tax_rate,
                input.current_stock,
                input.min_stock,
                input.image_path,
                input.expiry_date,
                is_scalable_int,
                input.scale_code,
                input.scale_plu,
                input.scale_barcode_type,
                input.scale_department_id,
                input.scale_sync_status.unwrap_or_else(|| "pending".to_string()),
                input.is_bundle,
                input.unloading_fee
            ],
        )
        .map_err(|e| e.to_string())?;

        let new_id = tx.last_insert_rowid();

        // Opening stock must land in the ledger TOO: the quantity history
        // replays inventory_movements, so a product created with stock
        // (e.g. 1200 units) was showing "no changes recorded yet".
        if input.current_stock.abs() >= f64::EPSILON {
            let mtype = if input.current_stock > 0.0 { "adjustment_inc" } else { "adjustment_dec" };
            let _ = tx.execute(
                "INSERT INTO inventory_movements (product_id, quantity, type, reference_type, reference_id, user_id, notes)
                 VALUES (?1, ?2, ?3, 'manual_edit', ?4, ?5, ?6)",
                rusqlite::params![
                    new_id,
                    input.current_stock,
                    mtype,
                    new_id,
                    user_id,
                    format!("Initial stock / المخزون الأولي: {}", input.current_stock)
                ],
            );
        }

        // Packaging prices saved atomically with the new product (pricing
        // spec §23), before the history row records their starting values.
        if let Some(packagings) = &input.packagings {
            write_packagings_tx(&tx, new_id, packagings, input.purchase_price)?;
        }
        let new_packagings_json: Option<String> = if input.packagings.is_some() {
            Some(packagings_snapshot(&tx, new_id)?)
        } else {
            None
        };

        let _ = tx.execute(
            "INSERT INTO product_price_history (product_id, old_purchase_price, new_purchase_price, old_sale_price, new_sale_price, packagings_old, packagings_new)
             VALUES (?1, 0, ?2, 0, ?3, NULL, ?4)",
            rusqlite::params![new_id, input.purchase_price, input.sale_price, new_packagings_json],
        );

        new_id
    };

    for (i, b) in clean_barcodes.iter().enumerate() {
        let is_primary = i == 0;
        let _ = tx.execute(
            "INSERT OR IGNORE INTO product_barcodes (product_id, barcode, is_primary) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, b, is_primary],
        );
    }

    // Quantity history: a manual stock edit on an existing product is an
    // adjustment movement so the history tab shows who changed what, when.
    if let (Some(pid), Some((_, _, old_stock))) = (product_id, old_state) {
        let delta = input.current_stock as f64 - old_stock;
        if delta.abs() >= f64::EPSILON {
            let mtype = if delta > 0.0 { "adjustment_inc" } else { "adjustment_dec" };
            let _ = tx.execute(
                "INSERT INTO inventory_movements (product_id, quantity, type, reference_type, reference_id, user_id, notes)
                 VALUES (?1, ?2, ?3, 'manual_edit', ?4, ?5, ?6)",
                rusqlite::params![
                    pid, delta, mtype, pid,
                    user_id,
                    format!("Manual stock edit: {} → {}", old_stock, input.current_stock)
                ],
            );
        }
    }

    // (Packaging sale prices were written per-branch above, in the same
    // transaction as the product row and the price-history snapshots.)

    // Cloud sync outbox (same transaction): product create/update →
    // upsert_pos_product; a manual stock edit additionally becomes a
    // stock_adjustment event (append-only ledger on the CRM side).
    {
        let primary_barcode = clean_barcodes.first().cloned();
        // Packaging definitions travel with the product so the CRM mirrors
        // the Palette/Fardeau/Bottle tier prices for the field apps. The
        // per-unit price rides along (authoritative); the CRM RPC keeps
        // writing the derived per-package total for existing readers.
        let mut packagings_json: Vec<serde_json::Value> = vec![];
        {
            let mut stmt = tx
                .prepare(
                    "SELECT id, name, units_per_package, sale_price, COALESCE(purchase_price, 0),
                            COALESCE(sale_price_per_unit, 0), packaging_type_id
                       FROM product_packagings WHERE product_id = ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(rusqlite::params![id], |r| {
                    Ok(serde_json::json!({
                        "pos_packaging_id": r.get::<_, i64>(0)?,
                        "name": r.get::<_, String>(1)?,
                        "units_per_package": r.get::<_, f64>(2)?,
                        "sale_price": r.get::<_, i64>(3)?,
                        "purchase_price": r.get::<_, i64>(4)?,
                        "sale_price_per_unit": r.get::<_, i64>(5)?,
                        "packaging_type_id": r.get::<_, Option<i64>>(6)?,
                    }))
                })
                .map_err(|e| e.to_string())?;
            for row in rows.flatten() {
                packagings_json.push(row);
            }
        }
        let payload = serde_json::json!({
            "sku": input.sku,
            "barcode": primary_barcode,
            "name_fr": input.name_fr,
            "name_ar": input.name_ar,
            "name_en": input.name_en,
            "sale_price": input.sale_price,
            "purchase_price": input.purchase_price,
            "min_stock": input.min_stock,
            "is_active": true,
            "packagings": packagings_json,
        });
        let _ = crate::cloudsync::outbox::enqueue_tx(
            &tx,
            crate::cloudsync::outbox::OutboxEntity::Product,
            id,
            &payload.to_string(),
        );
        if let (Some(pid), Some((_, _, old_stock))) = (product_id, old_state) {
            let delta = input.current_stock as f64 - old_stock;
            if delta.abs() >= f64::EPSILON {
                let adj_payload = serde_json::json!({
                    "product_local_id": pid,
                    "quantity": delta,
                    "reason": format!("Manual stock edit: {} → {}", old_stock, input.current_stock),
                });
                let _ = crate::cloudsync::outbox::enqueue_tx(
                    &tx,
                    crate::cloudsync::outbox::OutboxEntity::StockAdjustment,
                    pid,
                    &adj_payload.to_string(),
                );
            }
        }
    }

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    // Telegram alerts for product changes (fire-and-forget), localized to the
    // UI language (en/ar/fr) and attributed to the acting user.
    if product_id.is_some() {
        let lang = crate::services::notifier_service::ui_language(db);
        let actor = crate::services::notifier_service::actor_label(db, user_id);
        let name = if input.name_fr.is_empty() { input.name_ar.clone() } else { input.name_fr.clone() };

        if let Some((old_pur, old_sale, old_stock)) = old_state {
            if old_pur != input.purchase_price || old_sale != input.sale_price {
                let text = crate::services::notifier_service::tr(
                    &lang,
                    (
                        format!("🏷 *Price Change* — {}\nPurchase: {} → {} DZD\nSale: {} → {} DZD\n👤 By: {}", name, old_pur, input.purchase_price, old_sale, input.sale_price, actor),
                        format!("🏷 *تغيير السعر* — {}\nالشراء: {} → {} دج\nالبيع: {} → {} دج\n👤 بواسطة: {}", name, old_pur, input.purchase_price, old_sale, input.sale_price, actor),
                        format!("🏷 *Changement de Prix* — {}\nAchat : {} → {} DZD\nVente : {} → {} DZD\n👤 Par : {}", name, old_pur, input.purchase_price, old_sale, input.sale_price, actor),
                    ),
                );
                crate::services::notifier_service::notify_if_enabled(db, "notify_price_change", text);
            }
            if (old_stock - input.current_stock).abs() >= f64::EPSILON {
                let text = crate::services::notifier_service::tr(
                    &lang,
                    (
                        format!("📦 *Quantity Change* — {}\nStock: {} → {}\n👤 By: {}", name, old_stock, input.current_stock, actor),
                        format!("📦 *تغيير الكمية* — {}\nالمخزون: {} → {}\n👤 بواسطة: {}", name, old_stock, input.current_stock, actor),
                        format!("📦 *Changement de Quantité* — {}\nStock : {} → {}\n👤 Par : {}", name, old_stock, input.current_stock, actor),
                    ),
                );
                crate::services::notifier_service::notify_if_enabled(db, "notify_qty_change", text);
            }
        }

        // Name change alert (any of the localized names).
        if let Some((old_fr, old_ar)) = &old_names {
            if old_fr != &input.name_fr || old_ar != &input.name_ar {
                let text = crate::services::notifier_service::tr(
                    &lang,
                    (
                        format!("📝 *Product Renamed*\nOld: {} / {}\nNew: {} / {}\n👤 By: {}", old_fr, old_ar, input.name_fr, input.name_ar, actor),
                        format!("📝 *تغيير اسم المنتج*\nالقديم: {} / {}\nالجديد: {} / {}\n👤 بواسطة: {}", old_fr, old_ar, input.name_fr, input.name_ar, actor),
                        format!("📝 *Produit Renommé*\nAncien : {} / {}\nNouveau : {} / {}\n👤 Par : {}", old_fr, old_ar, input.name_fr, input.name_ar, actor),
                    ),
                );
                crate::services::notifier_service::notify_if_enabled(db, "notify_product_change", text);
            }
        }

        // Barcode set change alert.
        {
            let mut new_sorted = clean_barcodes.clone();
            new_sorted.sort();
            let mut old_sorted = old_barcodes.clone();
            old_sorted.sort();
            if new_sorted != old_sorted {
                let removed: Vec<String> = old_sorted.iter().filter(|b| !new_sorted.contains(b)).cloned().collect();
                let added: Vec<String> = new_sorted.iter().filter(|b| !old_sorted.contains(b)).cloned().collect();
                let text = crate::services::notifier_service::tr(
                    &lang,
                    (
                        format!("📶 *Barcode Change* — {}\n➕ Added: {}\n➖ Removed: {}\n👤 By: {}", name, added.join(", "), removed.join(", "), actor),
                        format!("📶 *تغيير الباركود* — {}\n➕ أُضيف: {}\n➖ حُذف: {}\n👤 بواسطة: {}", name, added.join(", "), removed.join(", "), actor),
                        format!("📶 *Changement de Code-barres* — {}\n➕ Ajouté : {}\n➖ Retiré : {}\n👤 Par : {}", name, added.join(", "), removed.join(", "), actor),
                    ),
                );
                crate::services::notifier_service::notify_if_enabled(db, "notify_product_change", text);
            }
        }
    }

    Ok(id)
}

pub fn delete_product(db: &DbState, product_id: i64) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute("UPDATE products SET is_active = 0 WHERE id = ?1", [product_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_categories(db: &DbState) -> Result<Vec<Category>, String> {
    let conn = db.conn.lock().unwrap();
    let _ = conn.execute("ALTER TABLE categories ADD COLUMN color TEXT DEFAULT '#0284c7';", []);

    let mut stmt = conn
        .prepare("SELECT id, parent_id, name_ar, name_fr, name_en, COALESCE(color, '#0284c7'), is_active FROM categories WHERE is_active = 1 ORDER BY id ASC")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Category {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name_ar: row.get(2)?,
                name_fr: row.get(3)?,
                name_en: row.get(4)?,
                color: row.get(5)?,
                is_active: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<Category> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

pub fn save_category(db: &DbState, name_ar: &str, name_fr: &str, name_en: &str, color: &str, category_id: Option<i64>) -> Result<i64, String> {
    let conn = db.conn.lock().unwrap();
    let _ = conn.execute("ALTER TABLE categories ADD COLUMN color TEXT DEFAULT '#0284c7';", []);

    if let Some(cid) = category_id {
        conn.execute(
            "UPDATE categories SET name_ar = ?1, name_fr = ?2, name_en = ?3, color = ?4 WHERE id = ?5",
            rusqlite::params![name_ar, name_fr, name_en, color, cid],
        ).map_err(|e| e.to_string())?;
        Ok(cid)
    } else {
        conn.execute(
            "INSERT INTO categories (name_ar, name_fr, name_en, color, is_active) VALUES (?1, ?2, ?3, ?4, 1)",
            rusqlite::params![name_ar, name_fr, name_en, color],
        ).map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }
}

pub fn delete_category(db: &DbState, category_id: i64) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute("UPDATE categories SET is_active = 0 WHERE id = ?1", [category_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_units(db: &DbState) -> Result<Vec<Unit>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, name, short_name, allow_decimals FROM units ORDER BY id ASC")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Unit {
                id: row.get(0)?,
                name: row.get(1)?,
                short_name: row.get(2)?,
                allow_decimals: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<Unit> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

pub fn save_unit(db: &DbState, name: &str, short_name: &str, allow_decimals: bool, unit_id: Option<i64>) -> Result<i64, String> {
    let conn = db.conn.lock().unwrap();
    if let Some(uid) = unit_id {
        conn.execute(
            "UPDATE units SET name = ?1, short_name = ?2, allow_decimals = ?3 WHERE id = ?4",
            rusqlite::params![name, short_name, allow_decimals, uid],
        )
        .map_err(|e| e.to_string())?;
        Ok(uid)
    } else {
        conn.execute(
            "INSERT INTO units (name, short_name, allow_decimals) VALUES (?1, ?2, ?3)",
            rusqlite::params![name, short_name, allow_decimals],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }
}
/// Pin/unpin a product. Pinned products float to the top of the catalog
/// and of their family.
pub fn toggle_product_pin(db: &DbState, product_id: i64, pinned: bool) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    if pinned {
        // New pin goes last among the pinned.
        let max_order: i64 = conn
            .query_row("SELECT COALESCE(MAX(pin_order), 0) FROM products WHERE pinned = 1", [], |r| r.get(0))
            .unwrap_or(0);
        conn.execute(
            "UPDATE products SET pinned = 1, pin_order = ?1 WHERE id = ?2",
            rusqlite::params![max_order + 1, product_id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        conn.execute(
            "UPDATE products SET pinned = 0, pin_order = 0 WHERE id = ?1",
            [product_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Reorder the manually pinned products: ordered_ids is the desired order.
pub fn reorder_pinned_products(db: &DbState, ordered_ids: Vec<i64>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    for (idx, id) in ordered_ids.iter().enumerate() {
        conn.execute(
            "UPDATE products SET pin_order = ?1 WHERE id = ?2 AND pinned = 1",
            rusqlite::params![(idx + 1) as i64, id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Units-in-packaging: carton 24 / fardeau 6 / palette 672 / plateau 30 eggs.
// ---------------------------------------------------------------------------

/// All packagings of a product.
pub fn list_packagings(db: &DbState, product_id: i64) -> Result<Vec<ProductPackaging>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, product_id, name, units_per_package, sale_price,
                    COALESCE(sale_price_per_unit, 0), packaging_type_id, COALESCE(unloading_fee, 0), COALESCE(is_default, 0)
             FROM product_packagings WHERE product_id = ?1 ORDER BY units_per_package ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([product_id], |row| {
            Ok(ProductPackaging {
                id: row.get(0)?,
                product_id: row.get(1)?,
                name: row.get(2)?,
                units_per_package: row.get(3)?,
                sale_price: row.get(4)?,
                sale_price_per_unit: row.get(5)?,
                packaging_type_id: row.get(6)?,
                unloading_fee: row.get(7)?,
                is_default: row.get::<_, i64>(8)? == 1,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Replace a product's packaging definitions (called from the product editor).
pub fn save_packagings(db: &DbState, product_id: i64, inputs: Vec<PackagingInput>) -> Result<(), String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let purchase_price: i64 = tx
        .query_row(
            "SELECT purchase_price FROM products WHERE id = ?1",
            [product_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    write_packagings_tx(&tx, product_id, &inputs, purchase_price)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// Validated packaging write, usable INSIDE another transaction so the
/// product editor can save product + prices atomically (pricing spec §23:
/// never a half-saved product).
///
/// Semantics: the AUTHORITATIVE stored price is `sale_price_per_unit`
/// (DZD per base unit); `sale_price` is rewritten as the derived package
/// total (per_unit × units) — never the other way around. Rows with
/// units_per_package <= 1 or an empty name are "not a packaging" and are
/// skipped (base-unit pricing lives on products.sale_price). A row WITH a
/// conversion but WITHOUT a price is a validation error, never silently
/// dropped or coerced to zero. The same floor as the unit price applies:
/// a per-unit sale price below the product's purchase cost is rejected.
pub(crate) fn write_packagings_tx(
    tx: &rusqlite::Transaction,
    product_id: i64,
    inputs: &[PackagingInput],
    product_purchase_price: i64,
) -> Result<(), String> {
    tx.execute("DELETE FROM product_packagings WHERE product_id = ?1", [product_id])
        .map_err(|e| e.to_string())?;
    let mut seen_names: Vec<String> = Vec::new();
    let mut seen_types: Vec<i64> = Vec::new();
    for input in inputs {
        let name = input.name.trim().to_string();
        if input.units_per_package <= 1 || name.is_empty() {
            continue; // single unit is not a packaging
        }
        if input.units_per_package < 0 {
            return Err(format!("Packaging '{}' has an invalid conversion / تحويل غير صالح للتغليف {}", name, input.units_per_package));
        }
        if input.sale_price_per_unit <= 0 {
            return Err(format!(
                "Packaging '{}' needs a sale price per unit / التغليف {} يحتاج سعر بيع للوحدة",
                name, name
            ));
        }
        if input.unloading_fee < 0 {
            return Err(format!(
                "Packaging '{}' has an invalid unloading fee / رسوم تنزيل غير صالحة للتغليف {}",
                name, name
            ));
        }
        // Same floor as the unit price: never sell a base unit below what
        // it cost, whatever the presentation.
        if product_purchase_price > 0 && input.sale_price_per_unit < product_purchase_price {
            return Err(format!(
                "Sale price per unit for '{}' cannot be less than the purchase cost ({} DZD) / سعر البيع للوحدة للتغليف {} لا يمكن أن يكون أقل من سعر الشراء",
                name, product_purchase_price, name
            ));
        }
        if let Some(tid) = input.packaging_type_id {
            let known: Option<i64> = tx
                .query_row(
                    "SELECT id FROM packaging_types WHERE id = ?1 AND is_active = 1",
                    [tid],
                    |r| r.get(0),
                )
                .ok();
            if known.is_none() {
                return Err(format!(
                    "Packaging type for '{}' does not exist or is inactive / نوع التغليف {} غير موجود أو معطّل",
                    name, name
                ));
            }
            if seen_types.contains(&tid) {
                return Err(format!("Duplicate packaging price for '{}' / سعر مكرر للتغليف {}", name, name));
            }
            seen_types.push(tid);
        }
        let norm = name.to_lowercase();
        if seen_names.contains(&norm) {
            return Err(format!("Duplicate packaging price for '{}' / سعر مكرر للتغليف {}", name, name));
        }
        seen_names.push(norm);

        // Legacy fallback: no type id → resolve by name; unmatched legacy
        // rows keep NULL and keep working by name (nothing is deleted).
        let type_id: Option<i64> = match input.packaging_type_id {
            Some(tid) => Some(tid),
            None => tx
                .query_row(
                    "SELECT id FROM packaging_types WHERE lower(trim(name)) = lower(trim(?1)) LIMIT 1",
                    [&name],
                    |r| r.get(0),
                )
                .ok(),
        };
        let derived_total = input.sale_price_per_unit * input.units_per_package;
        tx.execute(
            "INSERT INTO product_packagings (product_id, name, units_per_package, sale_price, sale_price_per_unit, purchase_price, packaging_type_id, unloading_fee, is_default)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                product_id,
                name,
                input.units_per_package,
                derived_total,
                input.sale_price_per_unit,
                input.purchase_price,
                type_id,
                input.unloading_fee.max(0),
                if input.is_default { 1 } else { 0 }
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Compact, order-stable snapshot of a product's packaging prices for the
/// price history: [{"name":"Palette","per_unit":190,"fee":50}, …] sorted by
/// conversion so two snapshots compare equal iff the pricing is identical.
pub(crate) fn packagings_snapshot(tx: &rusqlite::Transaction, product_id: i64) -> Result<String, String> {
    let mut stmt = tx
        .prepare(
            "SELECT name, COALESCE(sale_price_per_unit, 0), COALESCE(unloading_fee, 0)
             FROM product_packagings WHERE product_id = ?1
             ORDER BY units_per_package ASC, name ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([product_id], |r| {
            Ok(serde_json::json!({
                "name": r.get::<_, String>(0)?,
                "per_unit": r.get::<_, i64>(1)?,
                "fee": r.get::<_, i64>(2)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    let mut list: Vec<serde_json::Value> = Vec::new();
    for row in rows {
        list.push(row.map_err(|e| e.to_string())?);
    }
    Ok(serde_json::to_string(&list).map_err(|e| e.to_string())?)
}

/// Quantity history for a product: every movement (sales, purchases,
/// refunds, manual edits, broken) as old → new with the signed delta and the
/// acting user. Derived from inventory_movements by replaying the deltas in
/// chronological order (oldest first) starting from the current stock.
pub fn get_quantity_history(db: &DbState, product_id: i64) -> Result<Vec<QuantityHistoryEntry>, String> {
    let conn = db.conn.lock().unwrap();

    // Current stock is the sum of all movements, so walking backwards from
    // it reconstructs the running level at each event.
    let current: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(quantity), current_stock) FROM inventory_movements WHERE product_id = ?1",
            [product_id],
            |r| r.get(0),
        )
        .or_else(|_| {
            conn.query_row(
                "SELECT current_stock FROM products WHERE id = ?1",
                [product_id],
                |r| r.get(0),
            )
        })
        .map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.quantity, m.type, m.created_at, m.notes,
                    COALESCE(u.display_name, u.username)
             FROM inventory_movements m
             LEFT JOIN users u ON u.id = m.user_id
             WHERE m.product_id = ?1
             ORDER BY m.created_at ASC, m.id ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([product_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut raw: Vec<(i64, f64, String, String, Option<String>, Option<String>)> = Vec::new();
    for row in rows {
        raw.push(row.map_err(|e| e.to_string())?);
    }

    // Replay backwards: the level AFTER the last event is `current`.
    let mut after = current;
    let mut entries: Vec<QuantityHistoryEntry> = Vec::with_capacity(raw.len());
    for (id, delta, mtype, created_at, notes, user_name) in raw.iter().rev() {
        let before = after - delta;
        entries.push(QuantityHistoryEntry {
            id: *id,
            movement_type: mtype.clone(),
            old_quantity: before,
            new_quantity: after,
            difference: *delta,
            user_name: user_name.clone(),
            created_at: created_at.clone(),
            notes: notes.clone(),
        });
        after = before;
    }
    entries.reverse();
    Ok(entries)
}

#[cfg(test)]
mod save_product_tests {
    use super::*;
    use crate::database::DbState;

    fn fresh_db(tag: &str) -> DbState {
        let dir = std::env::temp_dir().join("titaou_product_tests");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("prod_{}_{}.sqlite", tag, std::process::id()));
        let _ = std::fs::remove_file(&path);
        let state = DbState { conn: std::sync::Mutex::new(rusqlite::Connection::open(&path).unwrap()) };
        state.run_migrations().unwrap();
        state
    }

    fn base_input(sku: &str) -> ProductInput {
        ProductInput {
            sku: Some(sku.to_string()),
            name_ar: "ماء".into(),
            name_fr: "Eau minérale 0.33L".into(),
            name_en: "Water 0.33L".into(),
            category_id: None,
            unit_id: None,
            purchase_price: 3000,
            sale_price: 5000,
            min_sale_price: 4500,
            tax_rate: 19,
            current_stock: 0.0,
            min_stock: 5.0,
            image_path: None,
            expiry_date: None,
            unloading_fee: 0,
            is_scalable: false,
            scale_code: None,
            scale_plu: None,
            scale_barcode_type: None,
            scale_department_id: None,
            scale_sync_status: None,
            is_bundle: false,
            packagings: None,
            barcodes: vec![],
        }
    }

    fn round_trip(db: &DbState, id: i64) -> (i64, i64, i64) {
        let conn = db.conn.lock().unwrap();
        conn.query_row(
            "SELECT sale_price, min_sale_price, tax_rate FROM products WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
    }

    /// Regression for the field bug "got 19 parameters, needed 22": the INSERT
    /// skipped sale_price/min_sale_price/tax_rate, so product CREATION always
    /// failed and the saved row lost its pricing. All three values must
    /// round-trip.
    #[test]
    fn a_create_base_unit_only_persists_pricing() {
        let db = fresh_db("a");
        let id = save_product(&db, base_input("PROD-A"), None, Some(1)).unwrap();
        assert_eq!(round_trip(&db, id), (5000, 4500, 19));
    }

    #[test]
    fn b_create_with_one_packaging() {
        let db = fresh_db("b");
        let id = save_product(&db, base_input("PROD-B"), None, Some(1)).unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO product_packagings (product_id, name, units_per_package, sale_price)
                 VALUES (?1, 'Pack 6', 6, 27000)",
                rusqlite::params![id],
            )
            .unwrap();
        }
        assert_eq!(round_trip(&db, id), (5000, 4500, 19));
        let conn = db.conn.lock().unwrap();
        let (name, upp): (String, f64) = conn
            .query_row(
                "SELECT name, units_per_package FROM product_packagings WHERE product_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((name.as_str(), upp), ("Pack 6", 6.0));
    }

    #[test]
    fn c_create_with_multiple_packaging_levels() {
        let db = fresh_db("c");
        let id = save_product(&db, base_input("PROD-C"), None, Some(1)).unwrap();
        {
            let conn = db.conn.lock().unwrap();
            for (name, upp, price) in
                [("Pack 6", 6.0, 27000), ("Fardeau 24", 24.0, 105000), ("Palette 150", 150.0, 640000)]
            {
                        conn.execute(
                    "INSERT INTO product_packagings (product_id, name, units_per_package, sale_price)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![id, name, upp, price],
                )
                .unwrap();
            }
        }
        let conn = db.conn.lock().unwrap();
        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM product_packagings WHERE product_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 3, "three packaging levels must persist");
        drop(conn);
        assert_eq!(round_trip(&db, id), (5000, 4500, 19));
    }

    // ── Pricing-model refinement tests (per-base-unit packaging prices) ──

    /// The spec's exact product: Water @ 200/unit, Palette 112 @ 190/unit →
    /// per-unit stored, package total DERIVED (190 × 112 = 21 280), never
    /// the other way around.
    #[test]
    fn d_packaging_price_stored_per_base_unit_total_derived() {
        let db = fresh_db("d");
        let mut input = base_input("PROD-D");
        input.purchase_price = 180;
        input.sale_price = 200;
        input.packagings = Some(vec![PackagingInput {
            name: "Palette PL112".into(),
            units_per_package: 112,
            sale_price: 0,
            sale_price_per_unit: 190,
            purchase_price: 0,
            packaging_type_id: Some(4), // Palette seed
            unloading_fee: 50,
            is_default: false,
        }]);
        let id = save_product(&db, input, None, Some(1)).unwrap();
        let packs = list_packagings(&db, id).unwrap();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].sale_price_per_unit, 190, "authoritative per-base-unit price");
        assert_eq!(packs[0].sale_price, 21280, "per-package total must be DERIVED 190×112");
        assert_eq!(packs[0].packaging_type_id, Some(4));
        assert_eq!(packs[0].unloading_fee, 50, "per-packaging unloading fee round-trips");
    }

    /// Atomic save (pricing spec §23): a packaging row sent with the product
    /// is persisted by save_product itself in ONE transaction — the caller
    /// no longer needs a second save_packagings round-trip.
    #[test]
    fn e_packagings_persist_atomically_with_product() {
        let db = fresh_db("e");
        let mut input = base_input("PROD-E");
        input.packagings = Some(vec![PackagingInput {
            name: "Carton".into(),
            units_per_package: 12,
            sale_price: 0,
            sale_price_per_unit: 3050,
            purchase_price: 0,
            unloading_fee: 0,
            packaging_type_id: Some(3),
            is_default: false,
        }]);
        let id = save_product(&db, input, None, Some(1)).unwrap();
        assert_eq!(list_packagings(&db, id).unwrap().len(), 1, "packagings saved with the product in one call");
    }

    /// Validation (pricing spec §24): a configured conversion without a
    /// price is rejected — never silently dropped or coerced to zero.
    #[test]
    fn f_packaging_without_price_rejected() {
        let db = fresh_db("f");
        let mut input = base_input("PROD-F");
        input.packagings = Some(vec![PackagingInput {
            name: "Sac".into(),
            units_per_package: 50,
            sale_price: 0,
            sale_price_per_unit: 0,
            purchase_price: 0,
            unloading_fee: 0,
            packaging_type_id: Some(5),
            is_default: false,
        }]);
        let err = save_product(&db, input, None, Some(1)).unwrap_err();
        assert!(err.to_lowercase().contains("sale price per unit"), "clear validation error: {}", err);
        // And the product must NOT have been half-saved:
        let conn = db.conn.lock().unwrap();
        let cnt: i64 = conn.query_row("SELECT COUNT(*) FROM products WHERE sku = 'PROD-F'", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 0, "invalid pricing must roll the whole save back");
    }

    /// The unit-price floor applies per row: a packaging sale price per
    /// unit below the product's purchase cost is rejected (field rule:
    /// "the sale price should not be less than the purchase price").
    #[test]
    fn f2_packaging_price_below_purchase_cost_rejected() {
        let db = fresh_db("f2");
        let mut input = base_input("PROD-F2"); // purchase 3000, sale 5000
        input.packagings = Some(vec![PackagingInput {
            name: "Fardeau".into(),
            units_per_package: 6,
            sale_price: 0,
            sale_price_per_unit: 2900, // below 3000 purchase cost
            purchase_price: 0,
            packaging_type_id: Some(2),
            unloading_fee: 0,
            is_default: false,
        }]);
        let err = save_product(&db, input, None, Some(1)).unwrap_err();
        assert!(err.contains("cannot be less than the purchase cost"), "{}", err);
        let conn = db.conn.lock().unwrap();
        let cnt: i64 = conn.query_row("SELECT COUNT(*) FROM products WHERE sku = 'PROD-F2'", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 0, "the whole save must roll back");
    }

    /// Validation (pricing spec §24): duplicate active packaging prices for
    /// the same product/type are rejected.
    #[test]
    fn g_duplicate_packaging_type_rejected() {
        let db = fresh_db("g");
        let mk = |name: &str, per_unit: i64| PackagingInput {
            name: name.into(),
            units_per_package: 12,
            sale_price: 0,
            sale_price_per_unit: per_unit,
            purchase_price: 0,
            unloading_fee: 0,
            packaging_type_id: Some(3),
            is_default: false,
        };
        let mut input = base_input("PROD-G");
        input.packagings = Some(vec![mk("Carton", 195), mk("Carton bis", 190)]);
        assert!(save_product(&db, input, None, Some(1)).is_err(), "duplicate type pricing must be rejected");
    }

    /// Legacy legacy rows (name copy, no type id, per-package price only)
    /// are backfilled on migration: per-unit derived, type resolved by name.
    #[test]
    fn h_legacy_packaging_rows_backfilled() {
        let db = fresh_db("h");
        {
            let conn = db.conn.lock().unwrap();
            // Insert BEFORE... the migration block runs inside run_migrations,
            // so simulate a legacy row by clearing the backfilled columns.
            conn.execute_batch(
                "INSERT INTO products (sku, name_ar, name_fr, name_en, purchase_price, sale_price) VALUES ('PROD-H','ماء','Eau','Water',180,200);
                 INSERT INTO product_packagings (product_id, name, units_per_package, sale_price)
                   VALUES ((SELECT id FROM products WHERE sku='PROD-H'), 'Fardeau', 6, 1170);
                 UPDATE product_packagings SET sale_price_per_unit = 0, packaging_type_id = NULL;",
            )
            .unwrap();
            // Re-run just the backfill statements the migration applies.
            conn.execute(
                "UPDATE product_packagings
                    SET sale_price_per_unit = CAST(round(sale_price * 1.0 / units_per_package) AS INTEGER)
                  WHERE (sale_price_per_unit IS NULL OR sale_price_per_unit = 0)
                    AND sale_price > 0 AND units_per_package > 1;",
                [],
            )
            .unwrap();
            conn.execute(
                "UPDATE product_packagings
                    SET packaging_type_id = (SELECT pt.id FROM packaging_types pt WHERE lower(trim(pt.name)) = lower(trim(product_packagings.name)) LIMIT 1)
                  WHERE packaging_type_id IS NULL;",
                [],
            )
            .unwrap();
        }
        let conn = db.conn.lock().unwrap();
        let (per_unit, type_id): (i64, Option<i64>) = conn
            .query_row(
                "SELECT sale_price_per_unit, packaging_type_id FROM product_packagings WHERE name = 'Fardeau'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(per_unit, 195, "1170/6 = 195 per base unit");
        assert_eq!(type_id, Some(2), "legacy name matched to the Fardeau type");
    }

    /// Unit protection (pricing spec §44): the system 'Unité' cannot be
    /// renamed, deactivated or deleted; other types need the admin password
    /// server-side.
    #[test]
    fn i_system_unit_protected_and_admin_gated() {
        let db = fresh_db("i");
        // Unité is seeded is_system by the migration.
        let sys_flag: i64 = {
            let conn = db.conn.lock().unwrap();
            conn.query_row("SELECT is_system FROM packaging_types WHERE id = 1", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(sys_flag, 1, "Unité must be marked as a protected system packaging");

        // Rename attempt → rejected.
        let err = save_packaging_type(&db, Some(1), "Bouteille".into(), "B".into(), 0, true, 1, "admin".into()).unwrap_err();
        assert!(err.contains("protected"), "rename must be rejected: {}", err);
        // Deactivate attempt (even with the right name) → stays active.
        save_packaging_type(&db, Some(1), "Unité".into(), "U".into(), 0, false, 1, "admin".into()).unwrap();
        let active: i64 = {
            let conn = db.conn.lock().unwrap();
            conn.query_row("SELECT is_active FROM packaging_types WHERE id = 1", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(active, 1, "Unité cannot be deactivated");
        // Delete attempt → rejected.
        assert!(delete_packaging_type(&db, 1, "admin".into()).is_err(), "Unité cannot be deleted");

        // Ordinary type mutations require the admin password server-side.
        assert!(save_packaging_type(&db, None, "Bidon".into(), "B".into(), 9, true, 24, "wrong".into()).is_err(), "wrong password rejected");
        let new_id = save_packaging_type(&db, None, "Bidon".into(), "B".into(), 9, true, 24, "admin".into()).unwrap();
        assert!(delete_packaging_type(&db, new_id, "wrong".into()).is_err(), "delete without admin password rejected");
        delete_packaging_type(&db, new_id, "admin".into()).unwrap();
    }
}

// ── Packaging TYPE templates (Settings) ─────────────────────────────────────

/// All packaging types (Settings → Packaging), ordered for display.
pub fn get_packaging_types(db: &DbState, active_only: bool) -> Result<Vec<serde_json::Value>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, name, abbreviation, display_order, is_active, COALESCE(is_system, 0), COALESCE(default_units, 0) FROM packaging_types {} ORDER BY display_order ASC, id ASC",
            if active_only { "WHERE is_active = 1" } else { "" }
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "name": r.get::<_, String>(1)?,
                "abbreviation": r.get::<_, String>(2)?,
                "display_order": r.get::<_, i64>(3)?,
                "is_active": r.get::<_, i64>(4)? != 0,
                "is_system": r.get::<_, i64>(5)? != 0,
                "default_units": r.get::<_, i64>(6)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn require_admin(db: &DbState, admin_password: &str) -> Result<(), String> {
    if !crate::auth::verify_admin_password(db, admin_password).unwrap_or(false) {
        return Err("Mot de passe administrateur incorrect / كلمة مرور المدير غير صحيحة / Incorrect admin password".into());
    }
    Ok(())
}

/// Create or update a packaging type template. Sensitive settings mutation:
/// requires the admin password, verified SERVER-SIDE (same mechanism as the
/// sales-mode switch and the danger zone). The base 'Unité' is a protected
/// SYSTEM packaging: its name, abbreviation, active state and conversion
/// (always 1) are immutable — only its display order may change.
/// `default_units` is the suggested conversion (prefill) for pricing rows
/// and purchase lines — per-product values still override it.
pub fn save_packaging_type(
    db: &DbState,
    id: Option<i64>,
    name: String,
    abbreviation: String,
    display_order: i64,
    is_active: bool,
    default_units: i64,
    admin_password: String,
) -> Result<i64, String> {
    require_admin(db, &admin_password)?;
    if default_units < 0 {
        return Err("Invalid default conversion / تحويل افتراضي غير صالح".into());
    }
    let conn = db.conn.lock().unwrap();
    match id {
        Some(existing) => {
            let (old_name, is_system): (String, bool) = conn
                .query_row(
                    "SELECT name, COALESCE(is_system, 0) FROM packaging_types WHERE id = ?1",
                    [existing],
                    |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)),
                )
                .map_err(|e| e.to_string())?;
            if is_system {
                // Identity is locked: same name required, always active,
                // conversion stays 1.
                if old_name.trim().to_lowercase() != name.trim().to_lowercase() {
                    return Err(format!(
                        "'{}' is a protected system packaging — it cannot be renamed / التغليف الأساسي محمي ولا يمكن تغيير اسمه",
                        old_name
                    ));
                }
                conn.execute(
                    "UPDATE packaging_types SET display_order = ?1, default_units = 1, is_active = 1 WHERE id = ?2",
                    rusqlite::params![display_order, existing],
                )
                .map_err(|e| e.to_string())?;
            } else {
                conn.execute(
                    "UPDATE packaging_types SET name = ?1, abbreviation = ?2, display_order = ?3, is_active = ?4, default_units = ?5 WHERE id = ?6",
                    rusqlite::params![name.trim(), abbreviation.trim(), display_order, is_active as i64, default_units, existing],
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(existing)
        }
        None => {
            conn.execute(
                "INSERT INTO packaging_types (name, abbreviation, display_order, is_active, default_units) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![name.trim(), abbreviation.trim(), display_order, is_active as i64, default_units],
            )
            .map_err(|e| e.to_string())?;
            Ok(conn.last_insert_rowid())
        }
    }
}

/// Remove a packaging type template. Products keep their saved packaging rows
/// (they store name + conversion copies), so history is never broken.
/// Admin-protected server-side; the base 'Unité' cannot be deleted.
pub fn delete_packaging_type(db: &DbState, id: i64, admin_password: String) -> Result<(), String> {
    require_admin(db, &admin_password)?;
    let conn = db.conn.lock().unwrap();
    let is_system: bool = conn
        .query_row(
            "SELECT COALESCE(is_system, 0) FROM packaging_types WHERE id = ?1",
            [id],
            |r| Ok(r.get::<_, i64>(0)? != 0),
        )
        .unwrap_or(false);
    if is_system {
        return Err("Unité is a protected system packaging and cannot be deleted / لا يمكن حذف الوحدة الأساسية".into());
    }
    conn.execute("DELETE FROM packaging_types WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
