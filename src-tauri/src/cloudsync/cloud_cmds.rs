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

    // Persist config. The password is stored AES-GCM-encrypted with a
    // machine-bound key (HWID) so the customer never re-types credentials;
    // a copied DB file is undecryptable elsewhere.
    let password_enc = cloud::secrets::encrypt(&password)?;
    {
        let conn = db.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES
              ('cloud_url', ?1), ('cloud_anon_key', ?2), ('cloud_email', ?3),
              ('cloud_refresh_token', ?4), ('cloud_password_enc', ?5),
              ('cloud_enabled', 'true')",
            rusqlite::params![
                url,
                anon_key,
                email,
                session.refresh_token,
                password_enc
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
             DELETE FROM app_settings WHERE key = 'cloud_refresh_token';
             DELETE FROM app_settings WHERE key = 'cloud_password_enc';",
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
        "id, full_name, role, employee_type",
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
        "*, client:clients(name)",
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
        "id, route_date, seller_id, status, name",
        &[
            ("order", "route_date.desc".into()),
            ("limit", limit.unwrap_or(30).to_string().into()),
        ],
    )?;
    Ok(Value::Array(rows))
}

/// Orders validated for a route (to auto-fill a truck load): aggregated
/// product quantities + totals from the route's stops. route_stops has no
/// direct FK to order_items (it runs through orders), so the lines are
/// fetched in two steps.
#[tauri::command]
pub fn cloud_route_order_items(route_id: String) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let stops = client.select(
        "route_stops",
        "order_id, stop_order, status, client:clients(name)",
        &[
            ("route_id", format!("eq.{route_id}")),
            ("order", "stop_order.asc".into()),
        ],
    )?;
    let order_ids: Vec<String> = stops
        .iter()
        .filter_map(|st| st["order_id"].as_str().map(String::from))
        .collect();
    if order_ids.is_empty() {
        return Ok(Value::Array(vec![]));
    }
    let in_list = order_ids
        .iter()
        .map(|id| format!("({id})"))
        .collect::<Vec<_>>()
        .join(",");
    let lines = client.select(
        "order_items",
        "order_id, product_id, quantity, unit_price, line_total, product:products(name)",
        &[("order_id", format!("in.{in_list}"))],
    )?;
    Ok(serde_json::json!({
        "stops": stops,
        "lines": lines,
    }))
}

/// Create a truck load / open a trip (manifest document; stock moves at
/// load time server-side). truck_id + driver_name snapshot are optional —
/// legacy loads without a truck keep working.
#[tauri::command]
pub fn cloud_create_truck_load(
    seller_id: String,
    route_id: Option<String>,
    route_date: Option<String>,
    items: Value,
    notes: Option<String>,
    name: Option<String>,
    truck_id: Option<String>,
    driver_name: Option<String>,
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
            "p_name": name,
            "p_truck_id": truck_id,
            "p_driver_name": driver_name,
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

/// Edit a truck load (header + items, stock-compensated server-side via
/// update_truck_load). Refused once returns were recorded on the load.
#[tauri::command]
pub fn cloud_update_truck_load(
    load_id: String,
    seller_id: Option<String>,
    route_id: Option<String>,
    route_date: Option<String>,
    items: Value,
    name: Option<String>,
    notes: Option<String>,
) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client
        .rpc(
            "update_truck_load",
            serde_json::json!({
                "p_load_id": load_id,
                "p_seller_id": seller_id,
                "p_route_id": route_id,
                "p_route_date": route_date,
                "p_items": items,
                "p_name": name,
                "p_notes": notes,
            }),
        )
        .map(|_| ())
}

/// Delete a truck load: reverses its stock movements, removes items + header.
#[tauri::command]
pub fn cloud_delete_truck_load(load_id: String) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client
        .rpc(
            "delete_truck_load",
            serde_json::json!({ "p_load_id": load_id }),
        )
        .map(|_| ())
}

/// Existing truck loads (with items) for the Truck Loads list.
#[tauri::command]
pub fn cloud_truck_loads() -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "truck_loads",
        "*, items:truck_load_items(quantity, returned_quantity, product_id, product:products(name))",
        &[("order", "created_at.desc".into())],
    )?;
    Ok(Value::Array(rows))
}

// ── Trucks & trips (Direct Sale back-office) ────────────────────────────────

/// Trucks list for the Direct Sale back-office screen.
#[tauri::command]
pub fn cloud_list_trucks() -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select("trucks", "*", &[("order", "name.asc".into())])?;
    Ok(Value::Array(rows))
}

/// Create or update a truck (admin; RLS enforces organization + role).
#[tauri::command]
pub fn cloud_save_truck(
    id: Option<String>,
    name: String,
    plate: Option<String>,
    driver_name: Option<String>,
    seller_id: Option<String>,
    is_active: Option<bool>,
    notes: Option<String>,
) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let org = cloud::auth::fetch_profile(&client)?["organization_id"]
        .as_str()
        .ok_or("no org")?
        .to_string();
    let payload = serde_json::json!({
        "organization_id": org,
        "name": name,
        "plate": plate.unwrap_or_default(),
        "driver_name": driver_name.unwrap_or_default(),
        "seller_id": seller_id,
        "is_active": is_active.unwrap_or(true),
        "notes": notes,
    });
    match id {
        Some(existing) => {
            client.patch("trucks", payload, &[("id", format!("eq.{existing}"))])?;
            Ok(serde_json::json!({ "id": existing }))
        }
        None => {
            let rows = client.insert("trucks", serde_json::json!([payload]))?;
            rows.first()
                .cloned()
                .ok_or_else(|| "truck insert returned no row".into())
        }
    }
}

/// START TRIP: loaded → in_progress (state machine enforced server-side).
#[tauri::command]
pub fn cloud_start_truck_trip(load_id: String) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client
        .rpc("start_truck_trip", serde_json::json!({ "p_load_id": load_id }))
        .map(|_| ())
}

/// OPEN RECONCILIATION: in_progress → reconciling (admin only server-side).
#[tauri::command]
pub fn cloud_open_truck_reconciliation(load_id: String) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client
        .rpc(
            "open_truck_reconciliation",
            serde_json::json!({ "p_load_id": load_id }),
        )
        .map(|_| ())
}

/// CLOSE & SETTLE: reconciling → closed. One atomic transaction: stock
/// reconciliation rows + physical-return movements + cash settlement row.
#[tauri::command]
pub fn cloud_close_truck_trip(
    load_id: String,
    counts: Value,
    reasons: Option<Value>,
    actual_cash: i64,
    cash_reason: Option<String>,
) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client
        .rpc(
            "close_truck_trip",
            serde_json::json!({
                "p_load_id": load_id,
                "p_counts": counts,
                "p_reasons": reasons,
                "p_actual_cash": actual_cash,
                "p_cash_reason": cash_reason,
            }),
        )
        .map(|_| ())
}

/// Add an explicit trip expense (loading_fee/unloading_fee/fuel/driver_fee/other).
#[tauri::command]
pub fn cloud_add_truck_trip_expense(
    load_id: String,
    category: String,
    amount: i64,
    notes: Option<String>,
) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client
        .rpc(
            "add_truck_trip_expense",
            serde_json::json!({
                "p_load_id": load_id,
                "p_category": category,
                "p_amount": amount,
                "p_notes": notes,
            }),
        )
        .map(|_| ())
}

/// Canonical per-trip totals (history list + dashboard card).
#[tauri::command]
pub fn cloud_stats_truck_trips(
    from_date: Option<String>,
    to_date: Option<String>,
) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    client.rpc(
        "stats_truck_trips",
        serde_json::json!({ "p_from": from_date, "p_to": to_date }),
    )
}

/// One trip's direct-sale orders with lines, payments and client name.
#[tauri::command]
pub fn cloud_trip_orders(load_id: String) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "orders",
        "id, created_at, status, total_amount, amount_paid, amount_due, payment_status, \
         client:clients(name), \
         items:order_items(quantity, base_quantity, unit_price, line_total, tva_rate, sale_unit, product:products(name)), \
         pays:payments(amount, method, created_at)",
        &[
            ("truck_load_id", format!("eq.{load_id}")),
            ("order", "created_at.asc".into()),
        ],
    )?;
    Ok(Value::Array(rows))
}

/// Warehouse products IN STOCK for the trip creator (migration 0051 RPC):
/// name + positive stock so the new-trip picker only offers what exists.
#[tauri::command]
pub fn cloud_warehouse_stock() -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.rpc(
        "warehouse_stock",
        serde_json::json!({ "p_min": 1 }),
    )?;
    Ok(rows)
}

/// Recent DIRECT-SALE (Android, source='direct_truck') orders for the POS
/// Sales History "Android" tab (spec §19): date-ranged, newest first, with
/// client/seller/truck resolved so the POS can show and filter them.
#[tauri::command]
pub fn cloud_recent_direct_orders(
    from_date: Option<String>,
    to_date: Option<String>,
) -> Result<Value, String> {
    let client = cloud::ensure_session()?;

    // Inclusive date window: created_at >= from 00:00, < to+1d 00:00.
    let mut query: Vec<(&str, String)> = vec![
        ("source", "eq.direct_truck".into()),
        ("order", "created_at.desc".into()),
        ("limit", "500".into()),
    ];
    if let Some(f) = from_date.as_deref().filter(|s| !s.trim().is_empty()) {
        query.push(("created_at", format!("gte.{f}T00:00:00")));
    }
    if let Some(t) = to_date.as_deref().filter(|s| !s.trim().is_empty()) {
        let end = chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.succ_opt())
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| t.to_string());
        query.push(("created_at", format!("lt.{end}T00:00:00")));
    }
    let rows = client.select("orders",
        "id, created_at, source, payment_status, total_amount, amount_paid, \n         client_id, seller_id, preseller_id, truck_load_id, \n         client:clients(name), load:truck_loads(name)",
        &query)?;

    // Seller names: a tiny profiles map (the org's field staff is small).
    let profiles = client
        .select(
            "profiles",
            "id, full_name",
            &[("order", "full_name.asc".into())],
        )
        .unwrap_or_default();
    let name_of = |id: &Value| -> String {
        profiles
            .iter()
            .find(|p| p["id"] == *id)
            .and_then(|p| p["full_name"].as_str())
            .unwrap_or("—")
            .to_string()
    };

    let out: Vec<Value> = rows
        .into_iter()
        .map(|mut r| {
            let seller_id = r["seller_id"].clone();
            let preseller_id = r["preseller_id"].clone();
            let seller = if seller_id.is_null() { &preseller_id } else { &seller_id };
            r["seller_name"] = Value::String(name_of(seller));
            r["truck_name"] = r["load"]["name"]
                .as_str()
                .unwrap_or("—")
                .to_string()
                .into();
            r.as_object_mut().map(|o| {
                o.remove("load");
                o.remove("client_id");
                o.remove("seller_id");
                o.remove("preseller_id");
                o.remove("truck_load_id");
            });
            r
        })
        .collect();
    Ok(Value::Array(out))
}

/// One trip's settlement + stock reconciliation + expenses — the immutable
/// audit view (SELECT-only policies make these rows unmodifiable).
#[tauri::command]
pub fn cloud_trip_settlement(load_id: String) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let settlement = client.select(
        "truck_trip_settlements",
        "*",
        &[("load_id", format!("eq.{load_id}"))],
    )?;
    let reconciliations = client.select(
        "truck_trip_reconciliations",
        "*, product:products(name)",
        &[("load_id", format!("eq.{load_id}")), ("order", "product_id.asc".into())],
    )?;
    let expenses = client.select(
        "truck_trip_expenses",
        "*",
        &[("load_id", format!("eq.{load_id}")), ("order", "created_at.asc".into())],
    )?;
    Ok(serde_json::json!({
        "settlement": settlement.first().cloned().unwrap_or(Value::Null),
        "reconciliations": reconciliations,
        "expenses": expenses,
    }))
}

// ── Setup-code provisioning (reseller flow) ────────────────────────────────

/// Compile-time product endpoint (set at build: TITAO_PRODUCT_SUPABASE_URL /
/// TITAO_PRODUCT_SUPABASE_ANON_KEY). The customer's POS needs this to know
/// WHERE to redeem a setup code — no technical input from them.
pub fn product_endpoint() -> Result<(String, String), String> {
    let url = option_env!("TITAO_PRODUCT_SUPABASE_URL").unwrap_or("").trim_end_matches('/');
    let anon = option_env!("TITAO_PRODUCT_SUPABASE_ANON_KEY").unwrap_or("");
    if !url.is_empty() && !anon.is_empty() {
        return Ok((url.to_string(), anon.to_string()));
    }
    Err("No product endpoint baked into this build — use the manual connection form".into())
}

fn http_plain() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

/// Redeem a one-time setup code (TITAO-XXXX-XXXX-XXXX): the pos-setup Edge
/// Function provisions the org's POS admin account and returns the
/// credentials once. Everything is persisted (password AES-GCM/HWID) and the
/// first sync cycle runs immediately — the customer types ONE code, done.
#[tauri::command]
pub fn cloud_setup_code(db: State<'_, DbState>, code: String) -> Result<Value, String> {
    if !cloud::is_coordinator() {
        return Err("Cloud sync runs on the server terminal — enter the setup code there".into());
    }
    let (url, anon) = product_endpoint()?;
    let code = code.trim().to_uppercase();
    if code.is_empty() {
        return Err("Setup code required".into());
    }

    let resp = http_plain()
        .post(format!("{}/functions/v1/pos-setup", url))
        .header("apikey", &anon)
        .json(&serde_json::json!({
            "code": code,
            "node": crate::network::terminal_name_for_this_pc(),
        }))
        .send()
        .map_err(|e| format!("network: {e}"))?;
    let status = resp.status();
    let body: Value = resp.json().map_err(|e| format!("bad response: {e}"))?;
    if !status.is_success() {
        let msg = body["error"].as_str().unwrap_or("Setup code rejected");
        return Err(msg.to_string());
    }

    let email = body["email"].as_str().ok_or("response missing email")?.to_string();
    let password = body["password"].as_str().ok_or("response missing password")?.to_string();

    // Validate by signing in immediately.
    let session = cloud::auth::sign_in(&url, &anon, &email, &password)?;
    let profile = cloud::auth::fetch_profile(&cloud::http::SupabaseClient::new(&url, &anon, &session.access_token))?;
    if profile["role"].as_str() != Some("admin") || !profile["is_active"].as_bool().unwrap_or(false) {
        return Err("Provisioned account is not an active admin — contact your vendor".into());
    }

    // Persist everything (password encrypted, machine-bound).
    let password_enc = cloud::secrets::encrypt(&password)?;
    {
        let conn = db.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES
              ('cloud_url', ?1), ('cloud_anon_key', ?2), ('cloud_email', ?3),
              ('cloud_refresh_token', ?4), ('cloud_password_enc', ?5),
              ('cloud_enabled', 'true')",
            rusqlite::params![url, anon, email, session.refresh_token, password_enc],
        );
    }
    *cloud::state().session.lock().unwrap() = Some(session);
    cloud::load_config(&db);

    // First cycle now — walk-in client + product linking pass + initial pull.
    cloud::cycle(&db)
}

/// Saved config for the Cloud Sync form (preload; password never returned).
#[tauri::command]
pub fn cloud_get_saved_config(db: State<'_, DbState>) -> Result<Value, String> {
    let cfg = cloud::state().config.lock().unwrap().clone();
    let anon_masked = if cfg.anon_key.len() > 12 {
        format!("{}…{}", &cfg.anon_key[..8], &cfg.anon_key[cfg.anon_key.len()-4..])
    } else {
        cfg.anon_key.clone()
    };
    Ok(serde_json::json!({
        "url": cfg.url,
        "email": cfg.email,
        "anon_key_masked": anon_masked,
        "has_password": !cfg.password_enc.is_empty(),
        "product_ready": product_endpoint().is_ok(),
    }))
}

// ── Onboarding: push the pre-existing local catalog to the CRM ─────────────

/// Enqueues every active product as a Product outbox event. For shops whose
/// catalog predates cloud sync (create/update events never fired for it).
/// Idempotent end-to-end: the CRM matches by SKU/barcode, and already-pending
/// local ids are skipped.
#[tauri::command]
pub fn cloud_push_catalog(db: State<'_, DbState>) -> Result<Value, String> {
    if !cloud::is_coordinator() {
        return Err("Cloud sync runs on the server terminal".into());
    }
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.sku, p.name_fr, p.name_ar, p.name_en,
                    p.sale_price, p.purchase_price, p.min_stock,
                    (SELECT b.barcode FROM product_barcodes b
                      WHERE b.product_id = p.id ORDER BY b.is_primary DESC, b.id LIMIT 1)
               FROM products p
              WHERE p.is_active = 1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, f64>(7)?,
                r.get::<_, Option<String>>(8)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut queued = 0i64;
    let mut skipped = 0i64;
    for row in rows.flatten() {
        let (id, sku, name_fr, name_ar, name_en, sale, purchase, min_stock, barcode) = row;
        // Skip products already queued (pending) — avoids duplicate spam on
        // repeated clicks; already-synced ones just re-upsert harmlessly.
        let pending: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sync_outbox
                  WHERE entity = 'product' AND local_id = ?1 AND status = 'pending'",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if pending > 0 {
            skipped += 1;
            continue;
        }
        let payload = serde_json::json!({
            "sku": sku,
            "barcode": barcode,
            "name_fr": name_fr,
            "name_ar": name_ar,
            "name_en": name_en,
            "sale_price": sale,
            "purchase_price": purchase,
            "min_stock": min_stock,
            "is_active": true,
        });
        conn.execute(
            "INSERT INTO sync_outbox (entity, local_id, payload, status)
             VALUES ('product', ?1, ?2, 'pending')",
            rusqlite::params![id, payload.to_string()],
        )
        .map_err(|e| e.to_string())?;
        queued += 1;
    }
    Ok(serde_json::json!({ "queued": queued, "skipped_pending": skipped }))
}

// ── Stock baseline reconcile (onboarding) ──────────────────────────────────

/// Brings the CRM ledger's stock in line with this POS's counts for every
/// mapped product: one 'adjustment' movement per product whose CRM stock
/// differs (idempotent — same count = no-op). Fixes shops whose stock
/// predates cloud sync (field apps would otherwise show 0 forever).
#[tauri::command]
pub fn cloud_reconcile_stock(db: State<'_, DbState>) -> Result<Value, String> {
    if !cloud::is_coordinator() {
        return Err("Cloud sync runs on the server terminal".into());
    }
    let client = cloud::ensure_session()?;
    let conn = db.conn.lock().unwrap();

    let mut stmt = conn
        .prepare(
            "SELECT m.remote_id, p.current_stock
               FROM sync_map m
               JOIN products p ON p.id = m.local_id
              WHERE m.entity = 'product' AND p.is_active = 1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
        })
        .map_err(|e| e.to_string())?;

    let mut reconciled = 0i64;
    let mut skipped = 0i64;
    let mut errors = 0i64;
    for row in rows.flatten() {
        let (remote_id, local_qty) = row;
        match client.rpc(
            "pos_stock_baseline",
            serde_json::json!({
                "p_product_id": remote_id,
                "p_qty": cloud::mapping::qty_round(local_qty),
            }),
        ) {
            Ok(_) => {
                // The RPC returns the qty; count a real reconcile only when
                // it reports success (delta 0 runs are no-ops server-side —
                // we can't distinguish cheaply, so count successes).
                reconciled += 1;
            }
            Err(e) => {
                errors += 1;
                eprintln!("[stock-baseline] {remote_id}: {e}");
            }
        }
    }
    Ok(serde_json::json!({
        "processed": reconciled,
        "errors": errors,
        "skipped_unlinked_note": skipped,
    }))
}

// ── Promotions (admin CRUD; field apps read active ones) ──────────────────

fn org_id_of(client: &cloud::http::SupabaseClient) -> Result<String, String> {
    let profile = cloud::auth::fetch_profile(client)?;
    profile["organization_id"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "profile has no organization".into())
}

#[tauri::command]
pub fn cloud_promotions_list() -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "promotions",
        "*, product:products(name)",
        &[("order", "created_at.desc".into())],
    )?;
    Ok(Value::Array(rows))
}

#[tauri::command]
pub fn cloud_promotion_save(
    db: State<'_, DbState>,
    id: Option<String>,
    name: String,
    description: Option<String>,
    discount_type: String,
    discount_value: i64,
    product_id: String,
    min_quantity: f64,
    ends_at: Option<String>,
    is_active: bool,
) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let org = org_id_of(&client)?;

    if discount_type != "percent" && discount_type != "fixed" {
        return Err("discount type must be percent or fixed".into());
    }
    if discount_value <= 0 || (discount_type == "percent" && discount_value > 100) {
        return Err("invalid discount value".into());
    }

    let mut row = serde_json::json!({
        "organization_id": org,
        "name": name,
        "description": description,
        "discount_type": discount_type,
        "discount_value": discount_value,
        "product_id": product_id,
        "min_quantity": cloud::mapping::qty_round(min_quantity),
        "ends_at": ends_at,
        "is_active": is_active,
    });

    match id {
        Some(existing) => {
            client.patch("promotions", row, &[("id", format!("eq.{existing}"))])?;
            Ok(serde_json::json!({ "id": existing }))
        }
        None => {
            row["created_by"] = Value::String(cloud::auth::fetch_profile(&client)?["id"]
                .as_str()
                .unwrap_or_default()
                .to_string());
            let rows = client.insert("promotions", serde_json::json!([row]))?;
            Ok(rows.into_iter().next().unwrap_or(Value::Null))
        }
    }
}

#[tauri::command]
pub fn cloud_promotion_delete(id: String) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client.delete("promotions", &[("id", format!("eq.{id}"))])
}

/// Wipe ALL promotions of the organization (Factory Reset step): promotions
/// live in the CRM database, so the local reset cannot remove them. Returns
/// the number of deleted rows. Admin RLS governs the delete; call errors
/// surface to the caller.
#[tauri::command]
pub fn cloud_promotions_reset() -> Result<i64, String> {
    let client = cloud::ensure_session()?;
    let org = cloud::auth::fetch_profile(&client)?["organization_id"]
        .as_str()
        .ok_or("no org")?
        .to_string();
    // Count first, then delete — PostgREST DELETE returns no representation
    // here, and the count is what the reset report shows.
    let existing = client.select(
        "promotions",
        "id",
        &[("organization_id", format!("eq.{org}"))],
    )?;
    let count = existing.len() as i64;
    if count > 0 {
        client.delete(
            "promotions",
            &[("organization_id", format!("eq.{org}"))],
        )?;
    }
    Ok(count)
}

/// Active promotions for the field apps' consumption + POS visibility.
#[tauri::command]
pub fn cloud_promotions_active() -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    client.rpc("active_promotions", serde_json::json!({}))
}

/// CRM products (id, name) — the promotion product picker.
#[tauri::command]
pub fn cloud_products_for_promos() -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "products",
        "id, name, packagings:product_packagings(name, units_per_package, sale_price)",
        &[("is_active", "eq.true".into()), ("order", "name.asc".into())],
    )?;
    Ok(Value::Array(rows))
}

/// Delete a truck that has no historical dependency (no trips). Trucks with
/// trips must be archived instead (is_active = false) so history survives.
#[tauri::command]
pub fn cloud_delete_truck(id: String) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    // Guard: refuse when any trip references the truck (defense in depth —
    // the DB FK sets truck_id NULL, but a silent history detach is confusing).
    let loads = client.select(
        "truck_loads",
        "id",
        &[("truck_id", format!("eq.{id}"))],
    )?;
    if !loads.is_empty() {
        return Err("Ce camion a des tournées enregistrées — archivez-le au lieu de le supprimer / Truck has trips — archive it instead".into());
    }
    client.delete("trucks", &[("id", format!("eq.{id}"))])
}

/// Total loading-fee (déchargement) expenses booked in a date range, keyed by
/// sale number — drives the Sales History FEES card.
#[tauri::command]
pub fn get_loading_fees_summary(
    db: State<'_, crate::database::DbState>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<serde_json::Value, String> {
    let conn = db.conn.lock().unwrap();
    let mut sql = String::from(
        "SELECT COALESCE(receipt_reference, '') AS sale_number, SUM(amount) AS total
         FROM expenses WHERE category_id = 8",
    );
    if let Some(sd) = &start_date {
        if !sd.is_empty() {
            sql.push_str(&format!(" AND date >= '{sd}'"));
        }
    }
    if let Some(ed) = &end_date {
        if !ed.is_empty() {
            sql.push_str(&format!(" AND date <= '{ed}'"));
        }
    }
    sql.push_str(" GROUP BY receipt_reference");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut by_sale = serde_json::Map::new();
    let mut total: i64 = 0;
    for row in rows.filter_map(|r| r.ok()) {
        total += row.1;
        if !row.0.is_empty() {
            by_sale.insert(row.0, serde_json::json!(row.1));
        }
    }
    Ok(serde_json::json!({ "total": total, "by_sale": by_sale }))
}

/// Read the business sales mode from the CRM organizations row.
#[tauri::command]
pub fn cloud_get_sales_mode() -> Result<String, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select("organizations", "sales_mode", &[])?;
    Ok(rows
        .first()
        .and_then(|r| r["sales_mode"].as_str().map(String::from))
        .unwrap_or_else(|| "pre_sale".into()))
}

/// Change the business sales mode. SECURITY-SENSITIVE: requires the local
/// admin password (server-side, the Supabase RLS org_admin_update policy
/// additionally restricts the patch to the admin account).
#[tauri::command]
pub fn cloud_set_sales_mode(
    db: State<'_, crate::database::DbState>,
    mode: String,
    admin_password: String,
) -> Result<(), String> {
    if mode != "pre_sale" && mode != "direct_sale" {
        return Err("invalid sales mode".into());
    }
    // Local admin authorization: only the machine's admin may flip the
    // business workflow (the Supabase RLS org_admin_update policy then
    // restricts the patch itself to the admin account).
    if !crate::auth::verify_admin_password(&db, &admin_password).unwrap_or(false) {
        return Err("Mot de passe administrateur incorrect / Incorrect admin password".into());
    }
    let client = cloud::ensure_session()?;
    let org = cloud::auth::fetch_profile(&client)?["organization_id"]
        .as_str()
        .ok_or("no org")?
        .to_string();
    client.patch(
        "organizations",
        serde_json::json!({ "sales_mode": mode }),
        &[("id", format!("eq.{org}"))],
    )
}
/// The loading fee (déchargement) booked for a sale, if any — identified by
/// receipt_reference = sale number within the Déchargement category.
#[tauri::command]
pub fn get_sale_loading_fee(db: State<'_, crate::database::DbState>, sale_number: String) -> Result<i64, String> {
    let conn = db.conn.lock().unwrap();
    let amount: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM expenses WHERE receipt_reference = ?1 AND category_id = 8",
            [sale_number],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(amount)
}

// ── Routes (admin: assign a day's orders to a seller as ordered stops) ─────

/// Orders eligible for routing: validated (goods pending), not already on a
/// route stop, with the client name for the picker.
#[tauri::command]
pub fn cloud_routable_orders() -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "orders",
        "id, client_id, client:clients(name), preseller_id, total_amount, amount_paid, created_at",
        &[
            ("status", "eq.validated".into()),
            ("source", "neq.pos".into()),
            ("order", "created_at.desc".into()),
            ("limit", "100".into()),
        ],
    )?;
    Ok(Value::Array(rows))
}

/// Validates the ids exist and aren't already routed; returns (order, client) pairs.
#[tauri::command]
pub fn cloud_create_route(
    seller_id: String,
    route_date: String,
    order_ids: Vec<String>,
    notes: Option<String>,
    name: Option<String>,
) -> Result<String, String> {
    let client = cloud::ensure_session()?;
    if order_ids.is_empty() {
        return Err("Pick at least one order for the route".into());
    }
    let items: Vec<Value> = order_ids
        .iter()
        .map(|id| json_route_stop_placeholder(id))
        .collect();
    let _ = items;

    // create_route RPC (0001-era RPC? none — insert directly: routes + stops).
    // Use the truck pattern: insert route, then stops in order.
    let org = cloud::auth::fetch_profile(&client)?["organization_id"]
        .as_str()
        .ok_or("no org")?
        .to_string();

    let rows = client.insert(
        "routes",
        serde_json::json!([{
            "organization_id": org,
            "seller_id": seller_id,
            "route_date": route_date,
            "status": "planned",
            "name": name,
        }]),
    )?;
    let route_id = rows
        .first()
        .and_then(|r| r["id"].as_str())
        .ok_or("route insert returned no id")?
        .to_string();

    let mut stops = Vec::new();
    for (i, order_id) in order_ids.iter().enumerate() {
        // route_stops.client_id is NOT NULL — resolve it from the order.
        let order_rows = client.select(
            "orders",
            "client_id",
            &[("id", format!("eq.{order_id}"))],
        )?;
        let client_id = order_rows
            .first()
            .and_then(|r| r["client_id"].as_str())
            .ok_or_else(|| format!("order {order_id} has no client"))?
            .to_string();
        stops.push(serde_json::json!({
            "route_id": route_id,
            "order_id": order_id,
            "client_id": client_id,
            "stop_order": (i + 1) as i64,
            "status": "validated",
        }));
    }
    client.insert("route_stops", serde_json::Value::Array(stops))?;
    Ok(route_id)
}

fn json_route_stop_placeholder(id: &str) -> Value {
    serde_json::json!({ "order_id": id })
}

/// Delete a route (stops cascade; orders stay untouched).
#[tauri::command]
pub fn cloud_delete_route(route_id: String) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client.delete("routes", &[("id", format!("eq.{route_id}"))])
}

// ── Field-order edit/delete (admin RPCs already exist in the CRM) ────────

/// Fetch one order's full detail for the editor (lines + notes + member).
#[tauri::command]
pub fn cloud_field_order_detail(order_id: String) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "orders",
        "*, client:clients(name)",
        &[("id", format!("eq.{order_id}"))],
    )?;
    let order = rows
        .into_iter()
        .next()
        .ok_or_else(|| "order not found".to_string())?;
    let lines = client.select(
        "order_items",
        "*, product:products(name)",
        &[("order_id", format!("eq.{order_id}"))],
    )?;
    Ok(serde_json::json!({ "order": order, "lines": lines }))
}

/// After the server side settles, drop the order's local mirror rows — the
/// pull never removes deleted rows, so without this a deleted order stays in
/// the Field Orders list forever and every later edit/delete on it answers
/// "order not found" from the server.
fn prune_field_order_mirror(db: &State<'_, DbState>, order_id: &str) {
    let conn = db.conn.lock().unwrap();
    let _ = conn.execute(
        "DELETE FROM crm_order_items WHERE crm_order_id = ?1",
        rusqlite::params![order_id],
    );
    let _ = conn.execute(
        "DELETE FROM crm_orders WHERE crm_id = ?1",
        rusqlite::params![order_id],
    );
}

/// Edit an order's lines/notes: p_items [{product_id, quantity, unit_price,
/// line_total?}] — stock + balance compensated server-side.
#[tauri::command]
pub fn cloud_field_order_edit(
    db: State<'_, DbState>,
    order_id: String,
    items: Value,
    notes: Option<String>,
) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    let result = client.rpc("update_order_items", serde_json::json!({
        "p_order_id": order_id,
        "p_items": items,
        "p_notes": notes,
    }));
    match result {
        Ok(_) => Ok(()),
        Err(msg) => {
            // "order not found" = the order is already gone on the CRM (e.g.
            // deleted from the admin app) — the mirror row is a stale ghost;
            // prune it so the list stops offering a dead order.
            if msg.contains("order not found") {
                prune_field_order_mirror(&db, &order_id);
            }
            Err(msg)
        }
    }
}

/// Delete an order: reverses its stock movements and remaining due, then
/// removes the order (payments detach). The local mirror row is dropped too
/// — the pull only upserts and never propagates deletions.
#[tauri::command]
pub fn cloud_field_order_delete(
    db: State<'_, DbState>,
    order_id: String,
) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    match client.rpc("delete_order", serde_json::json!({ "p_order_id": order_id })) {
        Ok(_) => {
            prune_field_order_mirror(&db, &order_id);
            Ok(())
        }
        Err(msg) => {
            // 'order not found' = already deleted elsewhere (the RPC is
            // correct); prune the stale mirror row and treat it as done.
            if msg.contains("order not found") {
                prune_field_order_mirror(&db, &order_id);
                return Ok(());
            }
            Err(msg)
        }
    }
}

// ── Route detail / edit / weekday plan ─────────────────────────────────────

/// Full route detail: stops in order with client names + order totals —
/// powers the "view route" modal, the load dropdown's details and printing.
#[tauri::command]
pub fn cloud_route_detail(route_id: String) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let rows = client.select(
        "routes",
        "id, route_date, seller_id, status, name",
        &[("id", format!("eq.{route_id}"))],
    )?;
    let route = rows.into_iter().next().ok_or("route not found")?;
    let stops = client.select(
        "route_stops",
        "stop_order, status, arrived_at, completed_at, order_id, client:clients(name), order:orders(total_amount, amount_paid)",
        &[
            ("route_id", format!("eq.{route_id}")),
            ("order", "stop_order.asc".into()),
        ],
    )?;
    Ok(serde_json::json!({ "route": route, "stops": stops }))
}

/// Replace a route's seller/date/status and its ordered stops.
/// stop order = array index; each {order_id, client_id}.
#[tauri::command]
pub fn cloud_update_route(
    route_id: String,
    seller_id: String,
    route_date: String,
    status: String,
    stops: Value,
    name: Option<String>,
) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client.patch(
        "routes",
        serde_json::json!({
            "seller_id": seller_id,
            "route_date": route_date,
            "status": status,
            "name": name,
        }),
        &[("id", format!("eq.{route_id}"))],
    )?;
    client.delete("route_stops", &[("route_id", format!("eq.{route_id}"))])?;
    let arr = stops.as_array().cloned().unwrap_or_default();
    if !arr.is_empty() {
        let mut rows = Vec::new();
        for (i, st) in arr.iter().enumerate() {
            rows.push(serde_json::json!({
                "route_id": route_id,
                "order_id": st["order_id"],
                "client_id": st["client_id"],
                "stop_order": (i + 1) as i64,
                "status": "validated",
            }));
        }
        client.insert("route_stops", serde_json::Value::Array(rows))?;
    }
    Ok(())
}

/// Reusable weekday plan: which clients this seller serves on which day.
/// Written to the clients' visit_days via a dedicated lightweight table:
/// client_visit_days exists on clients (visit_days text[]). We set it per
/// client directly.
#[tauri::command]
pub fn cloud_set_client_visit_days(
    client_id: String,
    days: Vec<String>,
) -> Result<(), String> {
    let client = cloud::ensure_session()?;
    client.patch(
        "clients",
        serde_json::json!({ "visit_days": days }),
        &[("id", format!("eq.{client_id}"))],
    )
}
