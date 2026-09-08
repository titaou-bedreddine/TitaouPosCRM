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
        let _ = conn.execute_batch("
            ALTER TABLE sales ADD COLUMN terminal_name TEXT DEFAULT '';
            ALTER TABLE purchases ADD COLUMN terminal_name TEXT DEFAULT '';
            ALTER TABLE expenses ADD COLUMN terminal_name TEXT DEFAULT '';
            ALTER TABLE cash_sessions ADD COLUMN terminal_name TEXT DEFAULT '';
            ALTER TABLE cash_movements ADD COLUMN terminal_name TEXT DEFAULT '';
            ALTER TABLE customer_debt_payments ADD COLUMN terminal_name TEXT DEFAULT '';
            ALTER TABLE supplier_debt_payments ADD COLUMN terminal_name TEXT DEFAULT '';
        ");
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
    path.push("TitaouPosT");
    path.push("titaou_post.db");
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