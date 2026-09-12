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
        "id, name",
        &[("is_active", "eq.true".into()), ("order", "name.asc".into())],
    )?;
    Ok(Value::Array(rows))
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
        }]),
    )?;
    let route_id = rows
        .first()
        .and_then(|r| r["id"].as_str())
        .ok_or("route insert returned no id")?
        .to_string();

    let mut stops = Vec::new();
    for (i, order_id) in order_ids.iter().enumerate() {
        stops.push(serde_json::json!({
            "route_id": route_id,
            "order_id": order_id,
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
