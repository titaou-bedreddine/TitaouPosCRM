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
                p.is_bundle, p.is_active, COALESCE(p.pinned, 0), COALESCE(p.pin_order, 0)
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

        if let Some((old_pur, old_sale)) = old_prices {
            if old_pur != input.purchase_price || old_sale != input.sale_price {
                let _ = tx.execute(
                    "INSERT INTO product_price_history (product_id, old_purchase_price, new_purchase_price, old_sale_price, new_sale_price)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![pid, old_pur, input.purchase_price, old_sale, input.sale_price],
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
                scale_sync_status = ?20, is_bundle = ?21
             WHERE id = ?22",
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
                scale_department_id, scale_sync_status, is_bundle, is_active
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, 1)",
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
                input.is_bundle
            ],
        )
        .map_err(|e| e.to_string())?;

        let new_id = tx.last_insert_rowid();

        let _ = tx.execute(
            "INSERT INTO product_price_history (product_id, old_purchase_price, new_purchase_price, old_sale_price, new_sale_price)
             VALUES (?1, 0, ?2, 0, ?3)",
            rusqlite::params![new_id, input.purchase_price, input.sale_price],
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

    // Cloud sync outbox (same transaction): product create/update →
    // upsert_pos_product; a manual stock edit additionally becomes a
    // stock_adjustment event (append-only ledger on the CRM side).
    {
        let primary_barcode = clean_barcodes.first().cloned();
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
            "SELECT id, product_id, name, units_per_package, sale_price, COALESCE(is_default, 0)
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
                is_default: row.get::<_, i64>(5)? == 1,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Replace a product's packaging definitions (called from the product editor).
pub fn save_packagings(db: &DbState, product_id: i64, inputs: Vec<PackagingInput>) -> Result<(), String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM product_packagings WHERE product_id = ?1", [product_id])
        .map_err(|e| e.to_string())?;
    for input in &inputs {
        if input.units_per_package <= 1 || input.name.trim().is_empty() {
            continue; // single unit is not a packaging
        }
        tx.execute(
            "INSERT INTO product_packagings (product_id, name, units_per_package, sale_price, is_default)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                product_id,
                input.name.trim(),
                input.units_per_package,
                input.sale_price,
                if input.is_default { 1 } else { 0 }
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
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
