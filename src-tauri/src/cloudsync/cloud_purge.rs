//! Owner-requested cloud purge: deleted test data must not resurrect from
//! the cloud on sync. Semantics respect the RESTRICT-FKs of the CRM schema
//! (sales_items/orders reference products/clients with ON DELETE RESTRICT):
//! rows with no transaction history are DELETED; referenced rows are
//! DEACTIVATED (is_active=false) so they stop syncing and showing.
//! Employees (profiles) are deactivated, never deleted — auth identity and
//! payroll history stay intact.

use crate::database::DbState;
use serde_json::{json, Value};
use tauri::State;

use super::{self as cloud, auth};

/// Purge cloud-side test data. Verifies the local admin password first.
/// entities: any of "products" | "clients" | "employees" | "trucks".
#[tauri::command]
pub fn cloud_purge_entities(
    db: State<'_, DbState>,
    entities: Vec<String>,
    admin_password: String,
) -> Result<Value, String> {
    if !crate::auth::verify_admin_password(&db, &admin_password).unwrap_or(false) {
        return Err("Mot de passe administrateur incorrect / Incorrect admin password".into());
    }
    let client = cloud::ensure_session()?;
    let profile = auth::fetch_profile(&client)?;
    let org = profile["organization_id"].as_str().ok_or("no org")?.to_string();
    let me = profile["id"].as_str().unwrap_or("").to_string();
    let org_q = format!("eq.{org}");

    let mut summary = serde_json::Map::new();

    // ── products: hard delete when possible, deactivate when referenced ──
    if entities.iter().any(|e| e == "products") {
        let mut deleted = 0i64;
        let mut deactivated = 0i64;
        let prods = client.select("products", "id", &[("organization_id", org_q.clone())])?;
        for p in &prods {
            let pid = match p["id"].as_str() { Some(v) => v, None => continue };
            match client.delete("products", &[("id", format!("eq.{pid}"))]) {
                Ok(_) => deleted += 1,
                Err(_) => {
                    // RESTRICT-FK: sold at least once — deactivate instead.
                    let _ = client.patch(
                        "products",
                        json!({ "is_active": false }),
                        &[("id", format!("eq.{pid}"))],
                    );
                    deactivated += 1;
                }
            }
        }
        // Local mirror: hard purge so nothing resurrects from the local DB.
        {
            let conn = db.conn.lock().unwrap();
            let _ = conn.execute("DELETE FROM product_packagings", []);
            let _ = conn.execute("DELETE FROM products", []);
            // OWNER: purged rows came back on the next refresh — the sync
            // OUTBOX held pending product events that the drain re-uploaded
            // (upsert_pos_product recreates them by SKU). Purge the queue and
            // the id maps for the wiped entities.
            let _ = conn.execute(
                "DELETE FROM sync_outbox WHERE entity IN ('product','stock_adjustment')",
                [],
            );
            let _ = conn.execute(
                "DELETE FROM sync_map WHERE entity IN ('product','stock_adjustment')",
                [],
            );
        }
        summary.insert("products".into(), json!({"deleted": deleted, "deactivated": deactivated}));
    }

    // ── clients: same policy as products ──
    if entities.iter().any(|e| e == "clients") {
        let mut deleted = 0i64;
        let mut deactivated = 0i64;
        let rows = client.select("clients", "id", &[("organization_id", org_q.clone())])?;
        for c in &rows {
            let cid = match c["id"].as_str() { Some(v) => v, None => continue };
            match client.delete("clients", &[("id", format!("eq.{cid}"))]) {
                Ok(_) => deleted += 1,
                Err(_) => {
                    let _ = client.patch(
                        "clients",
                        json!({ "is_active": false }),
                        &[("id", format!("eq.{cid}"))],
                    );
                    deactivated += 1;
                }
            }
        }
        // Local mirror: soft-delete (local sale history keeps referencing).
        {
            let conn = db.conn.lock().unwrap();
            let _ = conn.execute("UPDATE clients SET is_active = 0", []);
            // Same resurrection guard as products: drop pending client pushes
            // + maps so a later drain cannot recreate the purged clients.
            let _ = conn.execute(
                "DELETE FROM sync_outbox WHERE entity = 'customer'",
                [],
            );
            let _ = conn.execute(
                "DELETE FROM sync_map WHERE entity = 'customer'",
                [],
            );
        }
        summary.insert("clients".into(), json!({"deleted": deleted, "deactivated": deactivated}));
    }

    // ── employees (profiles): deactivate everyone except the operator ──
    if entities.iter().any(|e| e == "employees") {
        let rows = client.select("profiles", "id, is_active", &[("organization_id", org_q.clone())])?;
        let mut deactivated = 0i64;
        for pr in &rows {
            let pid = match pr["id"].as_str() { Some(v) => v, None => continue };
            if pid == me { continue; }
            client.patch(
                "profiles",
                json!({ "is_active": false }),
                &[("id", format!("eq.{pid}"))],
            ).map_err(|e| e.to_string())?;
            deactivated += 1;
        }
        summary.insert("employees".into(), json!({"deactivated": deactivated}));
    }

    // ── trucks: full purge each (positions/loads/items/settlements/expenses) ──
    if entities.iter().any(|e| e == "trucks") {
        let rows = client.select("trucks", "id", &[("organization_id", org_q.clone())])?;
        let mut purged = 0i64;
        for t in &rows {
            let tid = match t["id"].as_str() { Some(v) => v, None => continue };
            client
                .rpc("purge_truck", json!({ "p_truck_id": tid }))
                .map_err(|e| e.to_string())?;
            purged += 1;
        }
        summary.insert("trucks".into(), json!({"purged": purged}));
    }

    Ok(Value::Object(summary))
}

/// Whitelisted thumbnails of the org's cloud tables for the Settings →
/// Cloud Data browser ("a mirror of the cloud DB"). Everything org-scoped.
#[tauri::command]
pub fn cloud_browse_table(
    table: String,
    limit: Option<i64>,
) -> Result<Value, String> {
    let client = cloud::ensure_session()?;
    let profile = auth::fetch_profile(&client)?;
    let org = profile["organization_id"].as_str().ok_or("no org")?.to_string();

    let (cols, table) = match table.as_str() {
        "suppliers"  => ("id, name, contact_name, phone, email", "suppliers"),
        "clients"    => ("id, name, phone, type, current_balance, is_active", "clients"),
        "products"   => ("id, name, sku, unit_price, cost_price, is_active", "products"),
        "employees"  => ("id, full_name, role, is_active", "profiles"),
        "trucks"     => ("id, name, plate, driver_name, is_active", "trucks"),
        "transactions" => ("id, status, total_amount, amount_paid, created_at", "orders"),
        _ => return Err(format!("table '{}' is not browsable", table)),
    };

    let rows = client.select(
        table,
        cols,
        &[
            ("organization_id", format!("eq.{org}")),
            ("limit", limit.unwrap_or(200).to_string()),
            ("order", "id.desc".into()),
        ],
    )?;
    Ok(Value::Array(rows))
}

/// Delete ONE row of a whitelisted table from the cloud, org-scoped, after
/// verifying the local admin password. RESTRICT-FK rows (sold products,
/// billed clients) return the FK message — the caller surfaces it; use the
/// purge card to deactivate instead.
#[tauri::command]
pub fn cloud_delete_row(
    table: String,
    id: String,
    admin_password: String,
    db: State<'_, DbState>,
) -> Result<(), String> {
    if !crate::auth::verify_admin_password(&db, &admin_password).unwrap_or(false) {
        return Err("Mot de passe administrateur incorrect / Incorrect admin password".into());
    }
    let client = cloud::ensure_session()?;
    let profile = auth::fetch_profile(&client)?;
    let org = profile["organization_id"].as_str().ok_or("no org")?.to_string();

    let table = match table.as_str() {
        "suppliers" | "clients" | "products" | "trucks" | "transactions" => match table.as_str() {
            "transactions" => "orders",
            other => other,
        },
        _ => return Err(format!("table '{}' is not deletable here", table)),
    };

    client.delete(table, &[("id", format!("eq.{id}")), ("organization_id", format!("eq.{org}"))])?;

    // Resurrection guard: a pending product/customer push for this row would
    // recreate it on the next drain — drop its queued event + id map.
    {
        let entity = match table {
            "products" => Some("product"),
            "clients" => Some("customer"),
            _ => None,
        };
        if let Some(entity) = entity {
            let conn = db.conn.lock().unwrap();
            if let Some(local_id) = super::outbox::local_id(&conn, entity, &id) {
                let _ = conn.execute(
                    "DELETE FROM sync_outbox WHERE entity = ?1 AND local_id = ?2",
                    rusqlite::params![entity, local_id],
                );
                let _ = conn.execute(
                    "DELETE FROM sync_map WHERE entity = ?1 AND local_id = ?2",
                    rusqlite::params![entity, local_id],
                );
            }
        }
    }
    Ok(())
}

/// Deactivate ONE row (products/clients) — the fallback when a RESTRICT FK
/// (sold/billed rows) refuses the hard delete. Owner request: the Cloud
/// Data tab must always be able to remove an item from circulation.
#[tauri::command]
pub fn cloud_deactivate_row(
    table: String,
    id: String,
    admin_password: String,
    db: State<'_, DbState>,
) -> Result<(), String> {
    if !crate::auth::verify_admin_password(&db, &admin_password).unwrap_or(false) {
        return Err("Mot de passe administrateur incorrect / Incorrect admin password".into());
    }
    let table = match table.as_str() {
        "products" | "clients" => table,
        _ => return Err(format!("table '{}' cannot be deactivated here", table)),
    };
    let client = cloud::ensure_session()?;
    let profile = auth::fetch_profile(&client)?;
    let org = profile["organization_id"].as_str().ok_or("no org")?.to_string();
    client.patch(
        table.as_str(),
        serde_json::json!({ "is_active": false }),
        &[
            ("id", format!("eq.{id}")),
            ("organization_id", format!("eq.{org}")),
        ],
    )
}
