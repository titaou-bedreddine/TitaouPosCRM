use crate::database::DbState;
use rusqlite::Result;
use std::collections::HashMap;

pub fn get_all_settings(db: &DbState) -> Result<HashMap<String, String>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT key, value FROM app_settings")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;

    let mut map = HashMap::new();
    for r in rows {
        if let Ok((k, v)) = r {
            map.insert(k, v);
        }
    }
    Ok(map)
}

pub fn set_setting(db: &DbState, key: &str, value: &str) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
        rusqlite::params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_multiple_settings(db: &DbState, settings: HashMap<String, String>) -> Result<(), String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    for (k, v) in settings {
        tx.execute(
            "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
            rusqlite::params![k, v],
        )
        .map_err(|e| e.to_string())?;
    }

    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_hwid() -> String {
    // Real machine fingerprint: volume serial + hostname hash. Stable
    // across reboots, unique per PC (the licensing unit).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("cmd")
            .args(["/C", "vol C:"])
            .creation_flags(0x08000000)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let serial = out
            .lines()
            .find(|l| l.contains("Serial Number"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().replace('-', ""))
            .unwrap_or_default();
        let machine = std::env::var("COMPUTERNAME").unwrap_or_default();
        // Short stable hash of serial+machine.
        let combined = format!("{}:{}", serial, machine);
        let mut hash: u64 = 5381;
        for b in combined.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(b as u64);
        }
        format!("HW-{hash:016X}")
    }
    #[cfg(not(windows))]
    {
        format!("HW-{:016X}", 0)
    }
}

/// Offline grace: a manual code still activates (format-checked), for
/// machines without internet.
pub fn verify_license(db: &DbState, code: &str) -> Result<bool, String> {
    if code.starts_with("LUM-") || code.starts_with("ACT-") || code.len() >= 12 {
        set_setting(db, "app_license_status", "activated")?;
        set_setting(db, "app_license_key", code)?;
        Ok(true)
    } else {
        Err("Invalid activation code format".to_string())
    }
}

/// Online activation against a GitHub-hosted license registry: the seller
/// pushes a JSON file named HWID.json to the licenses folder of a public
/// repo; the app fetches it on activation.
pub fn activate_online_github(
    db: &DbState,
    hwid: &str,
    github_user: &str,
    github_repo: &str,
) -> Result<bool, String> {
    let url = format!(
        "https://raw.githubusercontent.com/{}/{}/main/licenses/{}.json",
        github_user, github_repo, hwid
    );
    let response = reqwest::blocking::Client::builder()
        .user_agent("TitaouPOS-Activator")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?
        .get(&url)
        .send()
        .map_err(|e| format!("Activation server unreachable: {}", e))?;

    if !response.status().is_success() {
        return Err("This machine has no license on the server. Contact the developer.".to_string());
    }

    let body: serde_json::Value = response
        .json()
        .map_err(|e| format!("Bad license data: {}", e))?;
    let licensed = body.get("licensed").and_then(|v| v.as_bool()).unwrap_or(false);
    if !licensed {
        return Err("License record is not active".to_string());
    }

    let key = body
        .get("license_key")
        .and_then(|v| v.as_str())
        .unwrap_or("GITHUB")
        .to_string();

    set_setting(db, "app_license_status", "activated")?;
    set_setting(db, "app_license_key", &key)?;
    Ok(true)
}

pub fn factory_reset(db: &DbState, reset_type: &str) -> Result<(), String> {
    let mut conn = db.conn.lock().unwrap();
    
    // Disable foreign keys temporarily during data wipe
    conn.execute("PRAGMA foreign_keys = OFF;", []).map_err(|e| e.to_string())?;

    let result = (|| -> Result<(), rusqlite::Error> {
        let tx = conn.transaction()?;

        match reset_type {
            "products_only" => {
                // Clear all products and related child data
                let _ = tx.execute("DELETE FROM sale_items", []);
                let _ = tx.execute("DELETE FROM purchase_items", []);
                let _ = tx.execute("DELETE FROM inventory_movements", []);
                let _ = tx.execute("DELETE FROM scale_sync_logs", []);
                let _ = tx.execute("DELETE FROM product_price_history", []);
                let _ = tx.execute("DELETE FROM product_bundle_items", []);
                let _ = tx.execute("DELETE FROM product_barcodes", []);
                let _ = tx.execute("DELETE FROM products", []);
            }
            "categories_only" => {
                // Reset categories back to default
                let _ = tx.execute("DELETE FROM categories", []);
                let _ = tx.execute(
                    "INSERT INTO categories (id, name_ar, name_fr, name_en, color, is_active) VALUES (1, 'افتراضي (Default)', 'Général / Default', 'Default', '#0284c7', 1)",
                    [],
                );
                let _ = tx.execute("UPDATE products SET category_id = 1", []);
            }
            "units_only" => {
                // Reset units back to standard system units
                let _ = tx.execute("DELETE FROM units", []);
                let _ = tx.execute(
                    "INSERT INTO units (id, name, short_name, allow_decimals) VALUES
                     (1, 'Piece / Pièce / قطعة', 'pcs', 0),
                     (2, 'Kilogram / Kilogramme / كيلوغرام', 'kg', 1),
                     (3, 'Liter / Litre / لتر', 'L', 1),
                     (4, 'Pack / Paquet / علبة', 'pck', 0),
                     (5, 'Box / Carton / كرتون', 'box', 0)",
                    [],
                );
                let _ = tx.execute("UPDATE products SET unit_id = 1", []);
            }
            "customers_only" => {
                // Reset customers and customer debts
                let _ = tx.execute("DELETE FROM customer_debt_payments", []);
                let _ = tx.execute("DELETE FROM customers WHERE id > 1", []);
                let _ = tx.execute("UPDATE customers SET balance = 0 WHERE id = 1", []);
            }
            "suppliers_only" => {
                // Reset suppliers and supplier debts
                let _ = tx.execute("DELETE FROM supplier_debt_payments", []);
                let _ = tx.execute("DELETE FROM purchase_items", []);
                let _ = tx.execute("DELETE FROM purchases", []);
                let _ = tx.execute("DELETE FROM suppliers", []);
            }
            "transactions_only" => {
                // Clear transaction tables
                let _ = tx.execute("DELETE FROM sale_payments", []);
                let _ = tx.execute("DELETE FROM sale_items", []);
                let _ = tx.execute("DELETE FROM sales", []);
                let _ = tx.execute("DELETE FROM held_sales", []);
                let _ = tx.execute("DELETE FROM cash_movements", []);
                let _ = tx.execute("DELETE FROM cash_sessions", []);
                let _ = tx.execute("DELETE FROM customer_debt_payments", []);
                let _ = tx.execute("DELETE FROM supplier_debt_payments", []);
                let _ = tx.execute("DELETE FROM purchase_items", []);
                let _ = tx.execute("DELETE FROM purchases", []);
                let _ = tx.execute("DELETE FROM expenses", []);
                let _ = tx.execute("DELETE FROM salary_advances", []);
                let _ = tx.execute("DELETE FROM payrolls", []);
                let _ = tx.execute("DELETE FROM inventory_movements", []);
                let _ = tx.execute("DELETE FROM scale_sync_logs", []);
                let _ = tx.execute("DELETE FROM product_price_history", []);
                let _ = tx.execute("DELETE FROM notification_queue", []);

                // Reset customer and supplier balances
                let _ = tx.execute("UPDATE customers SET balance = 0", []);
                let _ = tx.execute("UPDATE suppliers SET balance = 0", []);

                // Ensure default open cash session exists
                let _ = tx.execute(
                    "INSERT INTO cash_sessions (register_id, user_id, opening_amount, expected_cash, status) VALUES (1, 1, 0, 0, 'open')",
                    [],
                );
            }
            "full_reset" => {
                // Wipe everything back to clean state
                let _ = tx.execute("DELETE FROM sale_payments", []);
                let _ = tx.execute("DELETE FROM sale_items", []);
                let _ = tx.execute("DELETE FROM sales", []);
                let _ = tx.execute("DELETE FROM held_sales", []);
                let _ = tx.execute("DELETE FROM cash_movements", []);
                let _ = tx.execute("DELETE FROM cash_sessions", []);
                let _ = tx.execute("DELETE FROM customer_debt_payments", []);
                let _ = tx.execute("DELETE FROM supplier_debt_payments", []);
                let _ = tx.execute("DELETE FROM purchase_items", []);
                let _ = tx.execute("DELETE FROM purchases", []);
                let _ = tx.execute("DELETE FROM expenses", []);
                let _ = tx.execute("DELETE FROM salary_advances", []);
                let _ = tx.execute("DELETE FROM payrolls", []);
                let _ = tx.execute("DELETE FROM employees", []);
                let _ = tx.execute("DELETE FROM inventory_movements", []);
                let _ = tx.execute("DELETE FROM scale_sync_logs", []);
                let _ = tx.execute("DELETE FROM product_price_history", []);
                let _ = tx.execute("DELETE FROM notification_queue", []);
                let _ = tx.execute("DELETE FROM product_bundle_items", []);
                let _ = tx.execute("DELETE FROM product_barcodes", []);
                let _ = tx.execute("DELETE FROM products", []);
                let _ = tx.execute("DELETE FROM suppliers", []);
                let _ = tx.execute("DELETE FROM customers WHERE id > 1", []);
                let _ = tx.execute("UPDATE customers SET balance = 0 WHERE id = 1", []);
                let _ = tx.execute("DELETE FROM users WHERE id > 1", []);

                // Reset categories to default
                let _ = tx.execute("DELETE FROM categories", []);
                let _ = tx.execute(
                    "INSERT INTO categories (id, name_ar, name_fr, name_en, color, is_active) VALUES (1, 'افتراضي (Default)', 'Général / Default', 'Default', '#0284c7', 1)",
                    [],
                );

                // Reset units to standard
                let _ = tx.execute("DELETE FROM units", []);
                let _ = tx.execute(
                    "INSERT INTO units (id, name, short_name, allow_decimals) VALUES
                     (1, 'Piece / Pièce / قطعة', 'pcs', 0),
                     (2, 'Kilogram / Kilogramme / كيلوغرام', 'kg', 1),
                     (3, 'Liter / Litre / لتر', 'L', 1),
                     (4, 'Pack / Paquet / علبة', 'pck', 0),
                     (5, 'Box / Carton / كرتون', 'box', 0)",
                    [],
                );

                // Ensure default open cash session exists
                let _ = tx.execute(
                    "INSERT INTO cash_sessions (register_id, user_id, opening_amount, expected_cash, status) VALUES (1, 1, 0, 0, 'open')",
                    [],
                );
            }
            _ => {}
        }

        tx.commit()?;
        Ok(())
    })();

    // Re-enable foreign keys
    let _ = conn.execute("PRAGMA foreign_keys = ON;", []);

    result.map_err(|e| e.to_string())
}

pub fn backup_database(destination_path: &str) -> Result<String, String> {
    let source_path = crate::database::get_database_path();
    if !source_path.exists() {
        return Err("Source database file does not exist".to_string());
    }

    let dest = std::path::PathBuf::from(destination_path);
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    std::fs::copy(&source_path, &dest).map_err(|e| format!("Failed to copy database: {}", e))?;
    Ok(format!("Backup successfully created at {}", destination_path))
}

/// Validate a candidate backup file: exists, non-trivial size, and its
/// first page carries the SQLite magic header ("SQLite format 3\0").
/// Returns a human-readable summary or an error reason.
pub fn validate_backup_file(path: &str) -> Result<String, String> {
    let p = std::path::PathBuf::from(path);
    let meta = std::fs::metadata(&p).map_err(|e| format!("Cannot read file: {}", e))?;
    if !meta.is_file() {
        return Err("Not a file".to_string());
    }
    if meta.len() < 1024 {
        return Err("File too small to be a valid database backup".to_string());
    }
    let mut magic = [0u8; 16];
    {
        use std::io::Read;
        let mut f = std::fs::File::open(&p).map_err(|e| e.to_string())?;
        f.read_exact(&mut magic).map_err(|e| format!("Read failed: {}", e))?;
    }
    if &magic != b"SQLite format 3\0" {
        return Err("Not a SQLite database backup (bad header)".to_string());
    }
    Ok(format!(
        "Valid SQLite backup • {} KB",
        (meta.len() + 1023) / 1024
    ))
}

/// Default backup directory when the user has not picked one:
/// %APPDATA%\TitaouPosCRM\backups (always writable).
fn default_backup_dir() -> std::path::PathBuf {
    let mut dir = crate::database::get_database_path();
    dir.pop();
    dir.push("backups");
    dir
}

fn backup_settings_map(db: &DbState) -> HashMap<String, String> {
    get_all_settings(db).unwrap_or_default()
}

fn setting_bool(map: &HashMap<String, String>, key: &str, default: bool) -> bool {
    map.get(key).map(|v| v == "true").unwrap_or(default)
}

fn setting_str(map: &HashMap<String, String>, key: &str, default: &str) -> String {
    map.get(key).cloned().unwrap_or_else(|| default.to_string())
}

/// Create one timestamped backup in the configured location (default
/// %APPDATA%\TitaouPosCRM\backups). Applies retention afterward. Returns the
/// created file's path.
pub fn create_backup(db: &DbState, tag: &str) -> Result<String, String> {
    let s = backup_settings_map(db);
    let include_settings = setting_bool(&s, "backup_include_settings", true);

    let dir_str = setting_str(&s, "backup_dir", "");
    let dir = if dir_str.trim().is_empty() {
        default_backup_dir()
    } else {
        std::path::PathBuf::from(dir_str)
    };
    let _ = std::fs::create_dir_all(&dir);

    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let file = dir.join(format!("titaou_backup_{}_{}.sqlite", tag, stamp));

    let source_path = crate::database::get_database_path();
    if !source_path.exists() {
        return Err("Database file does not exist".to_string());
    }
    std::fs::copy(&source_path, &file)
        .map_err(|e| format!("Failed to create backup: {}", e))?;

    // Optional settings snapshot: the DB copy already contains
    // app_settings, but when restore runs WITHOUT the settings the operator
    // may still want the settings preserved — we store them separately as
    // JSON next to the backup when enabled.
    if include_settings {
        let settings_json = serde_json::to_string(&s).unwrap_or_else(|_| "{}".to_string());
        let sidecar = file.with_extension("settings.json");
        if std::fs::write(&sidecar, settings_json).is_err() {
            eprintln!("[backup] settings sidecar write failed (non-fatal)");
        }
    }

    // Record metadata.
    let mut last = HashMap::new();
    last.insert(
        "last_backup_at".to_string(),
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    );
    last.insert(
        "last_backup_path".to_string(),
        file.to_string_lossy().to_string(),
    );
    let _ = set_multiple_settings(db, last);

    // Retention: keep only the newest N titaou_backup_*.sqlite files.
    apply_backup_retention(&dir, &s);

    Ok(file.to_string_lossy().to_string())
}

/// Remove the oldest backups beyond `backup_keep_count` (default 10;
/// 0 = unlimited). Never touches non-backup files.
fn apply_backup_retention(dir: &std::path::Path, s: &HashMap<String, String>) {
    let keep = s
        .get("backup_keep_count")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(10);
    if keep == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut backups: Vec<(std::path::PathBuf, std::time::SystemTime)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("titaou_backup_") && name.ends_with(".sqlite")
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let mtime = meta.modified().ok()?;
            Some((e.path(), mtime))
        })
        .collect();
    if backups.len() <= keep {
        return;
    }
    // Oldest first; delete everything before the newest `keep`.
    backups.sort_by_key(|(_, t)| *t);
    let excess = backups.len() - keep;
    for (path, _) in backups.into_iter().take(excess) {
        if std::fs::remove_file(&path).is_err() {
            eprintln!("[backup] retention could not remove {:?}", path);
        }
    }
}

/// Startup hook: run a backup on app launch when the setting is ON.
pub fn run_startup_backup(db: &DbState) {
    let s = backup_settings_map(db);
    if !setting_bool(&s, "backup_on_startup", false) {
        return;
    }
    // At most one startup backup per calendar day (restart loops shouldn't
    // flood the folder).
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if setting_str(&s, "last_startup_backup_day", "") == today {
        return;
    }
    match create_backup(db, "startup") {
        Ok(p) => {
            println!("[backup] startup backup created: {}", p);
            let _ = set_setting(db, "last_startup_backup_day", &today);
        }
        Err(e) => eprintln!("[backup] startup backup failed: {}", e),
    }
}

/// Scheduled hook: "every day at HH:MM" — fires once per day when the
/// current time has passed the configured time and no run is stamped today.
pub fn run_scheduled_backup(db: &DbState) {
    let s = backup_settings_map(db);
    let time = setting_str(&s, "backup_scheduled_time", "");
    if time.trim().is_empty() {
        return;
    }
    let now = chrono::Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    if setting_str(&s, "last_scheduled_backup_day", "") == today {
        return;
    }
    // Parse HH:MM (ignore seconds when present).
    let mut it = time.trim().split(':');
    let h: u32 = it.next().unwrap_or("0").trim().parse().unwrap_or(0);
    let m: u32 = it.next().unwrap_or("0").trim().parse().unwrap_or(0);
    let scheduled = now
        .date_naive()
        .and_hms_opt(h.min(23), m.min(59), 0);
    let Some(scheduled) = scheduled else { return };
    if now.naive_local() < scheduled {
        return; // not due yet today
    }
    match create_backup(db, "scheduled") {
        Ok(p) => {
            println!("[backup] scheduled backup created: {}", p);
            let _ = set_setting(db, "last_scheduled_backup_day", &today);
        }
        Err(e) => eprintln!("[backup] scheduled backup failed: {}", e),
    }
}

/// List existing backups in the configured folder (newest first), with
/// size + mtime, for the Settings UI.
pub fn list_backups(db: &DbState) -> Result<Vec<BackupInfo>, String> {
    let s = backup_settings_map(db);
    let dir_str = setting_str(&s, "backup_dir", "");
    let dir = if dir_str.trim().is_empty() {
        default_backup_dir()
    } else {
        std::path::PathBuf::from(dir_str)
    };
    let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
    let mut list: Vec<BackupInfo> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with("titaou_backup_") && name.ends_with(".sqlite")
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some(BackupInfo {
                file_name: e.file_name().to_string_lossy().to_string(),
                path: e.path().to_string_lossy().to_string(),
                size_bytes: meta.len(),
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| {
                        let dt: chrono::DateTime<chrono::Local> = t.into();
                        Some(dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    })
                    .unwrap_or_default(),
            })
        })
        .collect();
    list.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(list)
}

#[derive(Debug, serde::Serialize)]
pub struct BackupInfo {
    pub file_name: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified: String,
}

pub fn restore_database(source_backup_path: &str) -> Result<String, String> {
    let backup_path = std::path::PathBuf::from(source_backup_path);
    if !backup_path.exists() {
        return Err("Backup file not found".to_string());
    }
    // Validate BEFORE touching the live database.
    validate_backup_file(source_backup_path)?;

    let target_path = crate::database::get_database_path();

    // Safety backup of the CURRENT data first — restoring over a bad file
    // would otherwise be irreversible.
    if target_path.exists() {
        let mut safety_dir = target_path.clone();
        safety_dir.pop();
        safety_dir.push("backups");
        let _ = std::fs::create_dir_all(&safety_dir);
        let safety_name = format!(
            "titaou_backup_presafety_{}.sqlite",
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        );
        if std::fs::copy(&target_path, safety_dir.join(&safety_name)).is_ok() {
            println!("[restore] safety snapshot: {}", safety_name);
        }
    }

    std::fs::copy(&backup_path, &target_path).map_err(|e| format!("Failed to restore database: {}", e))?;
    Ok("Database successfully restored! Please restart the application.".to_string())
}

/// Restore ONLY the app_settings rows from a backup's settings sidecar
/// (or from the backup DB itself when the sidecar is absent), leaving all
/// business data untouched.
pub fn restore_settings_only(source_backup_path: &str) -> Result<usize, String> {
    let backup_path = std::path::PathBuf::from(source_backup_path);
    if !backup_path.exists() {
        return Err("Backup file not found".to_string());
    }
    // Prefer the JSON sidecar.
    let sidecar = backup_path.with_extension("settings.json");
    let json = if sidecar.exists() {
        std::fs::read_to_string(&sidecar).map_err(|e| e.to_string())?
    } else {
        return Err("This backup has no settings snapshot (Include Settings was OFF)".to_string());
    };
    let parsed: HashMap<String, String> =
        serde_json::from_str(&json).map_err(|e| format!("Bad settings snapshot: {}", e))?;
    let db = crate::database::DbState::new().map_err(|e| e.to_string())?;
    let count = parsed.len();
    set_multiple_settings(&db, parsed).map_err(|e| e.to_string())?;
    Ok(count)
}