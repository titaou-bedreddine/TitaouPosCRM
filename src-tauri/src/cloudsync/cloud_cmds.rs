//! Tauri commands for cloud sync. LOCAL-ONLY: never forwarded over the LAN
//! (excluded from should_forward_ipc's whitelist — see network/invoke_registry).
//! On a LAN client terminal, the status command reports coordinator=false so
//! the UI can show "runs on the server terminal".

use super::{self as cloud, pull};
use crate::database::DbState;
use serde_json::Value;
use tauri::State;

/// Configure + connect: store URL/anon key/email, sign in ONCE with the
/// password (not persisted — only the refresh token is), verify the admin
/// profile, and run the first cycle (bootstrap: ensure_pos_client + product
/// linking pass happen inside pull).
#[tauri::command]
pub fn cloud_configure(
    db: State<'_, DbState>,
    url: String,
    anon_key: String,
    email: String,
    password: String,
) -> Result<Value, String> {
    if !cloud::is_coordinator() {
        return Err("Cloud sync runs on the server terminal — configure it there".into());
    }
    let url = url.trim_end_matches('/').to_string();
    if !url.starts_with("https://") {
        return Err("URL must start with https://".into());
    }
    if anon_key.is_empty() || email.is_empty() || password.is_empty() {
        return Err("anonymos key, email and password are required".into());
    }

    // Sign in immediately (validates credentials before saving anything).
    let session = cloud::auth::sign_in(&url, &anon_key, &email, &password)?;
    let client = cloud::http::SupabaseClient::new(
        &url,
        &anon_key,
        &session.access_token,
    );
    let profile = cloud::auth::fetch_profile(&client)?;
    let role = profile["role"].as_str().unwrap_or("");
    let is_active = profile["is_active"].as_bool().unwrap_or(false);
    if role != "admin" || !is_active {
        return Err(format!("This CRM account is not an active admin (role: {role}) — the POS requires the owner's admin account"));
    }

    // Persist config (password deliberately NOT stored).
    {
        let conn = db.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES
              ('cloud_url', ?1), ('cloud_anon_key', ?2), ('cloud_email', ?3),
              ('cloud_refresh_token', ?4), ('cloud_enabled', 'true')",
            rusqlite::params![
                url,
                anon_key,
                email,
                session.refresh_token
            ],
        );
    }
    *cloud::state().session.lock().unwrap() = Some(session);
    cloud::load_config(&db);

    // First cycle now — surfaces any pull/bootstrap problem immediately.
    let result = cloud::cycle(&db)?;
    Ok(result)
}

/// Live status snapshot (safe on client terminals — reports enabled=false +
/// coordinator=false there).
#[tauri::command]
pub fn cloud_get_status(db: State<'_, DbState>) -> Result<Value, String> {
    let mut s = cloud::status(&db);
    if let Value::Object(map) = &mut s {
        map.insert("coordinator".into(), Value::Bool(cloud::is_coordinator()));
    }
    Ok(s)
}

/// Manual "Sync now" (admin button) — one cycle + fresh counts.
#[tauri::command]
pub fn cloud_sync_now(db: State<'_, DbState>) -> Result<Value, String> {
    if !cloud::is_coordinator() {
        return Err("Cloud sync runs on the server terminal".into());
    }
    cloud::cycle(&db)
}

/// Disconnect: clear config + session (outbox rows keep pending state —
/// reconnecting resumes; nothing is lost).
#[tauri::command]
pub fn cloud_disconnect(db: State<'_, DbState>) -> Result<(), String> {
    {
        let conn = db.conn.lock().unwrap();
        let _ = conn.execute_batch(
            "UPDATE app_settings SET value = 'false' WHERE key = 'cloud_enabled';
             DELETE FROM app_settings WHERE key = 'cloud_refresh_token';",
        );
    }
    *cloud::state().session.lock().unwrap() = None;
    *cloud::state().org_id.lock().unwrap() = None;
    cloud::state().online.store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// Connectivity test without saving anything (URL + key + email + password).
#[tauri::command]
pub fn cloud_test_connection(
    url: String,
    anon_key: String,
    email: String,
    password: String,
) -> Result<Value, String> {
    let session = cloud::auth::sign_in(
        url.trim_end_matches('/'),
        &anon_key,
        &email,
        &password,
    )?;
    let client = cloud::http::SupabaseClient::new(
        url.trim_end_matches('/'),
        &anon_key,
        &session.access_token,
    );
    let profile = cloud::auth::fetch_profile(&client)?;
    Ok(serde_json::json!({
        "email": profile["email"],
        "role": profile["role"],
        "organization_id": profile["organization_id"],
        "full_name": profile["full_name"],
    }))
}

// ── Read-only mirror accessors (used by the Field Orders view) ─────────────

#[tauri::command]
pub fn cloud_field_orders(db: State<'_, DbState>, limit: Option<i64>) -> Result<Value, String> {
    let conn = db.conn.lock().unwrap();
    pull::list_field_orders(&conn, limit.unwrap_or(200))
}

#[tauri::command]
pub fn cloud_field_order_lines(
    db: State<'_, DbState>,
    crm_id: String,
) -> Result<Value, String> {
    let conn = db.conn.lock().unwrap();
    pull::field_order_lines(&conn, &crm_id)
}
