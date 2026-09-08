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

// ── Admin-station views (direct CRM access via the cloud session) ──────────

/// List org members (id, full_name, role, email, is_active) — admin RLS
/// (0016) scopes to the org. Powers the Field Team view.
#[tauri::command]
pub fn cloud_team_members() -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "profiles",
        "id, full_name, role, email, is_active",
        &[("order", "full_name.asc".into())],
    )?;
    Ok(Value::Array(rows))
}

/// Invite a member (preseller/seller/admin) via the invite-user Edge Function.
#[tauri::command]
pub fn cloud_team_invite(
    email: String,
    full_name: String,
    role: String,
    password: Option<String>,
) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    client.function(
        "invite-user",
        serde_json::json!({ "email": email, "fullName": full_name, "role": role, "password": password }),
    )
}

/// Update a member (name / role / password) via the manage-user Edge Function.
#[tauri::command]
pub fn cloud_team_update(
    user_id: String,
    full_name: Option<String>,
    role: Option<String>,
    password: Option<String>,
) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    client.function(
        "manage-user",
        serde_json::json!({
            "action": "update",
            "userId": user_id,
            "fullName": full_name,
            "role": role,
            "password": password,
        }),
    )
}

/// Delete a member via manage-user (refuses the last active admin server-side).
#[tauri::command]
pub fn cloud_team_delete(user_id: String) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    client.function(
        "manage-user",
        serde_json::json!({ "action": "delete", "userId": user_id }),
    )
}

/// Sellers + presellers (for the Truck Loads picker). Cached pull of profiles.
#[tauri::command]
pub fn cloud_field_staff() -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "profiles",
        "id, full_name, role",
        &[
            ("role", "in.(preseller,seller)".into()),
            ("is_active", "eq.true".into()),
            ("order", "full_name.asc".into()),
        ],
    )?;
    Ok(Value::Array(rows))
}

/// Pending client-deletion requests (admin) — powers the Deletion Requests view.
#[tauri::command]
pub fn cloud_deletion_requests() -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "client_deletion_requests",
        "*, client:clients(name), requested_by_profile:profiles!client_deletion_requests_requested_by_fkey(full_name)",
        &[
            ("status", "eq.pending".into()),
            ("order", "created_at.desc".into()),
        ],
    )?;
    Ok(Value::Array(rows))
}

/// Approve a pending deletion request (deletes the client server-side;
/// FK-rejects with a clear message when the client has orders).
#[tauri::command]
pub fn cloud_approve_deletion(request_id: String) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client
        .rpc("approve_client_deletion", serde_json::json!({ "p_request_id": request_id }))
        .map(|_| ())
}

/// Reject a pending deletion request.
#[tauri::command]
pub fn cloud_reject_deletion(request_id: String) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client
        .rpc("reject_client_deletion", serde_json::json!({ "p_request_id": request_id }))
        .map(|_| ())
}

/// Recent routes (id, date, seller) for the Truck Loads view.
#[tauri::command]
pub fn cloud_recent_routes(limit: Option<i64>) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "routes",
        "id, route_date, seller_id, status",
        &[
            ("order", "route_date.desc".into()),
            ("limit", limit.unwrap_or(30).to_string().into()),
        ],
    )?;
    Ok(Value::Array(rows))
}

/// Orders validated for a route (to auto-fill a truck load): aggregated
/// product quantities + totals from the route's stops.
#[tauri::command]
pub fn cloud_route_order_items(route_id: String) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "route_stops",
        "order_id, items:order_items(product_id, quantity, unit_price, line_total, product:products(name))",
        &[("route_id", format!("eq.{route_id}"))],
    )?;
    // Flatten to one list of lines.
    let mut lines: Vec<serde_json::Value> = vec![];
    for stop in rows {
        if let Some(items) = stop["items"].as_array() {
            for it in items {
                lines.push(it.clone());
            }
        }
    }
    Ok(Value::Array(lines))
}

/// Create a truck load (manifest document — no stock movement at load time).
#[tauri::command]
pub fn cloud_create_truck_load(
    seller_id: String,
    route_id: Option<String>,
    route_date: Option<String>,
    items: Value,
    notes: Option<String>,
) -> Result<String, String> {
    let client = cloud::ensure_session()?;
    let id = client.rpc(
        "create_truck_load",
        serde_json::json!({
            "p_seller_id": seller_id,
            "p_route_id": route_id,
            "p_route_date": route_date,
            "p_items": items,
            "p_notes": notes,
        }),
    )?;
    id.as_str().map(String::from).ok_or_else(|| "no id returned".into())
}

/// Record undelivered goods returning to the warehouse (+return movements).
#[tauri::command]
pub fn cloud_record_truck_return(load_id: String, returns: Value) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client
        .rpc(
            "record_truck_return",
            serde_json::json!({ "p_load_id": load_id, "p_returns": returns }),
        )
        .map(|_| ())
}

/// Existing truck loads (with items) for the Truck Loads list.
#[tauri::command]
pub fn cloud_truck_loads() -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "truck_loads",
        "*, seller:profiles!truck_loads_seller_id_fkey(full_name), items:truck_load_items(quantity, returned_quantity, product:products(name))",
        &[("order", "created_at.desc".into())],
    )?;
    Ok(Value::Array(rows))
}
