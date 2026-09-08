use rusqlite::{Connection, Result};
use std::path::PathBuf;
use std::sync::Mutex;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};

pub struct DbState {
    pub conn: Mutex<Connection>,
}

impl DbState {
    pub fn new() -> Result<Self> {
        let db_path = get_database_path();
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(&db_path)?;
        
        // Performance PRAGMAs
        conn.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
        ")?;

        let state = Self {
            conn: Mutex::new(conn),
        };

        state.run_migrations()?;
        state.seed_default_admin()?;
        state.ensure_default_session()?;

        Ok(state)
    }

    pub fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        let m1 = include_str!("../../migrations/001_initial_schema.sql");
        conn.execute_batch(m1)?;

        // Ensure missing columns exist in existing user SQLite databases
        let _ = conn.execute("ALTER TABLE categories ADD COLUMN color TEXT DEFAULT '#0284c7';", []);
        let _ = conn.execute("ALTER TABLE customers ADD COLUMN rc TEXT;", []);
        let _ = conn.execute("ALTER TABLE customers ADD COLUMN nif TEXT;", []);
        let _ = conn.execute("ALTER TABLE customers ADD COLUMN nis TEXT;", []);
        let _ = conn.execute("ALTER TABLE customers ADD COLUMN ai TEXT;", []);
        let _ = conn.execute("ALTER TABLE customers ADD COLUMN qr_code TEXT;", []);
        let _ = conn.execute("ALTER TABLE suppliers ADD COLUMN rc TEXT;", []);
        let _ = conn.execute("ALTER TABLE suppliers ADD COLUMN nif TEXT;", []);
        let _ = conn.execute("ALTER TABLE suppliers ADD COLUMN nis TEXT;", []);
        let _ = conn.execute("ALTER TABLE suppliers ADD COLUMN ai TEXT;", []);
        let _ = conn.execute("ALTER TABLE suppliers ADD COLUMN contact_person TEXT;", []);
        let _ = conn.execute("ALTER TABLE employees ADD COLUMN rfid_code TEXT;", []);
        // Employee ↔ customer link: every employee is auto-created as a
        // customer so their POS purchases (cash or debt) land in a history.
        let _ = conn.execute("ALTER TABLE employees ADD COLUMN customer_id INTEGER;", []);
        let _ = conn.execute("ALTER TABLE users ADD COLUMN pinned INTEGER DEFAULT 0;", []);
        conn.execute(
            "CREATE TABLE IF NOT EXISTS employee_absences (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                employee_id INTEGER NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
                days INTEGER NOT NULL,
                reason TEXT,
                date TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // ---- One-time UTC -> local migration (v0.5.10) ---------------------
        // Rows written before v0.5.9 carry UTC timestamps (SQLite
        // CURRENT_TIMESTAMP) and displayed one hour behind on UTC+ machines.
        // New writes are local; this shifts every legacy row once, guarded
        // by a settings flag so it never runs twice.
        let migrated: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM app_settings WHERE key = 'utc_migrated_v1'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if migrated == 0 {
            // SQLite lacks a direct UTC->local modifier, so compute the
            // machine's current offset in hours and apply it with
            // strftime('%H:%M:%S' arithmetic). chrono gives us the offset.
            let offset_secs = chrono::Local::now().offset().local_minus_utc();
            let sign = if offset_secs >= 0 { "+" } else { "-" };
            let abs_off = offset_secs.abs();
            let modifier = format!(
                "{}{:02}:{:02}",
                sign,
                abs_off / 3600,
                (abs_off % 3600) / 60
            );
            let tables_cols: &[(&str, &str)] = &[
                ("sales", "created_at"),
                ("sale_payments", "created_at"),
                ("cash_sessions", "opened_at"),
                ("cash_sessions", "closed_at"),
                ("cash_movements", "created_at"),
                ("expenses", "created_at"),
                ("purchases", "created_at"),
                ("inventory_movements", "created_at"),
                ("product_price_history", "created_at"),
                ("employees", "hire_date"),
                ("customer_debt_payments", "created_at"),
                ("supplier_debt_payments", "created_at"),
                ("employee_advances", "created_at"),
                ("employee_absences", "created_at"),
            ];
            for (table, col) in tables_cols {
                let _ = conn.execute(
                    &format!(
                        "UPDATE {table} SET {col} = datetime({col}, '{modifier}')
                         WHERE {col} IS NOT NULL AND {col} != ''",
                        table = table,
                        col = col,
                        modifier = modifier
                    ),
                    [],
                );
            }
            let _ = conn.execute(
                "INSERT OR REPLACE INTO app_settings (key, value) VALUES ('utc_migrated_v1', '1')",
                [],
            );
        }
        let _ = conn.execute("ALTER TABLE products ADD COLUMN expiry_date TEXT;", []);
        let _ = conn.execute("ALTER TABLE products ADD COLUMN is_scalable INTEGER DEFAULT 0;", []);
        let _ = conn.execute("ALTER TABLE products ADD COLUMN scale_code TEXT;", []);
        let _ = conn.execute("ALTER TABLE products ADD COLUMN scale_plu INTEGER;", []);
        let _ = conn.execute("ALTER TABLE products ADD COLUMN scale_barcode_type INTEGER DEFAULT 97;", []);
        let _ = conn.execute("ALTER TABLE products ADD COLUMN scale_department_id INTEGER DEFAULT 1;", []);
        let _ = conn.execute("ALTER TABLE products ADD COLUMN scale_sync_status TEXT DEFAULT 'pending';", []);
        // Pinning: pinned products float to the top of the catalog and of
        // their family; pin_order is the manual arrangement among pinned.
        let _ = conn.execute("ALTER TABLE products ADD COLUMN pinned INTEGER DEFAULT 0;", []);
        let _ = conn.execute("ALTER TABLE products ADD COLUMN pin_order INTEGER DEFAULT 0;", []);
        let _ = conn.execute("ALTER TABLE cash_sessions ADD COLUMN is_archived INTEGER DEFAULT 0;", []);

        // LAN multi-terminal: every financial record is stamped with the
        // PC (terminal name) that created it. Existing rows are backfilled
        // with THIS machine's name (they were all created here pre-LAN).
        // Each ALTER is a SEPARATE statement: execute_batch stops at the
        // first failure, which could silently skip every column after it
        // (seen in the field: cash_movements missing → "Failed to open
        // session"). Individual statements are also ID EMPOTENT — a
        // partially migrated DB self-repairs on this pass.
        for table in [
            "sales",
            "purchases",
            "expenses",
            "cash_sessions",
            "cash_movements",
            "customer_debt_payments",
            "supplier_debt_payments",
        ] {
            // "duplicate column name" is the only expected error (column
            // already added on a previous run) — anything else is real.
            if let Err(e) = conn.execute(
                &format!("ALTER TABLE {} ADD COLUMN terminal_name TEXT DEFAULT '';", table),
                [],
            ) {
                let msg = e.to_string();
                if !msg.contains("duplicate column") {
                    eprintln!("[db] terminal_name migration failed for {}: {}", table, msg);
                }
            }
        }
        let this_pc = crate::network::terminal_name_for_this_pc();
        let _ = conn.execute(
            "UPDATE sales SET terminal_name = ?1 WHERE terminal_name = '' OR terminal_name IS NULL",
            [&this_pc],
        );
        let _ = conn.execute(
            "UPDATE purchases SET terminal_name = ?1 WHERE terminal_name = '' OR terminal_name IS NULL",
            [&this_pc],
        );
        let _ = conn.execute(
            "UPDATE expenses SET terminal_name = ?1 WHERE terminal_name = '' OR terminal_name IS NULL",
            [&this_pc],
        );
        let _ = conn.execute(
            "UPDATE cash_sessions SET terminal_name = ?1 WHERE terminal_name = '' OR terminal_name IS NULL",
            [&this_pc],
        );

        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS scale_sync_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                product_id INTEGER,
                product_name TEXT,
                scale_plu INTEGER,
                action TEXT NOT NULL,
                direction TEXT NOT NULL,
                status TEXT NOT NULL,
                error_message TEXT,
                user_name TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );",
            [],
        );

        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS product_price_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                product_id INTEGER NOT NULL,
                old_purchase_price INTEGER NOT NULL,
                new_purchase_price INTEGER NOT NULL,
                old_sale_price INTEGER NOT NULL,
                new_sale_price INTEGER NOT NULL,
                user_id INTEGER,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );",
            [],
        );

        // Seed default expense categories if empty
        let _ = conn.execute_batch("
            INSERT OR IGNORE INTO expense_categories (id, name_ar, name_fr, name_en, description, is_active) VALUES
            (1, 'إيجار المحل', 'Loyer du magasin', 'Store Rent', 'Loyer commercial', 1),
            (2, 'كهرباء وغاز ومياه', 'Électricité & Eau', 'Utilities', 'Factures Sonelgaz et Eau', 1),
            (3, 'نقل وتوصيل', 'Transport & Livraison', 'Transport & Delivery', 'Frais de transport', 1),
            (4, 'صيانة وإصلاح', 'Maintenance & Réparation', 'Maintenance', 'Entretien matériel', 1),
            (5, 'مستلزمات وتغليف', 'Fournitures & Emballage', 'Supplies & Packaging', 'Sacs et emballage', 1),
            (6, 'مصاريف عامة متنوعة', 'Divers / Général', 'General Expenses', 'Dépenses diverses', 1),
            (7, 'سلف للموظفين', 'Avances Salaires', 'Salary Advances', 'Avances sur salaire', 1);
        ");

        // Pinning for suppliers/customers (same behavior as products).
        let _ = conn.execute("ALTER TABLE suppliers ADD COLUMN pinned INTEGER DEFAULT 0;", []);
        let _ = conn.execute("ALTER TABLE suppliers ADD COLUMN pin_order INTEGER DEFAULT 0;", []);
        let _ = conn.execute("ALTER TABLE customers ADD COLUMN pinned INTEGER DEFAULT 0;", []);
        let _ = conn.execute("ALTER TABLE customers ADD COLUMN pin_order INTEGER DEFAULT 0;", []);

        // Units-in-packaging: carton 24 / fardeau 6 / palette 672 /
        // plateau 30 eggs... Each packaging multiplies the base unit qty.
        let _ = conn.execute_batch("
            CREATE TABLE IF NOT EXISTS product_packagings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                units_per_package INTEGER NOT NULL,
                sale_price INTEGER NOT NULL DEFAULT 0,
                is_default INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_packagings_product ON product_packagings(product_id);
        ");

        // Salary advances (avance sur salaire): persisted, deductible from
        // the next payroll, and booked as an expense when paid in cash.
        let _ = conn.execute_batch("
            CREATE TABLE IF NOT EXISTS employee_advances (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                employee_id INTEGER NOT NULL REFERENCES employees(id) ON DELETE CASCADE,
                amount INTEGER NOT NULL,
                reason TEXT,
                date DATE NOT NULL,
                expense_id INTEGER,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
        ");

        // Seed default Walk-in Customer if not present
        let _ = conn.execute(
            "INSERT OR IGNORE INTO customers (id, name, code, phone, qr_code, balance, is_active)
             VALUES (1, 'Client Comptoir / زبون عادي', 'CUST-001', '0550000000', 'CUST-001', 0, 1);",
            [],
        );

        // Heal legacy DBs where row id=1 was a real customer created before
        // the walk-in seed existed: normalize the name back to the default
        // walk-in label so it never shows as a random person's name.
        let _ = conn.execute_batch("
            UPDATE customers SET name = 'Client Comptoir / زبون عادي',
                                 phone = '0550000000', code = 'CUST-001', qr_code = 'CUST-001'
             WHERE id = 1 AND name NOT LIKE '%Client Comptoir%' AND name NOT LIKE '%زبون عادي%';
        ");

        // Seed default Walk-in Supplier if not present
        let _ = conn.execute(
            "INSERT OR IGNORE INTO suppliers (id, name, contact_person, phone, qr_code, balance, is_active)
             VALUES (1, 'Fournisseur Divers / مورد متنوع', 'Comptoir', '0550000000', 'SUP-001', 0, 1);",
            [],
        );

        // sale_payments CHECK must accept 'versement' (layaway deposits).
        // SQLite cannot ALTER a CHECK constraint, so rebuild the table once
        // when it still carries the old constraint.
        let old_constraint = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='sale_payments'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap_or_default();
        if old_constraint.contains("'credit')") && !old_constraint.contains("'versement'") {
            let _ = conn.execute_batch(
                "CREATE TABLE sale_payments_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    sale_id INTEGER NOT NULL REFERENCES sales(id) ON DELETE CASCADE,
                    payment_method TEXT CHECK(payment_method IN ('cash', 'tpe', 'credit', 'versement')) NOT NULL,
                    amount INTEGER NOT NULL,
                    reference_code TEXT,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                );
                INSERT INTO sale_payments_new (id, sale_id, payment_method, amount, reference_code, created_at)
                    SELECT id, sale_id, payment_method, amount, reference_code, created_at FROM sale_payments;
                DROP TABLE sale_payments;
                ALTER TABLE sale_payments_new RENAME TO sale_payments;",
            );
        }

        // Performance Indexes
        let _ = conn.execute_batch("
            CREATE INDEX IF NOT EXISTS idx_products_sku ON products(sku);
            CREATE INDEX IF NOT EXISTS idx_products_scalable ON products(is_scalable);
            CREATE INDEX IF NOT EXISTS idx_products_expiry ON products(expiry_date);
            CREATE INDEX IF NOT EXISTS idx_barcodes_barcode ON barcodes(barcode);
            CREATE INDEX IF NOT EXISTS idx_barcodes_product_id ON barcodes(product_id);
            CREATE INDEX IF NOT EXISTS idx_sales_invoice_number ON sales(invoice_number);
            CREATE INDEX IF NOT EXISTS idx_sales_user_id ON sales(user_id);
            CREATE INDEX IF NOT EXISTS idx_purchases_invoice ON purchases(invoice_number);
            CREATE INDEX IF NOT EXISTS idx_employees_code ON employees(employee_code);
            CREATE INDEX IF NOT EXISTS idx_customers_code ON customers(code);
        ");

        // Debt-clearing archive: when an admin forgives a customer's or
        // supplier's outstanding balance, the cleared amount is archived here
        // (never destroyed) — the balance itself just becomes 0.
        let _ = conn.execute_batch("
            CREATE TABLE IF NOT EXISTS debt_clear_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_type TEXT NOT NULL CHECK(entity_type IN ('customer', 'supplier')),
                entity_id INTEGER NOT NULL,
                entity_name TEXT,
                previous_debt INTEGER NOT NULL,
                new_debt INTEGER NOT NULL DEFAULT 0,
                reason TEXT,
                user_name TEXT,
                created_at TEXT DEFAULT (datetime('now','localtime'))
            );
        ");

        // Payroll reminder dedup: one row per (employee, payment date,
        // reminder kind) — restarts/settings reloads never resend a
        // reminder that already fired for that occurrence.
        let _ = conn.execute_batch("
            CREATE TABLE IF NOT EXISTS payroll_reminder_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                employee_id INTEGER NOT NULL,
                payment_date TEXT NOT NULL,
                reminder_type TEXT NOT NULL CHECK(reminder_type IN ('DAY_BEFORE', 'DUE_TODAY')),
                created_at TEXT DEFAULT (datetime('now','localtime')),
                UNIQUE(employee_id, payment_date, reminder_type)
            );
        ");

        // ---- Cloud sync (TitaouCRM integration) ---------------------------
        // Transactional outbox: business services enqueue events INSIDE their
        // write transaction (a rolled-back sale never syncs). The cloudsync
        // pusher drains it on the coordinator only; idempotent CRM RPCs keyed
        // on pos_ref make replays exactly-once.
        let _ = conn.execute_batch("
            CREATE TABLE IF NOT EXISTS sync_outbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity TEXT NOT NULL CHECK(entity IN ('sale','refund','purchase','product','stock_adjustment','customer','customer_payment','supplier')),
                local_id INTEGER,
                payload TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','done','failed')),
                attempts INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                created_at TEXT DEFAULT (datetime('now','localtime')),
                pushed_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_sync_outbox_pending ON sync_outbox(status, id);
        ");

        // Local id ↔ CRM uuid mapping (exactly-once: never push an entity the
        // CRM already owns; never pull the same CRM row twice as new).
        let _ = conn.execute_batch("
            CREATE TABLE IF NOT EXISTS sync_map (
                entity TEXT NOT NULL,
                local_id INTEGER NOT NULL,
                remote_id TEXT NOT NULL,
                PRIMARY KEY (entity, local_id)
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_sync_map_remote ON sync_map(entity, remote_id);
        ");

        // Mirror of field-app orders (pulled read-only for the Field Orders
        // view + stock application). source='field' rows created by the
        // preseller/seller apps; source='pos' rows are ours (skipped).
        let _ = conn.execute_batch("
            CREATE TABLE IF NOT EXISTS crm_orders (
                crm_id TEXT PRIMARY KEY,
                client_name TEXT,
                member_name TEXT,
                source TEXT NOT NULL DEFAULT 'field',
                status TEXT,
                payment_status TEXT,
                total_amount INTEGER NOT NULL DEFAULT 0,
                amount_paid INTEGER NOT NULL DEFAULT 0,
                notes TEXT,
                created_at TEXT,
                updated_at TEXT,
                stock_applied INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS crm_order_items (
                crm_order_id TEXT NOT NULL,
                product_id INTEGER,          -- local product id after linking
                product_name TEXT,
                quantity REAL NOT NULL DEFAULT 0,
                unit_price INTEGER NOT NULL DEFAULT 0,
                line_total INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (crm_order_id, product_id)
            );
        ");

        // Pull cursors: high-water mark per pulled stream, so restarts resume
        // instead of re-pulling everything (rows carry updated_at from the
        // server; the cursor stores the max seen).
        let _ = conn.execute_batch("
            CREATE TABLE IF NOT EXISTS sync_cursors (
                stream TEXT PRIMARY KEY,
                cursor_value TEXT NOT NULL
            );
        ");

        // Backup settings persistence keys live in app_settings; nothing
        // schema-level needed for them.

        let m2 = include_str!("../../migrations/002_seed_data.sql");
        let _ = conn.execute_batch(m2);

        Ok(())
    }

    pub fn seed_default_admin(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        let _ = conn.execute(
            "INSERT OR IGNORE INTO roles (id, name, description, is_system) VALUES (1, 'Administrator', 'Full system access', 1);",
            [],
        );
        // Cashier: POS + sales history + expenses only (enforced by the app
        // navigation). Manager: store operations and reports.
        let _ = conn.execute(
            "INSERT OR IGNORE INTO roles (id, name, description, is_system) VALUES (2, 'Cashier', 'POS, sales history and expenses', 1);",
            [],
        );
        let _ = conn.execute(
            "INSERT OR IGNORE INTO roles (id, name, description, is_system) VALUES (3, 'Manager', 'Store operations and reports', 1);",
            [],
        );

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM users WHERE username = 'admin'",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        if count == 0 {
            let password = "admin";
            let salt = SaltString::generate(&mut OsRng);
            let argon2 = Argon2::default();
            let password_hash = argon2
                .hash_password(password.as_bytes(), &salt)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))))?
                .to_string();

            conn.execute(
                "INSERT INTO users (username, display_name, password_hash, role_id, max_discount_percent, is_active)
                 VALUES ('admin', 'Administrator', ?1, 1, 100.0, 1)",
                [&password_hash],
            )?;
        }

        Ok(())
    }

    fn ensure_default_session(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let _ = conn.execute(
            "INSERT OR IGNORE INTO registers (id, name, identifier, is_active) VALUES (1, 'Main Register 01', 'REG-01', 1);",
            [],
        );

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM cash_sessions WHERE status = 'open'",
            [],
            |r| r.get(0),
        ).unwrap_or(0);

        if count == 0 {
            let _ = conn.execute(
                "INSERT INTO cash_sessions (register_id, user_id, opening_amount, expected_cash, status, notes, opened_at)
                 VALUES (1, 1, 0, 0, 'open', 'Default Auto-Opened Session', datetime('now','localtime'))",
                [],
            );
            let session_id = conn.last_insert_rowid();
            let _ = conn.execute(
                "INSERT INTO cash_movements (session_id, user_id, type, amount, reason)
                 VALUES (?1, 1, 'opening_balance', 0, 'Startup Cash / رصيد افتتاحي')",
                [session_id],
            );
        }

        Ok(())
    }
}

pub fn get_database_path() -> PathBuf {
    let mut path = dirs_next().unwrap_or_else(|| PathBuf::from("."));
    path.push("TitaouPosCRM");
    path.push("titaou_poscrm.db");
    path
}

fn dirs_next() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
    }
}

#[cfg(test)]
mod terminal_stamp_tests {
    use super::*;

    fn fresh_db() -> DbState {
        let dir = std::env::temp_dir().join("titaou_migration_tests");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("mig_{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let state = DbState { conn: std::sync::Mutex::new(rusqlite::Connection::open(&path).unwrap()) };
        state.run_migrations().unwrap();
        state
    }

    fn has_column(conn: &rusqlite::Connection, table: &str, col: &str) -> bool {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table)).unwrap();
        let names: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        names.iter().any(|name| name == col)
    }

    #[test]
    fn all_stamp_tables_gain_terminal_name() {
        let db = fresh_db();
        let conn = db.conn.lock().unwrap();
        for t in ["sales", "purchases", "expenses", "cash_sessions", "cash_movements",
                  "customer_debt_payments", "supplier_debt_payments"] {
            assert!(has_column(&conn, t, "terminal_name"),
                    "{} missing terminal_name after migration", t);
        }
    }

    #[test]
    fn partial_migration_self_repairs() {
        // Field-found defect: a DB whose batch ALTER stopped early (e.g.
        // cash_movements never got the column) must be REPAIRED by the next
        // run_migrations pass — idempotent per-table ALTERs.
        let dir = std::env::temp_dir().join("titaou_migration_tests");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("mig_partial_{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let raw = rusqlite::Connection::open(&path).unwrap();
        raw.execute_batch("
            PRAGMA journal_mode = WAL;
            CREATE TABLE cash_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                register_id INTEGER, user_id INTEGER, opening_amount INTEGER DEFAULT 0,
                expected_cash INTEGER DEFAULT 0, status TEXT DEFAULT 'open',
                notes TEXT, opened_at TEXT, closed_at TEXT, actual_cash INTEGER,
                difference INTEGER, terminal_name TEXT DEFAULT ''
            );
            CREATE TABLE cash_movements (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER, user_id INTEGER, type TEXT, amount INTEGER,
                reason TEXT, notes TEXT, created_at TEXT
            );
        ").unwrap();
        drop(raw);

        let state = DbState { conn: std::sync::Mutex::new(rusqlite::Connection::open(&path).unwrap()) };
        state.run_migrations().unwrap();
        let conn = state.conn.lock().unwrap();
        assert!(has_column(&conn, "cash_movements", "terminal_name"),
                "cash_movements.terminal_name not repaired by migration");
        assert!(has_column(&conn, "cash_sessions", "terminal_name"),
                "cash_sessions.terminal_name not repaired by migration");
    }

    #[test]
    fn migrations_are_idempotent() {
        let db = fresh_db();
        // A second pass must not error (all "duplicate column" cases).
        db.run_migrations().unwrap();
        let conn = db.conn.lock().unwrap();
        assert!(has_column(&conn, "cash_movements", "terminal_name"));
    }
}
