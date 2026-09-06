use crate::database::DbState;
use crate::models::{Employee, User};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use rusqlite::Result;

pub fn list_employees(db: &DbState) -> Result<Vec<Employee>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, employee_code, full_name, phone, email, national_id, job_title, base_salary, salary_type, salary_start_date, hire_date, qr_code, rfid_code, is_active, notes, customer_id
             FROM employees
             WHERE is_active = 1
             ORDER BY id DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Employee {
                id: row.get(0)?,
                employee_code: row.get(1)?,
                full_name: row.get(2)?,
                phone: row.get(3)?,
                email: row.get(4)?,
                national_id: row.get(5)?,
                job_title: row.get(6)?,
                base_salary: row.get(7)?,
                salary_type: row.get(8)?,
                salary_start_date: row.get(9)?,
                hire_date: row.get(10)?,
                qr_code: row.get(11)?,
                rfid_code: row.get(12)?,
                is_active: row.get(13)?,
                notes: row.get(14)?,
                customer_id: row.get(15).ok().flatten(),
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<Employee> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

pub fn save_employee(
    db: &DbState,
    code: &str,
    name: &str,
    phone: Option<String>,
    email: Option<String>,
    national_id: Option<String>,
    job_title: &str,
    base_salary: i64,
    salary_type: &str,
    salary_start_date: Option<String>,
    hire_date: &str,
    notes: Option<String>,
    rfid_code: Option<String>,
    employee_id: Option<i64>,
) -> Result<i64, String> {
    let conn = db.conn.lock().unwrap();

    if let Some(eid) = employee_id {
        conn.execute(
            "UPDATE employees
             SET employee_code = ?1, full_name = ?2, phone = ?3, email = ?4, national_id = ?5,
                 job_title = ?6, base_salary = ?7, salary_type = ?8, salary_start_date = ?9, hire_date = ?10, notes = ?11, rfid_code = ?12
             WHERE id = ?13",
            rusqlite::params![
                code, name, phone, email, national_id, job_title,
                base_salary, salary_type, salary_start_date, hire_date, notes, rfid_code, eid
            ],
        )
        .map_err(|e| e.to_string())?;
        // Keep the auto-created linked customer's name in sync.
        let _: i64 = conn
            .execute(
                "UPDATE customers SET name = ?1, phone = COALESCE(?2, phone) WHERE id = (SELECT customer_id FROM employees WHERE id = ?3 AND customer_id IS NOT NULL)",
                rusqlite::params![name, phone, eid],
            )
            .unwrap_or(0) as i64;
        Ok(eid)
    } else {
        let qr_code = format!("EMP-QR-{}", code);
        conn.execute(
            "INSERT INTO employees (employee_code, full_name, phone, email, national_id, job_title, base_salary, salary_type, salary_start_date, hire_date, qr_code, notes, rfid_code)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![
                code, name, phone, email, national_id, job_title,
                base_salary, salary_type, salary_start_date, hire_date, qr_code, notes, rfid_code
            ],
        )
        .map_err(|e| e.to_string())?;
        let emp_id = conn.last_insert_rowid();
        // Every employee is automatically a customer: their POS purchases
        // (cash or debt) are tracked on their own history.
        let cust_qr = format!("EMP-{}", code);
        conn.execute(
            "INSERT INTO customers (name, phone, qr_code, balance, initial_debt, is_active)
             VALUES (?1, ?2, ?3, 0, 0, 1)",
            rusqlite::params![name, phone, cust_qr],
        )
        .map_err(|e| e.to_string())?;
        let customer_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE employees SET customer_id = ?1 WHERE id = ?2",
            rusqlite::params![customer_id, emp_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(emp_id)
    }
}

pub fn delete_employee(db: &DbState, employee_id: i64) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute("UPDATE employees SET is_active = 0 WHERE id = ?1", [employee_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn change_user_password(
    db: &DbState,
    user_id: i64,
    new_password: &str,
    old_password: Option<String>,
) -> Result<(), String> {
    // Changing a password requires the CURRENT password. A forgotten
    // password is recovered with the master key "TITAOU".
    let master = old_password.as_deref().map(|p| p.trim()).unwrap_or("");
    if master != "TITAOU" {
        let conn0 = db.conn.lock().unwrap();
        let stored: Option<String> = conn0
            .query_row(
                "SELECT password_hash FROM users WHERE id = ?1",
                [user_id],
                |r| r.get(0),
            )
            .ok();
        drop(conn0);
        let current = old_password.clone().unwrap_or_default();
        let ok = match stored {
            Some(hash) => {
                use argon2::PasswordVerifier;
                match argon2::password_hash::PasswordHash::new(&hash) {
                    Ok(parsed) => {
                        Argon2::default()
                            .verify_password(current.as_bytes(), &parsed)
                            .is_ok()
                    }
                    Err(_) => false,
                }
            }
            None => false,
        };
        if !ok {
            return Err(
                "Current password is incorrect / كلمة المرور الحالية غير صحيحة".to_string(),
            );
        }
    }

    let conn = db.conn.lock().unwrap();
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(new_password.as_bytes(), &salt)
        .map_err(|e| e.to_string())?
        .to_string();

    conn.execute(
        "UPDATE users SET password_hash = ?1 WHERE id = ?2",
        rusqlite::params![password_hash, user_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn get_user_by_qr(db: &DbState, qr_code: &str) -> Result<Option<User>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT u.id, u.username, u.display_name, u.role_id, r.name, u.max_discount_percent, u.is_active
             FROM employees e
             JOIN users u ON e.user_account_id = u.id OR u.username = LOWER(REPLACE(e.employee_code, '-', ''))
             LEFT JOIN roles r ON u.role_id = r.id
             WHERE e.qr_code = ?1 AND e.is_active = 1 AND u.is_active = 1",
        )
        .map_err(|e| e.to_string())?;

    let row = stmt.query_row([qr_code], |r| {
        Ok(User {
            id: r.get(0)?,
            username: r.get(1)?,
            display_name: r.get(2)?,
            role_id: r.get(3)?,
            role_name: r.get(4)?,
            max_discount_percent: r.get(5)?,
            is_active: r.get(6)?,
            permissions: vec!["sales.create".to_string(), "sales.view".to_string()],
        })
    });

    match row {
        Ok(u) => Ok(Some(u)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
/// Find an active employee by a scanned RFID tag (or QR). Returns the
/// Employee (with the linked auto-customer id) or None when unknown.
pub fn find_employee_by_rfid(db: &DbState, rfid: &str) -> Result<Option<Employee>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, employee_code, full_name, phone, email, national_id, job_title, base_salary, salary_type, salary_start_date, hire_date, qr_code, rfid_code, is_active, notes, customer_id
             FROM employees
             WHERE is_active = 1 AND (rfid_code = ?1 OR qr_code = ?1)
             ORDER BY id DESC LIMIT 1",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map([rfid], |row| {
            Ok(Employee {
                id: row.get(0)?,
                employee_code: row.get(1)?,
                full_name: row.get(2)?,
                phone: row.get(3)?,
                email: row.get(4)?,
                national_id: row.get(5)?,
                job_title: row.get(6)?,
                base_salary: row.get(7)?,
                salary_type: row.get(8)?,
                salary_start_date: row.get(9)?,
                hire_date: row.get(10)?,
                qr_code: row.get(11)?,
                rfid_code: row.get(12)?,
                is_active: row.get(13)?,
                notes: row.get(14)?,
                customer_id: row.get(15).ok().flatten(),
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.next().map(|r| r.ok()).flatten())
}

/// RFID login for the login screen: resolve the tag to the employee's
/// linked user account (must be active). No password by design — the card
/// IS the credential.
pub fn login_with_rfid(db: &DbState, rfid: &str) -> Result<Option<crate::models::User>, String> {
    let conn = db.conn.lock().unwrap();
    let user_id: Option<i64> = conn
        .query_row(
            "SELECT user_account_id FROM employees
             WHERE is_active = 1 AND user_account_id IS NOT NULL
               AND (rfid_code = ?1 OR qr_code = ?1)",
            [rfid],
            |r| r.get(0),
        )
        .ok();
    let Some(uid) = user_id else {
        return Ok(None);
    };
    let mut stmt = conn
        .prepare(
            "SELECT u.id, u.username, u.display_name, u.role_id, r.name, u.max_discount_percent, u.is_active
             FROM users u LEFT JOIN roles r ON u.role_id = r.id
             WHERE u.id = ?1 AND u.is_active = 1",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map([uid], |r| {
            Ok(crate::models::User {
                id: r.get(0)?,
                username: r.get(1)?,
                display_name: r.get(2)?,
                role_id: r.get(3)?,
                role_name: r.get(4)?,
                max_discount_percent: r.get(5)?,
                is_active: r.get(6)?,
                permissions: vec![],
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.next().map(|r| r.ok()).flatten())
}

/// Next free employee code ("EMP-NN"), computed server-side against ALL
/// rows — soft-deleted employees keep their code (UNIQUE constraint), so
/// the client-side guess over only the active list collided on save.
pub fn next_employee_code(db: &DbState) -> Result<String, String> {
    let conn = db.conn.lock().unwrap();
    let mut max_num: i64 = 0;
    let mut stmt = conn
        .prepare("SELECT employee_code FROM employees WHERE employee_code LIKE 'EMP-%'")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    for code in rows {
        if let Ok(c) = code {
            if let Some(n) = c.trim().strip_prefix("EMP-") {
                if let Ok(num) = n.parse::<i64>() {
                    max_num = max_num.max(num);
                }
            }
        }
    }
    Ok(format!("EMP-{:02}", max_num + 1))
}
