//! Outbox drain: converts each pending event into an idempotent CRM RPC call.
//! Exactly-once is guaranteed by the CRM side (pos_ref unique indexes) — a
//! replayed push returns the existing row instead of duplicating.

use super::http::SupabaseClient;
use super::mapping::{crm_payment_method, crm_product_name, dzd_to_centimes, qty_round};
use super::outbox::{self, OutboxEntity, PendingEvent};
use rusqlite::Connection;
use serde_json::{json, Value};

/// Drain up to `batch` pending events. Returns (pushed, failed, retried).
/// A transient error aborts the remaining batch (order matters for the
/// ledger; the next cycle resumes).
pub fn drain(
    conn: &mut Connection,
    client: &SupabaseClient,
    batch: i64,
) -> Result<(usize, usize, usize), String> {
    let events = outbox::pending(conn, batch).map_err(|e| e.to_string())?;
    let mut pushed = 0;
    let mut failed = 0;
    let mut retried = 0;

    for ev in events {
        match handle(conn, client, &ev) {
            Ok(()) => {
                outbox::mark_done(conn, ev.outbox_id).map_err(|e| e.to_string())?;
                pushed += 1;
            }
            Err(e) => {
                if is_permanent(&e) {
                    outbox::mark_failed(conn, ev.outbox_id, &e).map_err(|e| e.to_string())?;
                    failed += 1;
                } else {
                    outbox::record_retry(conn, ev.outbox_id, &e).map_err(|e| e.to_string())?;
                    retried += 1;
                    // Stop the batch on the first transient error — keep
                    // ledger order, retry from here next cycle.
                    return Ok((pushed, failed, retried));
                }
            }
        }
    }
    Ok((pushed, failed, retried))
}

/// 4xx-class errors the CRM will never accept (bad payload, unknown product,
/// permission). Network / 5xx are transient.
fn is_permanent(err: &str) -> bool {
    err.contains("[40") // PGRST/GoTrue error codes carried as [4xxxx]
        || err.contains("HTTP 40")
        || err.contains("required")
        || err.contains("not found")
        || err.contains("unknown product")
}

fn handle(conn: &mut Connection, client: &SupabaseClient, ev: &PendingEvent) -> Result<(), String> {
    let entity = OutboxEntity::parse(&ev.entity)
        .ok_or_else(|| format!("unknown outbox entity: {}", ev.entity))?;
    let payload: Value =
        serde_json::from_str(&ev.payload).map_err(|e| format!("bad payload json: {e}"))?;

    match entity {
        OutboxEntity::Sale => push_sale(conn, client, ev.local_id, payload),
        OutboxEntity::Refund => push_refund(conn, client, payload),
        OutboxEntity::Purchase => push_purchase(conn, client, ev.local_id, payload),
        OutboxEntity::Product => push_product(conn, client, ev.local_id, payload),
        OutboxEntity::StockAdjustment => push_stock_adjustment(conn, client, payload),
        OutboxEntity::Customer => push_customer(conn, client, ev.local_id, payload),
        OutboxEntity::CustomerPayment => push_customer_payment(conn, client, payload),
        OutboxEntity::Supplier => push_supplier(conn, client, ev.local_id, payload),
    }
}

/// A POS counter sale. Payload (built by sales_service at enqueue):
/// { sale_number, items: [{product_id, quantity, unit_price, line_total}],
///   subtotal, tax_amount, total, paid, change, skip_stock,
///   payments: [{method, amount}], customer_local_id, created_at }
fn push_sale(
    conn: &mut Connection,
    client: &SupabaseClient,
    _local_id: i64,
    p: Value,
) -> Result<(), String> {
    // Walk-in (local customer 1 / null) maps to the org's POS client; other
    // customers resolve via sync_map (unknown → ensure_pos_client as safe
    // fallback so the sale never blocks sync).
    let crm_client_id = resolve_client(conn, client, p["customer_local_id"].as_i64())?;

    let items = p["items"]
        .as_array()
        .ok_or("sale push: items required")?;
    if items.is_empty() {
        return Err("sale push: at least one item required".to_string());
    }
    // Resolve each local product id → CRM uuid. Unlinked products are a
    // TRANSIENT state (the puller's linking pass will map them) — retry later.
    let mut items_json: Vec<Value> = vec![];
    for it in items {
        let local_id = it["product_local_id"].as_i64().unwrap_or(0);
        let product_id = outbox::remote_id(conn, "product", local_id)
            .ok_or_else(|| format!("product {local_id} not linked to CRM yet"))?;
        items_json.push(json!({
            "product_id": product_id,
            "quantity": qty_round(it["quantity"].as_f64().unwrap_or(0.0)),
            "unit_price": dzd_to_centimes(it["unit_price"].as_i64().unwrap_or(0)),
            "line_total": dzd_to_centimes(it["line_total"].as_i64().unwrap_or(0)),
            "base_quantity": qty_round(it["base_quantity"].as_f64().unwrap_or(0.0)),
            "sale_unit": it["sale_unit"],
        }));
    }

    // Payments: map methods + convert money; drop unmappable (credit).
    let mut payments_json: Vec<Value> = vec![];
    for pay in p["payments"].as_array().unwrap_or(&vec![]) {
        if let Some(method) = pay["method"].as_str().and_then(crm_payment_method) {
            let amount = dzd_to_centimes(pay["amount"].as_i64().unwrap_or(0));
            if amount > 0 {
                payments_json.push(json!({ "method": method, "amount": amount }));
            }
        }
    }

    let total = dzd_to_centimes(p["total"].as_i64().unwrap_or(0));
    // Credit remainder: if the POS recorded less paid than total (credit
    // sale), the RPC computes the balance delta — but record it as a payment
    // with amount 0? No: pass the real paid sum; the RPC adds (total-paid)
    // to the client balance only when > 0. For non-walk-in credit customers
    // the CRM balance mirrors the POS debt.

    client
        .rpc(
            "create_pos_order",
            json!({
                "p_client_id": crm_client_id,
                "p_items": items_json,
                "p_subtotal": dzd_to_centimes(p["subtotal"].as_i64().unwrap_or(0)),
                "p_tax": dzd_to_centimes(p["tax_amount"].as_i64().unwrap_or(0)),
                "p_total": total,
                "p_payments": payments_json,
                "p_notes": p["notes"],
                "p_pos_ref": p["sale_number"].as_str().unwrap_or(""),
                "p_skip_stock": p["skip_stock"].as_bool().unwrap_or(false),
                "p_created_at": p["created_at"],
            }),
        )
        .map(|_| ())
}

/// Refund → return movements (stock truth only).
fn push_refund(conn: &mut Connection, client: &SupabaseClient, p: Value) -> Result<(), String> {
    let items = p["items"]
        .as_array()
        .ok_or("refund push: items required")?;
    let mut items_json: Vec<Value> = vec![];
    for it in items {
        let local_id = it["product_local_id"].as_i64().unwrap_or(0);
        let product_id = outbox::remote_id(conn, "product", local_id)
            .ok_or_else(|| format!("product {local_id} not linked to CRM yet"))?;
        items_json.push(json!({
            "product_id": product_id,
            "quantity": qty_round(it["quantity"].as_f64().unwrap_or(0.0)),
        }));
    }
    client
        .rpc(
            "record_pos_refund",
            json!({
                "p_pos_ref": p["sale_number"].as_str().unwrap_or(""),
                "p_items": items_json,
            }),
        )
        .map(|_| ())
}

/// POS purchase → create_purchase (idempotent via p_number).
fn push_purchase(
    conn: &mut Connection,
    client: &SupabaseClient,
    _local_id: i64,
    p: Value,
) -> Result<(), String> {
    // Resolve the supplier to a CRM uuid (sync_map; walk-in supplier id 1 →
    // create on the fly is NOT wanted, so require a mapped supplier).
    let supplier_local = p["supplier_local_id"].as_i64();
    let supplier_remote = supplier_local
        .and_then(|id| outbox::remote_id(conn, "supplier", id))
        .ok_or("purchase push: supplier not linked to CRM yet")?;

    let items = p["items"].as_array().ok_or("purchase push: items required")?;
    let mut items_json: Vec<Value> = vec![];
    for it in items {
        let local_id = it["product_local_id"].as_i64().unwrap_or(0);
        let product_id = outbox::remote_id(conn, "product", local_id)
            .ok_or_else(|| format!("product {local_id} not linked to CRM yet"))?;
        items_json.push(json!({
            "product_id": product_id,
            "quantity": qty_round(it["base_quantity"].as_f64().unwrap_or(
                it["quantity"].as_f64().unwrap_or(0.0))),
            "unit_cost": dzd_to_centimes(it["unit_cost"].as_i64().unwrap_or(0)),
        }));
    }

    client
        .rpc(
            "create_purchase",
            json!({
                "p_supplier_id": supplier_remote,
                "p_items": items_json,
                "p_number": p["invoice_number"].as_str().unwrap_or(""),
            }),
        )
        .map(|_| ())
}

/// Product create/update → upsert_pos_product (SKU → barcode → insert).
fn push_product(
    conn: &mut Connection,
    client: &SupabaseClient,
    local_id: i64,
    p: Value,
) -> Result<(), String> {
    let remote = client.rpc(
        "upsert_pos_product",
        json!({
            "p_sku": p["sku"],
            "p_barcode": p["barcode"],
            "p_name": crm_product_name(
                p["name_fr"].as_str().unwrap_or(""),
                p["name_ar"].as_str().unwrap_or(""),
                p["name_en"].as_str().unwrap_or(""),
            ),
            "p_category": p["category"],
            "p_unit_price": dzd_to_centimes(p["sale_price"].as_i64().unwrap_or(0)),
            "p_cost_price": dzd_to_centimes(p["purchase_price"].as_i64().unwrap_or(0)),
            "p_min_stock_alert": qty_round(p["min_stock"].as_f64().unwrap_or(0.0)),
            "p_is_active": p["is_active"].as_bool().unwrap_or(true),
            "p_packagings": p["packagings"],
        }),
    )?;
    let id = remote.as_str().unwrap_or_default().to_string();
    if id.is_empty() {
        return Err("upsert_pos_product returned no id".into());
    }
    outbox::map_local(conn, "product", local_id, &id).map_err(|e| e.to_string())?;
    Ok(())
}

/// Manual stock adjustment (admin counted stock) → adjustment movement.
fn push_stock_adjustment(conn: &mut Connection, client: &SupabaseClient, p: Value) -> Result<(), String> {
    let product_remote = outbox::remote_id(conn, "product", p["product_local_id"].as_i64().unwrap_or(0))
        .ok_or("adjustment push: product not linked to CRM yet")?;
    // Insert a movement directly through PostgREST (admin RLS allows insert;
    // the append-only trigger blocks updates/deletes, which we never do).
    client.insert(
        "stock_movements",
        json!([{
            "organization_id": p["org_id"],
            "product_id": product_remote,
            "type": "adjustment",
            "quantity": qty_round(p["quantity"].as_f64().unwrap_or(0.0)),
            "reason": p["reason"].as_str().unwrap_or("POS stock correction"),
        }]),
    )?;
    Ok(())
}

/// Customer create/update → clients upsert.
fn push_customer(
    conn: &mut Connection,
    client: &SupabaseClient,
    local_id: i64,
    p: Value,
) -> Result<(), String> {
    // Walk-in (id 1) is handled by ensure_pos_client, never here.
    if local_id == 1 {
        return Ok(());
    }
    match outbox::remote_id(conn, "customer", local_id) {
        Some(remote) => client
            .patch(
                "clients",
                json!({
                    "name": p["name"],
                    "phone": p["phone"],
                    "owner_name": p["owner_name"],
                    "address": p["address"],
                }),
                &[("id", format!("eq.{remote}"))],
            ),
        None => {
            // New customer → insert + remember the mapping.
            let rows = client.insert(
                "clients",
                json!([{
                    "organization_id": p["org_id"],
                    "name": p["name"],
                    "phone": p["phone"],
                    "owner_name": p["owner_name"],
                    "address": p["address"],
                    "created_by": p["user_id"],
                }]),
            )?;
            let id = rows
                .first()
                .and_then(|r| r["id"].as_str())
                .ok_or("customer insert returned no id")?
                .to_string();
            outbox::map_local(conn, "customer", local_id, &id).map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}

/// Customer debt payment → record_client_payment (idempotent).
fn push_customer_payment(conn: &mut Connection, client: &SupabaseClient, p: Value) -> Result<(), String> {
    let client_crm = resolve_client(conn, client, p["customer_local_id"].as_i64())?;
    client
        .rpc(
            "record_client_payment",
            json!({
                "p_client_id": client_crm,
                "p_amount": dzd_to_centimes(p["amount"].as_i64().unwrap_or(0)),
                "p_method": crm_payment_method(p["method"].as_str().unwrap_or("cash")).unwrap_or("cash"),
                "p_pos_ref": p["pos_ref"].as_str().unwrap_or(""),
                "p_notes": p["notes"],
            }),
        )
        .map(|_| ())
}

/// Supplier create/update → suppliers upsert (match by name when unmapped).
fn push_supplier(
    conn: &mut Connection,
    client: &SupabaseClient,
    local_id: i64,
    p: Value,
) -> Result<(), String> {
    // Walk-in supplier (id 1) is not pushed — it exists only in the POS.
    if local_id == 1 {
        return Ok(());
    }
    match outbox::remote_id(conn, "supplier", local_id) {
        Some(remote) => client.patch(
            "suppliers",
            json!({
                "name": p["name"],
                "contact_name": p["contact_person"],
                "phone": p["phone"],
                "email": p["email"],
                "address": p["address"],
            }),
            &[("id", format!("eq.{remote}"))],
        ),
        None => {
            let rows = client.insert(
                "suppliers",
                json!([{
                    "organization_id": p["org_id"],
                    "name": p["name"],
                    "contact_name": p["contact_person"],
                    "phone": p["phone"],
                    "email": p["email"],
                    "address": p["address"],
                }]),
            )?;
            let id = rows
                .first()
                .and_then(|r| r["id"].as_str())
                .ok_or("supplier insert returned no id")?
                .to_string();
            outbox::map_local(conn, "supplier", local_id, &id).map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}

/// Resolve a POS customer to a CRM client uuid: sync_map first; walk-in (1 or
/// None) → ensure_pos_client; unknown → ensure_pos_client too (never lose a
/// sale to sync — the admin can re-link later).
fn resolve_client(
    conn: &mut Connection,
    client: &SupabaseClient,
    customer_local_id: Option<i64>,
) -> Result<String, String> {
    match customer_local_id {
        Some(id) if id != 1 => {
            if let Some(remote) = outbox::remote_id(conn, "customer", id) {
                return Ok(remote);
            }
            // Fall through: unmapped named customer → POS client (safe).
            client
                .rpc("ensure_pos_client", json!({}))
                .and_then(|v| v.as_str().map(String::from).ok_or("no id".into()))
        }
        _ => client
            .rpc("ensure_pos_client", json!({}))
            .and_then(|v| v.as_str().map(String::from).ok_or("no id".into())),
    }
}
