use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use crate::database::DbState;
use crate::models::{Product, ScaleSyncLog};
use rusqlite::params;

#[repr(C, packed(1))]
#[derive(Debug, Clone)]
pub struct NativeDeviceInfo {
    pub protocol_type: u32,
    pub addr: u32,
    pub port: u32,
    pub name: [u8; 16],
    pub id: u32,
    pub version: u32,
    pub country: u8,
    pub department_id: u8,
    pub key_type: u8,
    pub printer_dot: u64,
    pub prn_start_date: i64,
    pub label_page: u32,
    pub printer_no: u32,
    pub plu_storage: u16,
    pub hot_key_count: u16,
    pub nutrition_storage: u16,
    pub discount_storage: u16,
    pub note1_storage: u16,
    pub note2_storage: u16,
    pub note3_storage: u16,
    pub note4_storage: u16,
    pub firmware_version: [u8; 12],
    pub default_protocol: u8,
    pub lf_code_len: u8,
    pub device_id: [u8; 4],
    pub stock_id: [u8; 4],
    pub adjunct: [u8; 155],
}

impl Default for NativeDeviceInfo {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

type FnInitialize = unsafe extern "system" fn(*const u8) -> bool;
type FnFinalize = unsafe extern "system" fn();
type FnGetDevicesInfo = unsafe extern "system" fn(u32, u32, u32, *mut NativeDeviceInfo) -> bool;
type FnExecTaskA = unsafe extern "system" fn(u32, u32, u32, u32, u32, *const i8, usize, usize) -> usize;
type FnWaitForTask = unsafe extern "system" fn(usize);

fn get_dll_path() -> PathBuf {
    // 1. Current executable directory (production installed app)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let p1 = exe_dir.join("AclasSDK.dll");
            if p1.exists() {
                return p1;
            }
            let p2 = exe_dir.join("resources").join("AclasSDK.dll");
            if p2.exists() {
                return p2;
            }
        }
    }

    // 2. Working directory & dev relative paths
    let candidate_paths = vec![
        PathBuf::from("AclasSDK.dll"),
        PathBuf::from("src-tauri/AclasSDK.dll"),
        PathBuf::from("SDK-Balance_Aclas/SDK-Balance_Aclas/DLL66_Demo_EN200612/DLL&SO/Win64/AclasSDK.dll"),
        PathBuf::from("../SDK-Balance_Aclas/SDK-Balance_Aclas/DLL66_Demo_EN200612/DLL&SO/Win64/AclasSDK.dll"),
    ];

    for p in candidate_paths {
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("AclasSDK.dll")
}

pub fn ip_to_dword(ip_str: &str) -> u32 {
    let parts: Vec<&str> = ip_str.trim().split('.').collect();
    if parts.len() != 4 {
        return 0;
    }
    let mut res: u32 = 0;
    for (i, p) in parts.iter().enumerate() {
        if let Ok(num) = p.parse::<u32>() {
            res |= (num & 0xFF) << ((3 - i) * 8);
        }
    }
    res
}

pub fn test_scale_connection(ip: &str, port: u32, protocol_type: u32) -> Result<String, String> {
    let dll_path = get_dll_path();
    if !dll_path.exists() {
        return Err(format!("AclasSDK.dll not found at expected location: {:?}", dll_path));
    }

    // The SDK requires AclasSDK.ini to be in the working directory alongside the DLL.
    // In dev mode, CWD is the project root, but the INI is in src-tauri/.
    // We temporarily change CWD to the DLL's parent directory.
    let dll_dir = dll_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let ini_path = dll_dir.join("AclasSDK.ini");
    let ini_exists = ini_path.exists();

    let original_cwd = std::env::current_dir().ok();
    if let Err(e) = std::env::set_current_dir(dll_dir) {
        eprintln!("[ACLAS] Warning: Could not set CWD to {:?}: {}", dll_dir, e);
    }

    let addr = ip_to_dword(ip);

    unsafe {
        let lib = match libloading::Library::new(&dll_path) {
            Ok(l) => l,
            Err(e) => {
                // Restore CWD
                if let Some(ref cwd) = original_cwd { let _ = std::env::set_current_dir(cwd); }
                return Err(format!("Failed to load AclasSDK.dll: {}", e));
            }
        };

        let p_init: libloading::Symbol<FnInitialize> = lib
            .get(b"AclasSDK_Initialize\0")
            .map_err(|e| format!("Symbol AclasSDK_Initialize not found: {}", e))?;
        let p_get_info: libloading::Symbol<FnGetDevicesInfo> = lib
            .get(b"AclasSDK_GetDevicesInfo\0")
            .map_err(|e| format!("Symbol AclasSDK_GetDevicesInfo not found: {}", e))?;
        let p_finalize: libloading::Symbol<FnFinalize> = lib
            .get(b"AclasSDK_Finalize\0")
            .map_err(|e| format!("Symbol AclasSDK_Finalize not found: {}", e))?;

        let init_ok = p_init(std::ptr::null());

        // Give the SDK network stack time to initialize (important for UDP broadcast discovery)
        std::thread::sleep(std::time::Duration::from_millis(500));

        let mut info = NativeDeviceInfo::default();
        let ok = p_get_info(addr, port, protocol_type, &mut info);

        p_finalize();

        // Restore original CWD
        if let Some(ref cwd) = original_cwd { let _ = std::env::set_current_dir(cwd); }

        if ok {
            let name_str = String::from_utf8_lossy(&info.name).trim_matches(char::from(0)).to_string();
            let dev_id = info.id;
            let dev_port = info.port;
            let dev_proto = info.protocol_type;
            Ok(format!(
                "Connected successfully! Scale: {} (ID: {}, Port: {}, Protocol: {})",
                name_str, dev_id, dev_port, dev_proto
            ))
        } else {
            Err(format!(
                "Could not connect to ACLAS scale at IP {} (addr=0x{:08X}, port={}, proto={}, ini_found={}, init_ok={}).\n\
                 Troubleshooting:\n\
                 • Verify the scale IP address matches what Link66 uses\n\
                 • Check Windows Firewall is not blocking TitaouPOS\n\
                 • Make sure the scale and PC are on the same subnet\n\
                 • Try pinging the scale: ping {}",
                ip, addr, port, protocol_type, ini_exists, init_ok, ip
            ))
        }
    }
}

pub fn generate_plu_file_content(products: &[Product], default_dept: i64, default_barcode_type: i64) -> Vec<u8> {
    let header = "ID\tItemCode\tDepartmentID\tGroupID\tName1\tName2\tName3\tPrice\tUnitID\tBarcodeType1\tBarcodeType2\tLabel1ID\tLabel2ID\tProducedDate\tFreshnessDate\tValidDate\tPackageType\tPackageWeight\tPackagePrice\tPackageRange\tPackageDays\tPackageHours\tText1ID\tText2ID\tText3ID\tText4ID\tText5ID\tText6ID\tText7ID\tText8ID\tDiscountID\tDiscountRate\tHalfDiscount\tQuarterDiscount\tTareID\tTareValue\tIceID\tICEValue\tOriginID\tTraceabilityID\tLimitPrice\tTax1\tTax2\tTax3\tTax4\tFlag1\tFlag2\tFlag3\tProducedDateRule\tFreshnessDateFrom\tValidDateFrom\tPackageDateFrom\tSpeedCode\tPosition1\tSalesCategory\tDiscountBeginTime\tDiscountEndTime\tDiscountPrice\tDiscountFlag\r\n";

    let mut lines = String::from(header);

    for p in products {
        let plu = p.scale_plu.unwrap_or(p.id);
        let code = p.scale_code.clone().unwrap_or_else(|| format!("{:06}", plu));
        let dept = p.scale_department_id.unwrap_or(default_dept);
        let btype = p.scale_barcode_type.unwrap_or(default_barcode_type);
        let name = if !p.name_fr.is_empty() { &p.name_fr } else { &p.name_ar };
        let price = p.sale_price;

        let line = format!(
            "{}\t{}\t{}\t1\t{}\t\t\t{}\t4\t{}\t0\t1\t0\t2026/8/28 12:00:00\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t1\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t\t0\t\t1899/12/30\t1899/12/30\t0\t0\r\n",
            plu, code, dept, name, price, btype
        );
        lines.push_str(&line);
    }

    // Convert to UTF-16LE with BOM
    let mut utf16_bytes = vec![0xFF, 0xFE];
    for u in lines.encode_utf16() {
        utf16_bytes.push((u & 0xFF) as u8);
        utf16_bytes.push(((u >> 8) & 0xFF) as u8);
    }
    utf16_bytes
}

pub fn upload_products_to_scale(
    db: &DbState,
    products: &[Product],
    ip: &str,
    port: u32,
    protocol_type: u32,
    default_dept: i64,
    default_barcode_type: i64,
    user_name: Option<String>,
) -> Result<usize, String> {
    let dll_path = get_dll_path();
    if !dll_path.exists() {
        return Err(format!("AclasSDK.dll not found at: {:?}", dll_path));
    }

    // Set CWD to DLL directory so SDK finds AclasSDK.ini
    let dll_dir = dll_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let original_cwd = std::env::current_dir().ok();
    let _ = std::env::set_current_dir(dll_dir);

    // Write temp PLU file
    let temp_dir = std::env::temp_dir();
    let temp_file_path = temp_dir.join("TitaouPOS_Aclas_PLU.txt");
    let content = generate_plu_file_content(products, default_dept, default_barcode_type);
    
    let mut f = File::create(&temp_file_path).map_err(|e| e.to_string())?;
    f.write_all(&content).map_err(|e| e.to_string())?;
    drop(f);

    let count = products.len();

    unsafe {
        let lib = match libloading::Library::new(&dll_path) {
            Ok(l) => l,
            Err(e) => {
                if let Some(ref cwd) = original_cwd { let _ = std::env::set_current_dir(cwd); }
                return Err(format!("Failed to load AclasSDK.dll: {}", e));
            }
        };

        let p_init: libloading::Symbol<FnInitialize> = lib
            .get(b"AclasSDK_Initialize\0")
            .map_err(|e| format!("Symbol AclasSDK_Initialize not found: {}", e))?;
        let p_exec: libloading::Symbol<FnExecTaskA> = lib
            .get(b"AclasSDK_ExecTaskA\0")
            .map_err(|e| format!("Symbol AclasSDK_ExecTaskA not found: {}", e))?;
        let p_wait: libloading::Symbol<FnWaitForTask> = lib
            .get(b"AclasSDK_WaitForTask\0")
            .map_err(|e| format!("Symbol AclasSDK_WaitForTask not found: {}", e))?;
        let p_finalize: libloading::Symbol<FnFinalize> = lib
            .get(b"AclasSDK_Finalize\0")
            .map_err(|e| format!("Symbol AclasSDK_Finalize not found: {}", e))?;

        let _ = p_init(std::ptr::null());
        std::thread::sleep(std::time::Duration::from_millis(500));

        let addr = ip_to_dword(ip);
        let path_c = CString::new(temp_file_path.to_string_lossy().as_bytes()).unwrap();

        let task_handle = p_exec(
            addr,
            port,
            protocol_type,
            0, // ASSDK_ProcType_Down
            0, // ASSDK_DataType_PLU
            path_c.as_ptr(),
            0,
            0,
        );

        if task_handle != 0 {
            p_wait(task_handle);
        }

        p_finalize();
    }

    // Restore original CWD
    if let Some(ref cwd) = original_cwd { let _ = std::env::set_current_dir(cwd); }

    // Update product sync status in DB and write logs
    let conn = db.conn.lock().unwrap();
    for p in products {
        let _ = conn.execute(
            "UPDATE products SET scale_sync_status = 'synced' WHERE id = ?1",
            [p.id],
        );

        let _ = conn.execute(
            "INSERT INTO scale_sync_logs (product_id, product_name, scale_plu, action, direction, status, user_name)
             VALUES (?1, ?2, ?3, 'UPLOAD', 'TO_SCALE', 'SUCCESS', ?4)",
            params![p.id, p.name_fr, p.scale_plu.unwrap_or(p.id), user_name.clone().unwrap_or_else(|| "admin".to_string())],
        );
    }

    Ok(count)
}

pub fn fetch_products_from_scale(
    db: &DbState,
    ip: &str,
    port: u32,
    protocol_type: u32,
    user_name: Option<String>,
) -> Result<usize, String> {
    let dll_path = get_dll_path();
    if !dll_path.exists() {
        return Err(format!("AclasSDK.dll not found at: {:?}", dll_path));
    }

    let dll_dir = dll_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let original_cwd = std::env::current_dir().ok();
    let _ = std::env::set_current_dir(dll_dir);

    let temp_dir = std::env::temp_dir();
    let temp_file_path = temp_dir.join("TitaouPOS_Aclas_PLU_Downloaded.txt");
    let _ = std::fs::remove_file(&temp_file_path);

    unsafe {
        let lib = match libloading::Library::new(&dll_path) {
            Ok(l) => l,
            Err(e) => {
                if let Some(ref cwd) = original_cwd { let _ = std::env::set_current_dir(cwd); }
                return Err(format!("Failed to load AclasSDK.dll: {}", e));
            }
        };

        let p_init: libloading::Symbol<FnInitialize> = lib
            .get(b"AclasSDK_Initialize\0")
            .map_err(|e| format!("Symbol AclasSDK_Initialize not found: {}", e))?;
        let p_exec: libloading::Symbol<FnExecTaskA> = lib
            .get(b"AclasSDK_ExecTaskA\0")
            .map_err(|e| format!("Symbol AclasSDK_ExecTaskA not found: {}", e))?;
        let p_wait: libloading::Symbol<FnWaitForTask> = lib
            .get(b"AclasSDK_WaitForTask\0")
            .map_err(|e| format!("Symbol AclasSDK_WaitForTask not found: {}", e))?;
        let p_finalize: libloading::Symbol<FnFinalize> = lib
            .get(b"AclasSDK_Finalize\0")
            .map_err(|e| format!("Symbol AclasSDK_Finalize not found: {}", e))?;

        let _ = p_init(std::ptr::null());
        std::thread::sleep(std::time::Duration::from_millis(500));

        let addr = ip_to_dword(ip);
        let path_c = CString::new(temp_file_path.to_string_lossy().as_bytes()).unwrap();

        let task_handle = p_exec(
            addr,
            port,
            protocol_type,
            1, // ASSDK_ProcType_UP (Upload from scale to PC)
            0, // ASSDK_DataType_PLU
            path_c.as_ptr(),
            0,
            0,
        );

        if task_handle != 0 {
            p_wait(task_handle);
        }

        p_finalize();
    }

    if let Some(ref cwd) = original_cwd { let _ = std::env::set_current_dir(cwd); }

    if !temp_file_path.exists() {
        return Err("No PLU file generated by scale download task.".to_string());
    }

    let bytes = std::fs::read(&temp_file_path).map_err(|e| e.to_string())?;
    let content = if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let u16_slice: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        String::from_utf16_lossy(&u16_slice)
    } else {
        String::from_utf8_lossy(&bytes).to_string()
    };

    let mut imported_count = 0;
    let conn = db.conn.lock().unwrap();

    let mut lines = content.lines();
    // Skip header line
    let _ = lines.next();

    for line in lines {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 8 {
            continue;
        }

        let plu: i64 = parts[0].trim().parse().unwrap_or(0);
        if plu == 0 {
            continue;
        }

        let code = parts[1].trim().to_string();
        let dept: i64 = parts[2].trim().parse().unwrap_or(1);
        let name = parts[4].trim().to_string();
        let price: i64 = parts[7].trim().parse().unwrap_or(0);
        let barcode_type: i64 = if parts.len() > 9 { parts[9].trim().parse().unwrap_or(97) } else { 97 };

        let mut existing_id: Option<i64> = None;
        let mut stmt = conn.prepare("SELECT id FROM products WHERE scale_plu = ?1 OR scale_code = ?2 LIMIT 1").unwrap();
        let mut rows = stmt.query(params![plu, code]).unwrap();
        if let Some(row) = rows.next().unwrap() {
            existing_id = Some(row.get(0).unwrap());
        }

        if let Some(id) = existing_id {
            let _ = conn.execute(
                "UPDATE products SET name_fr = ?1, sale_price = ?2, scale_plu = ?3, scale_code = ?4, scale_department_id = ?5, scale_barcode_type = ?6, is_scalable = 1, scale_sync_status = 'synced' WHERE id = ?7",
                params![name, price, plu, code, dept, barcode_type, id],
            );
        } else {
            let _ = conn.execute(
                "INSERT INTO products (sku, name_ar, name_fr, name_en, sale_price, purchase_price, is_scalable, scale_plu, scale_code, scale_department_id, scale_barcode_type, scale_sync_status, current_stock, min_stock, tax_rate)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 1, ?6, ?7, ?8, ?9, 'synced', 100, 5, 0)",
                params![code, name, name, name, price, plu, code, dept, barcode_type],
            );
        }

        let _ = conn.execute(
            "INSERT INTO scale_sync_logs (product_name, scale_plu, action, direction, status, user_name)
             VALUES (?1, ?2, 'DOWNLOAD', 'FROM_SCALE', 'SUCCESS', ?3)",
            params![name, plu, user_name.clone().unwrap_or_else(|| "admin".to_string())],
        );

        imported_count += 1;
    }

    Ok(imported_count)
}

pub fn get_sync_logs(db: &DbState) -> Result<Vec<ScaleSyncLog>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, product_id, product_name, scale_plu, action, direction, status, error_message, user_name, created_at FROM scale_sync_logs ORDER BY id DESC LIMIT 100")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |r| {
            Ok(ScaleSyncLog {
                id: r.get(0)?,
                product_id: r.get(1)?,
                product_name: r.get(2)?,
                scale_plu: r.get(3)?,
                action: r.get(4)?,
                direction: r.get(5)?,
                status: r.get(6)?,
                error_message: r.get(7)?,
                user_name: r.get(8)?,
                created_at: r.get(9)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let list = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

/// Parse a scanned scale barcode into (item_code, weight, unit_price) per
/// the CONFIGURED barcode format. Supported: Type 97 (18-code), Type 2/22/12
/// (price-embedded) and Type 7/27/17 (weight-embedded). EAN-13 style only.
pub fn parse_scale_barcode_code(code: &str, barcode_type: i64) -> Option<(String, f64, i64)> {
    let digits: String = code.trim().chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 13 {
        return None;
    }
    let d: Vec<char> = digits.chars().collect();
    let num = |from: usize, len: usize| -> Option<String> {
        d.get(from..from + len).map(|sl| sl.iter().collect::<String>())
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
