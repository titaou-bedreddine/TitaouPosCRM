import io

def edit(path, subs):
    with io.open(path, 'r', encoding='utf-8') as f:
        s = f.read()
    for old, new, label in subs:
        if old not in s:
            print('MISS', label)
            continue
        s = s.replace(old, new, 1)
        print('ok:', label)
    with io.open(path, 'w', encoding='utf-8', newline='') as f:
        f.write(s)

# ============ scale barcode parse service ============
with io.open('src-tauri/src/services/scale_service.rs', 'r', encoding='utf-8') as f:
    sc = f.read()
addition = '''

/// Parse a scanned scale barcode into (item_code, weight, unit_price) per
/// the CONFIGURED barcode format. Supported: Type 97 (18-code), Type 2/22/12
/// (price-embedded) and Type 7/27/17 (weight-embedded). EAN-13 style only.
pub fn parse_scale_barcode_code(code: &str, barcode_type: i64) -> Option<(String, f64, i64)> {
    let digits: String = code.trim().chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 13 {
        return None;
    }
    let d: Vec<char> = digits.chars().collect();
    let num = |from: usize, len: usize| -> String {
        d.get(from..from + len)?.iter().collect::<String>()
    };
    match barcode_type {
        // Type 97: DD IIIII PPPPP WWWWW C  (dept 2, item 5, price 5, weight 5)
        97 => {
            let item = num(2, 5)?;
            let price: i64 = num(7, 5)?.parse().ok()?;
            let weight_raw: f64 = num(12, 5)?.parse().ok()?;
            let weight = weight_raw / 1000.0; // grams → kg
            Some((item, weight, price))
        }
        // Type 2:  DD IIIII PPPPP C   (price ×100)
        2 => {
            let item = num(2, 5)?;
            let price: i64 = num(7, 5)?.parse().ok()?;
            Some((item, 0.0, price))
        }
        // Type 7:  DD IIIII WWWWW C   (weight grams)
        7 => {
            let item = num(2, 5)?;
            let weight_raw: f64 = num(7, 5)?.parse().ok()?;
            Some((item, weight_raw / 1000.0, 0))
        }
        // Type 22: D IIIIII PPPPP C
        22 => {
            let item = num(1, 6)?;
            let price: i64 = num(7, 5)?.parse().ok()?;
            Some((item, 0.0, price))
        }
        // Type 27: D IIIIII WWWWW C
        27 => {
            let item = num(1, 6)?;
            let weight_raw: f64 = num(7, 5)?.parse().ok()?;
            Some((item, weight_raw / 1000.0, 0))
        }
        // Type 12: 22 IIIII PPPPP C
        12 => {
            let item = num(2, 5)?;
            let price: i64 = num(7, 5)?.parse().ok()?;
            Some((item, 0.0, price))
        }
        // Type 17: 22 IIIII WWWWW C
        17 => {
            let item = num(2, 5)?;
            let weight_raw: f64 = num(7, 5)?.parse().ok()?;
            Some((item, weight_raw / 1000.0, 0))
        }
        _ => None,
    }
}

/// Resolve a scanned scale barcode against the product catalog: returns the
/// product with the scanned WEIGHT as quantity (price from the barcode when
/// embedded, else the product's shelf price).
pub fn resolve_scale_scan(
    db: &crate::database::DbState,
    scanned: &str,
) -> Option<crate::models::Product> {
    let settings = crate::services::settings_service::get_all_settings(db).ok()?;
    let btype: i64 = settings
        .get("scale_default_barcode_type")
        .and_then(|v| v.parse().ok())
        .unwrap_or(97);
    let (item_code, _weight, _price) = parse_scale_barcode_code(scanned, btype)?;

    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, sku, name_ar, name_fr, name_en, category_id, NULL, unit_id, NULL,
                    purchase_price, sale_price, min_sale_price, tax_rate, current_stock, min_stock,
                    NULL, NULL, is_scalable, scale_code, scale_plu, scale_barcode_type,
                    scale_department_id, scale_sync_status, is_bundle, is_active, NULL,
                    COALESCE(pinned, 0), COALESCE(pin_order, 0)
             FROM products
             WHERE is_scalable = 1 AND (scale_code = ?1 OR CAST(scale_plu AS TEXT) = ?1 OR sku = ?1)
             LIMIT 1",
        )
        .ok()?;
    let mut rows = stmt
        .query_map([&item_code], |row| {
            Ok(crate::models::Product {
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
                is_scalable: row.get(18)?,
                scale_code: row.get(19)?,
                scale_plu: row.get(20)?,
                scale_barcode_type: row.get(21)?,
                scale_department_id: row.get(22)?,
                scale_sync_status: row.get(23)?,
                is_bundle: row.get(24)?,
                is_active: row.get(25)?,
                barcodes: vec![],
                total_sold: None,
                pinned: row.get::<_, i64>(26)? == 1,
                pin_order: row.get(27)?,
            })
        })
        .ok()?;
    rows.next().map(|r| r.ok()).flatten()
}
'''
if 'pub fn resolve_scale_scan' not in sc:
    sc = sc.rstrip() + addition
    with io.open('src-tauri/src/services/scale_service.rs', 'w', encoding='utf-8', newline='') as f:
        f.write(sc)
    print('ok: scale_service additions')

# ============ commands: parse_scale_barcode, login_with_rfid, print_escpos_raw ============
edit('src-tauri/src/commands/mod.rs', [
    ('''#[tauri::command]
pub fn find_employee_by_rfid(db: State<'_, DbState>, rfid: String) -> Result<Option<Employee>, String> {
    employee_service::find_employee_by_rfid(&db, &rfid)
}''',
     '''#[tauri::command]
pub fn find_employee_by_rfid(db: State<'_, DbState>, rfid: String) -> Result<Option<Employee>, String> {
    employee_service::find_employee_by_rfid(&db, &rfid)
}

/// RFID login: resolve a scanned card to the employee's active user account.
#[tauri::command]
pub fn login_with_rfid(db: State<'_, DbState>, rfid: String) -> Result<Option<User>, String> {
    employee_service::login_with_rfid(&db, &rfid)
}

/// Resolve a scanned scale barcode (ACLAS price/weight-embedded EAN) to the
/// scalable product. The WEIGHT rides back as the suggested quantity.
#[tauri::command]
pub fn resolve_scale_scan(db: State<'_, DbState>, code: String) -> Result<Option<ScaleScanResult>, String> {
    let settings = settings_service::get_all_settings(&db)?;
    let btype: i64 = settings
        .get("scale_default_barcode_type")
        .and_then(|v| v.parse().ok())
        .unwrap_or(97);
    let Some((item_code, weight, price)) =
        scale_service::parse_scale_barcode_code(&code, btype)
    else {
        return Ok(None);
    };
    let Some(product) = scale_service::resolve_scale_scan(&db, &code) else {
        return Ok(None);
    };
    Ok(Some(ScaleScanResult {
        product_id: product.id,
        name: product.name_fr.clone(),
        unit_price: if price > 0 { price } else { product.sale_price },
        weight,
    }))
}

#[derive(Debug, serde::Serialize)]
pub struct ScaleScanResult {
    pub product_id: i64,
    pub name: Option<String>,
    pub unit_price: i64,
    pub weight: f64,
}

/// Browser-free printing fallback: send a ready-made ESC/POS byte payload
/// (text commands, cut) RAW to the Windows spooler — no driver dialog, no
/// browser, no PDF. Works on any thermal receipt printer even on machines
/// with NO Chrome/Edge installed.
#[tauri::command]
pub fn print_escpos_raw(payload: String, printer: Option<String>) -> Result<(), String> {
    crate::printing::escpos::print_raw(&payload, printer.as_deref())
}''',
     'new commands'),
])

# ============ register commands ============
edit('src-tauri/src/lib.rs', [
    ("            commands::find_employee_by_rfid,",
     "            commands::find_employee_by_rfid,\n            commands::login_with_rfid,\n            commands::resolve_scale_scan,\n            commands::print_escpos_raw,")
])
print('lib registered')
print('backend batch B done')
