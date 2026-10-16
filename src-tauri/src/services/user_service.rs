use crate::database::DbState;
use crate::models::{Role, UserAccount};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use rusqlite::Result;

pub fn get_all_users(db: &DbState) -> Result<Vec<UserAccount>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT u.id, u.username, u.display_name, u.role_id, r.name, u.max_discount_percent, u.is_active, u.last_login, u.created_at,
                    COALESCE(u.pinned, 0)
             FROM users u
             LEFT JOIN roles r ON u.role_id = r.id
             ORDER BY COALESCE(u.pinned, 0) DESC, u.id ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(UserAccount {
                id: row.get(0)?,
                username: row.get(1)?,
                display_name: row.get(2)?,
                role_id: row.get(3)?,
                role_name: row.get(4)?,
                max_discount_percent: row.get(5)?,
                is_active: row.get(6)?,
                last_login: row.get(7)?,
                created_at: row.get(8)?,
                pinned: row.get::<_, Option<i64>>(9).ok().flatten().map(|v| v != 0).unwrap_or(false),
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<UserAccount> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

pub fn get_all_roles(db: &DbState) -> Result<Vec<Role>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, name, description, is_system FROM roles ORDER BY id ASC")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Role {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                is_system: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<Role> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

pub fn create_user(
    db: &DbState,
    username: &str,
    display_name: &str,
    password: &str,
    role_id: Option<i64>,
    max_discount_percent: f64,
) -> Result<i64, String> {
    let clean_username = username.trim().to_lowercase();
    if clean_username.is_empty() {
        return Err("Username cannot be empty".to_string());
    }
    if password.trim().is_empty() {
        return Err("Password cannot be empty".to_string());
    }

    let conn = db.conn.lock().unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM users WHERE LOWER(username) = LOWER(?1)",
            [&clean_username],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if count > 0 {
        return Err(format!("Username '{}' is already taken", clean_username));
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| e.to_string())?
        .to_string();

    conn.execute(
        "INSERT INTO users (username, display_name, password_hash, role_id, max_discount_percent, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, 1)",
        rusqlite::params![clean_username, display_name.trim(), password_hash, role_id, max_discount_percent],
    )
    .map_err(|e| e.to_string())?;

    Ok(conn.last_insert_rowid())
}

pub fn update_user(
    db: &DbState,
    user_id: i64,
    username: &str,
    display_name: &str,
    role_id: Option<i64>,
    max_discount_percent: f64,
    is_active: bool,
    new_password: Option<String>,
) -> Result<(), String> {
    // OWNER PROTECTION: the Owner account is outside admin control — the
    // owner manages his own credentials from his own session (profile /
    // change-password). Admins only see him in Accounts/ذات التبويب.
    {
        let conn = db.conn.lock().unwrap();
        let target_role: String = conn
            .query_row(
                "SELECT COALESCE(r.name, '') FROM users u LEFT JOIN roles r ON u.role_id = r.id WHERE u.id = ?1",
                [user_id],
                |r| r.get(0),
            )
            .unwrap_or_default();
        drop(conn);
        if target_role == "Owner" && new_password.is_some() {
            return Err(
                "The Owner account manages itself: use the owner session's change-password (his own password). / حساب المالك يُدار من طرفه فقط"
                    .to_string(),
            );
        }
        if target_role == "Owner" {
            return Err("The Owner account cannot be edited by admins / لا يمكن تعديل حساب المالك".to_string());
        }
    }

    let clean_username = username.trim().to_lowercase();
    if clean_username.is_empty() {
        return Err("Username cannot be empty".to_string());
    }

    let conn = db.conn.lock().unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM users WHERE LOWER(username) = LOWER(?1) AND id != ?2",
            rusqlite::params![&clean_username, user_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if count > 0 {
        return Err(format!("Username '{}' is already in use by another account", clean_username));
    }

    if let Some(pwd) = new_password {
        let trimmed_pwd = pwd.trim();
        if !trimmed_pwd.is_empty() {
            let salt = SaltString::generate(&mut OsRng);
            let argon2 = Argon2::default();
            let password_hash = argon2
                .hash_password(trimmed_pwd.as_bytes(), &salt)
                .map_err(|e| e.to_string())?
                .to_string();

            conn.execute(
                "UPDATE users SET password_hash = ?1, pin_hash = NULL WHERE id = ?2",
                rusqlite::params![password_hash, user_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    conn.execute(
        "UPDATE users
         SET username = ?1, display_name = ?2, role_id = ?3, max_discount_percent = ?4, is_active = ?5
         WHERE id = ?6",
        rusqlite::params![clean_username, display_name.trim(), role_id, max_discount_percent, is_active, user_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn delete_user(db: &DbState, user_id: i64) -> Result<(), String> {
    if user_id == 1 {
        return Err("Cannot delete the primary system administrator account".to_string());
    }
    // OWNER PROTECTION: the Owner account can never be deleted — by anyone.
    {
        let conn = db.conn.lock().unwrap();
        let target_role: String = conn
            .query_row(
                "SELECT COALESCE(r.name, '') FROM users u LEFT JOIN roles r ON u.role_id = r.id WHERE u.id = ?1",
                [user_id],
                |r| r.get(0),
            )
            .unwrap_or_default();
        drop(conn);
        if target_role == "Owner" {
            return Err(
                "The Owner account cannot be deleted at all / لا يمكن حذف حساب المالك نهائياً".to_string(),
            );
        }
    }

    let conn = db.conn.lock().unwrap();

    // Check if user has associated transactions or sessions
    let sales_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sales WHERE user_id = ?1",
            [user_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let session_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cash_sessions WHERE user_id = ?1",
            [user_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if sales_count > 0 || session_count > 0 {
        // Soft delete to maintain transactional integrity
        conn.execute("UPDATE users SET is_active = 0 WHERE id = ?1", [user_id])
            .map_err(|e| e.to_string())?;
    } else {
        conn.execute("DELETE FROM users WHERE id = ?1", [user_id])
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Pin/unpin a user; pinned users float to the top of the login screen.
pub fn toggle_user_pin(db: &DbState, user_id: i64, pinned: bool) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "UPDATE users SET pinned = ?1 WHERE id = ?2",
        rusqlite::params![if pinned { 1 } else { 0 }, user_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
