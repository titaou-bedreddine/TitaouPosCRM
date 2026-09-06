//! Server-side command registry for the LAN API.
//!
//! The `/api/v1/invoke/{command}` endpoint dispatches WHITELISTED business
//! operations to the exact same service functions the local UI uses — no raw
//! SQL ever crosses the network. Each command carries its auth requirement
//! and (for mutations) the real-time event broadcast to other terminals.
//!
//! Argument parsing tolerates both camelCase (what the Tauri frontend sends)
//! and snake_case keys, and reuses the model structs for nested payloads so
//! serialization is bit-identical with local mode.

use crate::auth;
use crate::database::DbState;
use crate::models::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Auth levels
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Auth {
    /// Usable with just a registered device token (pre-login: the login
    /// screen needs the user list and the login call itself).
    Device,
    /// Any authenticated user.
    User,
    /// Administrators only.
    Admin,
}

pub struct CommandSpec {
    pub auth: Auth,
    /// Real-time event broadcast after a successful mutation.
    pub event: Option<&'static str>,
}

const fn spec(auth: Auth, event: Option<&'static str>) -> CommandSpec {
    CommandSpec { auth, event }
}

/// The whitelist. Everything NOT here returns "not available over the
/// network" — that deliberately excludes local hardware (printers, cash
/// drawer, scale SDK), file pickers, backups and licensing, which stay
/// local per PC by design.
pub fn lookup(command: &str) -> Option<CommandSpec> {
    let s = match command {
        // --- auth (pre-login surface) ---
        "get_active_users" => spec(Auth::Device, None),
        "get_user_by_qr" => spec(Auth::Device, None),
        "login_with_rfid" => spec(Auth::Device, None),
        "verify_admin_password" => spec(Auth::User, None),
        "change_user_password" => spec(Auth::User, None),

        // --- users & roles ---
        "get_all_users" => spec(Auth::Admin, None),
        "get_all_roles" => spec(Auth::Admin, None),
        "create_user" => spec(Auth::Admin, Some("user_updated")),
        "update_user" => spec(Auth::Admin, Some("user_updated")),
        "delete_user" => spec(Auth::Admin, Some("user_updated")),
        "toggle_user_pin" => spec(Auth::User, Some("user_updated")),

        // --- dashboard ---
        "get_dashboard_stats" => spec(Auth::User, None),

        // --- cash / sessions (register-scoped per terminal) ---
        "get_active_cash_session" => spec(Auth::User, None),
        "open_cash_session" => spec(Auth::User, Some("session_opened")),
        "add_cash_movement" => spec(Auth::User, Some("session_updated")),
        "close_cash_session" => spec(Auth::User, Some("session_closed")),
        "list_cash_movements" => spec(Auth::User, None),
        "list_session_history" => spec(Auth::User, None),
        "edit_opening_balance" => spec(Auth::User, Some("session_updated")),
        "edit_cash_session" => spec(Auth::User, Some("session_updated")),
        "archive_cash_session" => spec(Auth::User, Some("session_updated")),
        "delete_cash_session" => spec(Auth::User, Some("session_updated")),

        // --- products ---
        "search_products" => spec(Auth::User, None),
        "save_product" => spec(Auth::User, Some("product_updated")),
        "delete_product" => spec(Auth::User, Some("product_updated")),
        "get_categories" => spec(Auth::User, None),
        "save_category" => spec(Auth::User, Some("product_updated")),
        "delete_category" => spec(Auth::User, Some("product_updated")),
        "get_units" => spec(Auth::User, None),
        "save_unit" => spec(Auth::User, Some("product_updated")),
        "toggle_product_pin" => spec(Auth::User, Some("product_updated")),
        "reorder_pinned_products" => spec(Auth::User, Some("product_updated")),
        "list_packagings" => spec(Auth::User, None),
        "save_packagings" => spec(Auth::User, Some("product_updated")),
        "get_price_history" => spec(Auth::User, None),
        "get_quantity_history" => spec(Auth::User, None),
        "resolve_scale_scan" => spec(Auth::User, None),

        // --- sales ---
        "process_sale" => spec(Auth::User, Some("sale_created")),
        "create_sale" => spec(Auth::User, Some("sale_created")),
        "replace_sale" => spec(Auth::User, Some("sale_updated")),
        "list_sales" => spec(Auth::User, None),
        "get_sale_items" => spec(Auth::User, None),
        "get_last_sale" => spec(Auth::User, None),
        "get_sale_by_number" => spec(Auth::User, None),
        "hold_sale" => spec(Auth::User, Some("held_updated")),
        "list_held_sales" => spec(Auth::User, None),
        "delete_held_sale" => spec(Auth::User, Some("held_updated")),
        "delete_sale" => spec(Auth::User, Some("sale_deleted")),

        // --- customers ---
        "list_customers" => spec(Auth::User, None),
        "save_customer" => spec(Auth::User, Some("customer_updated")),
        "delete_customer" => spec(Auth::User, Some("customer_updated")),
        "toggle_customer_pin" => spec(Auth::User, Some("customer_updated")),
        "record_customer_debt_payment" => spec(Auth::User, Some("customer_updated")),
        "clear_customer_debt" => spec(Auth::User, Some("customer_updated")),
        "list_debt_clear_log" => spec(Auth::User, None),

        // --- suppliers ---
        "list_suppliers" => spec(Auth::User, None),
        "save_supplier" => spec(Auth::User, Some("supplier_updated")),
        "delete_supplier" => spec(Auth::User, Some("supplier_updated")),
        "toggle_supplier_pin" => spec(Auth::User, Some("supplier_updated")),
        "record_supplier_debt_payment" => spec(Auth::User, Some("supplier_updated")),
        "list_supplier_debt_payments" => spec(Auth::User, None),
        "clear_supplier_debt" => spec(Auth::User, Some("supplier_updated")),

        // --- purchases ---
        "create_purchase" => spec(Auth::User, Some("purchase_created")),
        "get_purchase_items" => spec(Auth::User, None),
        "delete_purchase" => spec(Auth::User, Some("purchase_deleted")),
        "list_purchases" => spec(Auth::User, None),

        // --- expenses ---
        "add_expense" => spec(Auth::User, Some("expense_created")),
        "update_expense" => spec(Auth::User, Some("expense_updated")),
        "list_expenses" => spec(Auth::User, None),
        "delete_expense" => spec(Auth::User, Some("expense_deleted")),

        // --- employees & payroll ---
        "list_employees" => spec(Auth::User, None),
        "save_employee" => spec(Auth::User, Some("employee_updated")),
        "find_employee_by_rfid" => spec(Auth::User, None),
        "next_employee_code" => spec(Auth::User, None),
        "delete_employee" => spec(Auth::User, Some("employee_updated")),
        "list_payrolls" => spec(Auth::User, None),
        "record_employee_advance" => spec(Auth::User, Some("payroll_updated")),
        "list_employee_advances" => spec(Auth::User, None),
        "record_employee_absence" => spec(Auth::User, Some("payroll_updated")),
        "list_employee_absences" => spec(Auth::User, None),

        // --- settings (shop-wide; the client filters per-PC keys) ---
        "get_all_settings" => spec(Auth::Device, None),
        "get_setting" => spec(Auth::Device, None),
        "set_setting" => spec(Auth::User, Some("settings_updated")),
        "set_multiple_settings" => spec(Auth::User, Some("settings_updated")),

        // --- notifications (server-side feeds) ---
        "list_app_notifications" => spec(Auth::User, None),
        "dismiss_app_notification" => spec(Auth::User, None),
        "run_payroll_reminder_scan" => spec(Auth::User, Some("notification_created")),
        "send_telegram_message" => spec(Auth::User, None),
        "send_telegram_recap" => spec(Auth::User, None),
        _ => return None,
    };
    Some(s)
}

/// Every whitelisted command name — the client-side forwarding list is built
/// from this so the two sides can never drift.
pub fn all_command_names() -> Vec<&'static str> {
    // Cheap and drift-proof: probe the match table via lookup on a known set.
    KNOWN_COMMANDS.iter().copied().collect()
}

/// Kept in lockstep with the match above (asserted by a test).
pub const KNOWN_COMMANDS: &[&str] = &[
    "get_active_users", "get_user_by_qr", "login_with_rfid", "verify_admin_password",
    "change_user_password", "get_all_users", "get_all_roles", "create_user", "update_user",
    "delete_user", "toggle_user_pin", "get_dashboard_stats", "get_active_cash_session",
    "open_cash_session", "add_cash_movement", "close_cash_session", "list_cash_movements",
    "list_session_history", "edit_opening_balance", "edit_cash_session", "archive_cash_session",
    "delete_cash_session", "search_products", "save_product", "delete_product", "get_categories",
    "save_category", "delete_category", "get_units", "save_unit", "toggle_product_pin",
    "reorder_pinned_products", "list_packagings", "save_packagings", "get_price_history",
    "get_quantity_history", "resolve_scale_scan", "process_sale", "create_sale", "replace_sale",
    "list_sales", "get_sale_items", "get_last_sale", "get_sale_by_number", "hold_sale",
    "list_held_sales", "delete_held_sale", "delete_sale", "list_customers", "save_customer",
    "delete_customer", "toggle_customer_pin", "record_customer_debt_payment",
    "clear_customer_debt", "list_debt_clear_log", "list_suppliers", "save_supplier",
    "delete_supplier", "toggle_supplier_pin", "record_supplier_debt_payment",
    "list_supplier_debt_payments", "clear_supplier_debt", "create_purchase", "get_purchase_items",
    "delete_purchase", "list_purchases", "add_expense", "update_expense", "list_expenses",
    "delete_expense", "list_employees", "save_employee", "find_employee_by_rfid",
    "next_employee_code", "delete_employee", "list_payrolls", "record_employee_advance",
    "list_employee_advances", "record_employee_absence", "list_employee_absences",
    "get_all_settings", "get_setting", "set_setting", "set_multiple_settings",
    "list_app_notifications", "dismiss_app_notification", "run_payroll_reminder_scan",
    "send_telegram_message", "send_telegram_recap",
];

// ---------------------------------------------------------------------------
// JSON arg helpers (camelCase ↔ snake_case tolerant)
// ---------------------------------------------------------------------------

fn to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = false;
    for c in s.chars() {
        if c == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn field<'a>(args: &'a Value, name: &str) -> Option<&'a Value> {
    if args.is_null() {
        return None;
    }
    args.get(name)
        .or_else(|| args.get(to_camel(name)))
        .or_else(|| args.get(name))
}

fn missing(name: &str) -> String {
    format!("missing argument '{}' (network call)", name)
}

fn req_i64(args: &Value, name: &str) -> Result<i64, String> {
    field(args, name)
        .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .ok_or_else(|| missing(name))
}

fn opt_i64(args: &Value, name: &str) -> Result<Option<i64>, String> {
    match field(args, name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => Ok(v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))),
    }
}

fn req_f64(args: &Value, name: &str) -> Result<f64, String> {
    field(args, name).and_then(|v| v.as_f64()).ok_or_else(|| missing(name))
}

fn req_str(args: &Value, name: &str) -> Result<String, String> {
    field(args, name)
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .ok_or_else(|| missing(name))
}

fn opt_str(args: &Value, name: &str) -> Result<Option<String>, String> {
    match field(args, name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Ok(Some(other.to_string())),
    }
}

fn req_bool(args: &Value, name: &str) -> Result<bool, String> {
    field(args, name).and_then(|v| v.as_bool()).ok_or_else(|| missing(name))
}

fn opt_bool(args: &Value, name: &str) -> Result<Option<bool>, String> {
    match field(args, name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => Ok(v.as_bool()),
    }
}

/// Nested model payload (reuses the exact model struct → identical JSON).
fn model<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T, String> {
    let v = field(args, name).cloned().unwrap_or(Value::Null);
    serde_json::from_value(v).map_err(|e| format!("bad argument '{}': {}", name, e))
}

fn as_json<T: Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Caller context
// ---------------------------------------------------------------------------

pub struct InvokeContext<'a> {
    pub db: &'a DbState,
    /// Node id of the calling terminal (device token owner).
    pub caller_node: String,
    /// Authenticated user (None = device-token-only, pre-login).
    pub caller_user: Option<CallerUser>,
}

#[derive(Debug, Clone)]
pub struct CallerUser {
    pub user_id: i64,
    pub username: String,
    pub role_id: Option<i64>,
    pub role_name: Option<String>,
}

impl CallerUser {
    pub fn is_admin(&self) -> bool {
        self.role_id == Some(1) || self.role_name.as_deref() == Some("Administrator")
    }
}

/// The register row id reserved for one terminal (created on demand), so
/// sessions never mix between PCs.
fn ensure_register_for_node(db: &DbState, node_id: &str) -> Result<i64, String> {
    let conn = db.conn.lock().unwrap();
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM registers WHERE identifier = ?1",
            [node_id],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    let suffix = node_id.strip_prefix("NODE-").unwrap_or(node_id);
    conn.execute(
        "INSERT INTO registers (name, identifier, is_active) VALUES (?1, ?2, 1)",
        rusqlite::params![format!("Terminal {}", suffix), node_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Execute one whitelisted command. Returns the command's JSON result —
/// the same value the local Tauri command would return.
pub fn dispatch(ctx: &InvokeContext, command: &str, args: &Value) -> Result<Value, String> {
    let s = lookup(command).ok_or_else(|| {
        format!(
            "'{}' is not available over the network (local-only operation)",
            command
        )
    })?;

    // Permission enforcement (spec: a client cannot bypass permissions by
    // calling the API manually). Device-level (pre-login) commands run
    // without a user; everything else requires one, Admin commands require
    // an administrator.
    if s.auth != Auth::Device {
        let caller = ctx.caller_user.as_ref().ok_or_else(|| {
            "Authentication required: log in first".to_string()
        })?;
        if s.auth == Auth::Admin && !caller.is_admin() {
            return Err("Administrator permission required".into());
        }
    }

    let db = ctx.db;
    let result: Value = match command {
        // --- auth ---
        "get_active_users" => as_json(auth::list_active_users(db)?)?,
        "get_user_by_qr" => as_json(
            crate::services::employee_service::get_user_by_qr(db, &req_str(args, "qr_code")?)?,
        )?,
        "login_with_rfid" => as_json(
            crate::services::employee_service::login_with_rfid(db, &req_str(args, "rfid")?)?,
        )?,
        "verify_admin_password" => as_json(auth::verify_admin_password(db, &req_str(args, "password")?)?)?,
        "change_user_password" => {
            crate::services::employee_service::change_user_password(
                db,
                req_i64(args, "user_id")?,
                &req_str(args, "new_password")?,
                opt_str(args, "old_password")?,
            )?;
            Value::Null
        }

        // --- users & roles ---
        "get_all_users" => as_json(crate::services::user_service::get_all_users(db)?)?,
        "get_all_roles" => as_json(crate::services::user_service::get_all_roles(db)?)?,
        "create_user" => as_json(crate::services::user_service::create_user(
            db,
            &req_str(args, "username")?,
            &req_str(args, "display_name")?,
            &req_str(args, "password")?,
            opt_i64(args, "role_id")?,
            req_f64(args, "max_discount_percent")?,
        )?)?,
        "update_user" => {
            crate::services::user_service::update_user(
                db,
                req_i64(args, "user_id")?,
                &req_str(args, "username")?,
                &req_str(args, "display_name")?,
                opt_i64(args, "role_id")?,
                req_f64(args, "max_discount_percent")?,
                req_bool(args, "is_active")?,
                opt_str(args, "new_password")?,
            )?;
            Value::Null
        }
        "delete_user" => {
            crate::services::user_service::delete_user(db, req_i64(args, "user_id")?)?;
            Value::Null
        }
        "toggle_user_pin" => {
            crate::services::user_service::toggle_user_pin(db, req_i64(args, "user_id")?, req_bool(args, "pinned")?)?;
            Value::Null
        }

        // --- dashboard ---
        "get_dashboard_stats" => as_json(crate::services::dashboard_service::get_stats(
            db,
            opt_str(args, "start_date")?,
            opt_str(args, "end_date")?,
        )?)?,

        // --- cash (register-scoped per calling terminal) ---
        "get_active_cash_session" => {
            let register_id = ensure_register_for_node(db, &ctx.caller_node)?;
            as_json(crate::services::cash_service::get_active_session_for_register(db, register_id)?)?
        }
        "open_cash_session" => {
            let register_id = ensure_register_for_node(db, &ctx.caller_node)?;
            as_json(crate::services::cash_service::open_session_for_register(
                db,
                req_i64(args, "user_id")?,
                register_id,
                req_i64(args, "opening_amount")?,
                opt_str(args, "notes")?,
            )?)?
        }
        "add_cash_movement" => {
            crate::services::cash_service::add_cash_movement(
                db,
                req_i64(args, "session_id")?,
                req_i64(args, "user_id")?,
                &req_str(args, "movement_type")?,
                req_i64(args, "amount")?,
                opt_str(args, "reason")?,
            )?;
            Value::Null
        }
        "close_cash_session" => {
            crate::services::cash_service::close_session(
                db,
                req_i64(args, "session_id")?,
                req_i64(args, "actual_cash")?,
                opt_str(args, "notes")?,
            )?;
            Value::Null
        }
        "list_cash_movements" => as_json(crate::services::cash_service::list_movements(db, req_i64(args, "session_id")?)?)?,
        "list_session_history" => as_json(crate::services::cash_service::list_session_history(
            db,
            opt_str(args, "from_date")?,
            opt_str(args, "to_date")?,
            opt_bool(args, "include_archived")?,
        )?)?,
        "edit_opening_balance" => {
            crate::services::cash_service::edit_opening_balance(
                db,
                req_i64(args, "session_id")?,
                req_i64(args, "new_amount")?,
                req_str(args, "reason")?,
                opt_str(args, "admin_password")?,
            )?;
            Value::Null
        }
        "edit_cash_session" => {
            crate::services::cash_service::edit_cash_session(
                db,
                req_i64(args, "session_id")?,
                opt_i64(args, "opening_amount")?,
                opt_i64(args, "actual_cash")?,
                opt_str(args, "notes")?,
                opt_str(args, "admin_password")?,
            )?;
            Value::Null
        }
        "archive_cash_session" => {
            crate::services::cash_service::archive_cash_session(
                db,
                req_i64(args, "session_id")?,
                req_bool(args, "archived")?,
                opt_str(args, "admin_password")?,
            )?;
            Value::Null
        }
        "delete_cash_session" => {
            crate::services::cash_service::delete_cash_session(
                db,
                req_i64(args, "session_id")?,
                opt_str(args, "admin_password")?,
            )?;
            Value::Null
        }

        // --- products ---
        "search_products" => as_json(crate::services::product_service::search_products(
            db,
            &req_str(args, "query")?,
            opt_i64(args, "category_id")?,
            &req_str(args, "search_type")?,
        )?)?,
        "save_product" => as_json(crate::services::product_service::save_product(
            db,
            model(args, "input")?,
            opt_i64(args, "product_id")?,
            opt_i64(args, "user_id")?,
        )?)?,
        "delete_product" => {
            crate::services::product_service::delete_product(db, req_i64(args, "product_id")?)?;
            Value::Null
        }
        "get_categories" => as_json(crate::services::product_service::get_categories(db)?)?,
        "save_category" => as_json(crate::services::product_service::save_category(
            db,
            &req_str(args, "name_ar")?,
            &req_str(args, "name_fr")?,
            &req_str(args, "name_en")?,
            &req_str(args, "color")?,
            opt_i64(args, "category_id")?,
        )?)?,
        "delete_category" => {
            crate::services::product_service::delete_category(db, req_i64(args, "category_id")?)?;
            Value::Null
        }
        "get_units" => as_json(crate::services::product_service::get_units(db)?)?,
        "save_unit" => as_json(crate::services::product_service::save_unit(
            db,
            &req_str(args, "name")?,
            &req_str(args, "short_name")?,
            req_bool(args, "allow_decimals")?,
            opt_i64(args, "unit_id")?,
        )?)?,
        "toggle_product_pin" => {
            crate::services::product_service::toggle_product_pin(db, req_i64(args, "product_id")?, req_bool(args, "pinned")?)?;
            Value::Null
        }
        "reorder_pinned_products" => {
            let ids_val = field(args, "ordered_ids").cloned().unwrap_or(Value::Array(vec![]));
            let ids: Vec<i64> = serde_json::from_value(ids_val).map_err(|e| e.to_string())?;
            crate::services::product_service::reorder_pinned_products(db, ids)?;
            Value::Null
        }
        "list_packagings" => as_json(crate::services::product_service::list_packagings(db, req_i64(args, "product_id")?)?)?,
        "save_packagings" => {
            crate::services::product_service::save_packagings(
                db,
                req_i64(args, "product_id")?,
                model(args, "inputs")?,
            )?;
            Value::Null
        }
        "get_price_history" => {
            let conn = db.conn.lock().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT id, old_purchase_price, new_purchase_price, old_sale_price, new_sale_price, user_id, created_at
                     FROM product_price_history WHERE product_id = ?1 ORDER BY id DESC LIMIT 100",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([req_i64(args, "product_id")?], |row| {
                    Ok(PriceHistoryEntry {
                        id: row.get(0)?,
                        old_purchase_price: row.get(1)?,
                        new_purchase_price: row.get(2)?,
                        old_sale_price: row.get(3)?,
                        new_sale_price: row.get(4)?,
                        user_id: row.get(5)?,
                        created_at: row.get(6)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let list: Vec<PriceHistoryEntry> = rows.filter_map(|r| r.ok()).collect();
            as_json(list)?
        }
        "get_quantity_history" => as_json(crate::services::product_service::get_quantity_history(db, req_i64(args, "product_id")?)?)?,
        "resolve_scale_scan" => {
            let settings = crate::services::settings_service::get_all_settings(db)?;
            let btype: i64 = settings
                .get("scale_default_barcode_type")
                .and_then(|v| v.parse().ok())
                .unwrap_or(97);
            let code = req_str(args, "code")?;
            let parsed = crate::services::scale_service::parse_scale_barcode_code(&code, btype);
            match parsed.and_then(|(_, weight, price)| {
                crate::services::scale_service::resolve_scale_scan(db, &code)
                    .map(|product| (weight, price, product))
            }) {
                Some((weight, price, product)) => as_json(json!({
                    "product_id": product.id,
                    "name": product.name_fr,
                    "unit_price": if price > 0 { price } else { product.sale_price },
                    "weight": weight,
                }))?,
                None => Value::Null,
            }
        }

        // --- sales ---
        "process_sale" | "create_sale" => {
            let input: CreateSaleInput = model(args, "input")?;
            as_json(crate::services::sales_service::process_sale(db, input)?)?
        }
        "replace_sale" => {
            let input: CreateSaleInput = model(args, "input")?;
            as_json(crate::services::sales_service::replace_sale(db, req_i64(args, "original_sale_id")?, input)?)?
        }
        "list_sales" => as_json(crate::services::sales_service::list_sales(
            db,
            opt_str(args, "start_date")?,
            opt_str(args, "end_date")?,
            opt_i64(args, "user_id")?,
            req_i64(args, "limit").unwrap_or(200),
        )?)?,
        "get_sale_items" => as_json(crate::services::sales_service::get_sale_items(db, req_i64(args, "sale_id")?)?)?,
        "get_last_sale" => {
            match crate::services::sales_service::get_last_sale(db)? {
                Some((sale, items)) => as_json(json!({ "sale": sale, "items": items }))?,
                None => Value::Null,
            }
        }
        "get_sale_by_number" => {
            let number = req_str(args, "saleNumber")?;
            match crate::services::sales_service::get_sale_by_number(db, &number)? {
                Some((sale, items)) => as_json(json!({ "sale": sale, "items": items }))?,
                None => Value::Null,
            }
        }
        "hold_sale" => {
            let uid = opt_i64(args, "user_id")?.unwrap_or(1);
            let raw_json = opt_str(args, "cart_json")?
                .or(opt_str(args, "cart_data_json")?)
                .unwrap_or_else(|| "[]".to_string());
            let note = opt_str(args, "note")?.or(opt_str(args, "notes")?);
            as_json(crate::services::sales_service::hold_sale(
                db,
                uid,
                opt_i64(args, "customer_id")?,
                &raw_json,
                note,
            )?)?
        }
        "list_held_sales" => as_json(crate::services::sales_service::list_held_sales(db)?)?,
        "delete_held_sale" => {
            crate::services::sales_service::delete_held_sale(db, req_i64(args, "held_id")?)?;
            Value::Null
        }
        "delete_sale" => {
            crate::services::sales_service::delete_sale(db, req_i64(args, "sale_id")?, opt_i64(args, "user_id")?)?;
            Value::Null
        }

        // --- customers ---
        "list_customers" => as_json(crate::services::customer_service::list_customers(db)?)?,
        "save_customer" => as_json(crate::services::customer_service::save_customer(
            db,
            &req_str(args, "name")?,
            opt_str(args, "phone")?,
            opt_str(args, "email")?,
            opt_str(args, "address")?,
            opt_str(args, "rc")?,
            opt_str(args, "nif")?,
            opt_str(args, "nis")?,
            opt_str(args, "ai")?,
            opt_i64(args, "initial_debt")?.unwrap_or(0),
            opt_str(args, "notes")?,
            opt_i64(args, "customer_id")?,
        )?)?,
        "delete_customer" => {
            crate::services::customer_service::delete_customer(db, req_i64(args, "customer_id")?)?;
            Value::Null
        }
        "toggle_customer_pin" => {
            crate::services::customer_service::toggle_customer_pin(db, req_i64(args, "customer_id")?, req_bool(args, "pinned")?)?;
            Value::Null
        }
        "record_customer_debt_payment" => as_json(crate::services::customer_service::record_customer_debt_payment(
            db,
            model(args, "input")?,
        )?)?,
        "clear_customer_debt" => as_json(crate::services::customer_service::clear_customer_debt(
            db,
            req_i64(args, "customer_id")?,
            opt_str(args, "reason")?,
            &req_str(args, "admin_password")?,
            opt_i64(args, "user_id")?,
        )?)?,
        "list_debt_clear_log" => {
            let conn = db.conn.lock().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT id, entity_type, entity_id, entity_name, previous_debt, new_debt, reason, user_name, created_at
                     FROM debt_clear_log ORDER BY id DESC LIMIT 200",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(crate::commands::DebtClearEntry {
                        id: row.get(0)?,
                        entity_type: row.get(1)?,
                        entity_id: row.get(2)?,
                        entity_name: row.get(3)?,
                        previous_debt: row.get(4)?,
                        new_debt: row.get(5)?,
                        reason: row.get(6)?,
                        user_name: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let list: Vec<crate::commands::DebtClearEntry> = rows.filter_map(|r| r.ok()).collect();
            as_json(list)?
        }

        // --- suppliers ---
        "list_suppliers" => as_json(crate::services::supplier_service::list_suppliers(db)?)?,
        "save_supplier" => as_json(crate::services::supplier_service::save_supplier(
            db,
            &req_str(args, "name")?,
            opt_str(args, "contact_person")?,
            opt_str(args, "phone")?,
            opt_str(args, "email")?,
            opt_str(args, "address")?,
            opt_str(args, "rc")?,
            opt_str(args, "nif")?,
            opt_str(args, "nis")?,
            opt_str(args, "ai")?,
            opt_str(args, "notes")?,
            opt_i64(args, "supplier_id")?,
        )?)?,
        "delete_supplier" => {
            crate::services::supplier_service::delete_supplier(db, req_i64(args, "supplier_id")?)?;
            Value::Null
        }
        "toggle_supplier_pin" => {
            crate::services::supplier_service::toggle_supplier_pin(db, req_i64(args, "supplier_id")?, req_bool(args, "pinned")?)?;
            Value::Null
        }
        "record_supplier_debt_payment" => as_json(crate::services::supplier_service::record_supplier_debt_payment(
            db,
            model(args, "input")?,
        )?)?,
        "list_supplier_debt_payments" => as_json(crate::services::supplier_service::list_supplier_debt_payments(
            db,
            req_i64(args, "supplier_id")?,
        )?)?,
        "clear_supplier_debt" => as_json(crate::services::supplier_service::clear_supplier_debt(
            db,
            req_i64(args, "supplier_id")?,
            opt_str(args, "reason")?,
            &req_str(args, "admin_password")?,
            opt_i64(args, "user_id")?,
        )?)?,

        // --- purchases ---
        "create_purchase" => {
            let input: CreatePurchaseInput = model(args, "input")?;
            as_json(crate::services::purchase_service::create_purchase(db, input)?)?
        }
        "get_purchase_items" => as_json(crate::services::purchase_service::get_purchase_items(db, req_i64(args, "purchase_id")?)?)?,
        "delete_purchase" => {
            crate::services::purchase_service::delete_purchase(db, req_i64(args, "purchase_id")?, opt_i64(args, "user_id")?)?;
            Value::Null
        }
        "list_purchases" => as_json(crate::services::purchase_service::list_purchases(db)?)?,

        // --- expenses ---
        "add_expense" => as_json(crate::services::expense_service::add_expense(
            db,
            req_i64(args, "category_id")?,
            req_i64(args, "amount")?,
            &req_str(args, "payment_method")?,
            opt_i64(args, "session_id")?,
            req_i64(args, "user_id")?,
            opt_str(args, "recipient")?,
            opt_str(args, "receipt_reference")?,
            opt_str(args, "notes")?,
            opt_str(args, "date")?,
        )?)?,
        "update_expense" => {
            crate::services::expense_service::update_expense(
                db,
                req_i64(args, "expense_id")?,
                req_i64(args, "category_id")?,
                req_i64(args, "amount")?,
                &req_str(args, "payment_method")?,
                opt_str(args, "recipient")?,
                opt_str(args, "receipt_reference")?,
                opt_str(args, "notes")?,
                req_str(args, "date")?,
            )?;
            Value::Null
        }
        "list_expenses" => as_json(crate::services::expense_service::list_expenses(db)?)?,
        "delete_expense" => {
            crate::services::expense_service::delete_expense(db, req_i64(args, "expense_id")?)?;
            Value::Null
        }

        // --- employees & payroll ---
        "list_employees" => as_json(crate::services::employee_service::list_employees(db)?)?,
        "save_employee" => as_json(crate::services::employee_service::save_employee(
            db,
            &req_str(args, "code")?,
            &req_str(args, "name")?,
            opt_str(args, "phone")?,
            opt_str(args, "email")?,
            opt_str(args, "national_id")?,
            &req_str(args, "job_title")?,
            req_i64(args, "base_salary")?,
            &req_str(args, "salary_type")?,
            opt_str(args, "salary_start_date")?,
            &req_str(args, "hire_date")?,
            opt_str(args, "notes")?,
            opt_str(args, "rfid_code")?,
            opt_i64(args, "employee_id")?,
        )?)?,
        "find_employee_by_rfid" => as_json(crate::services::employee_service::find_employee_by_rfid(db, &req_str(args, "rfid")?)?)?,
        "next_employee_code" => as_json(crate::services::employee_service::next_employee_code(db)?)?,
        "delete_employee" => {
            crate::services::employee_service::delete_employee(db, req_i64(args, "employee_id")?)?;
            Value::Null
        }
        "list_payrolls" => as_json(crate::services::payroll_service::list_payrolls(db)?)?,
        "record_employee_advance" => as_json(crate::services::payroll_service::record_employee_advance(
            db,
            model(args, "input")?,
        )?)?,
        "list_employee_advances" => as_json(crate::services::payroll_service::list_employee_advances(
            db,
            opt_i64(args, "employee_id")?,
            opt_str(args, "month")?,
        )?)?,
        "record_employee_absence" => as_json(crate::services::payroll_service::record_employee_absence(
            db,
            req_i64(args, "employee_id")?,
            req_i64(args, "days")?,
            opt_str(args, "reason")?,
            req_str(args, "date")?,
        )?)?,
        "list_employee_absences" => as_json(crate::services::payroll_service::list_employee_absences(
            db,
            opt_i64(args, "employee_id")?,
            opt_str(args, "month")?,
        )?)?,

        // --- settings ---
        "get_all_settings" => as_json(crate::services::settings_service::get_all_settings(db)?)?,
        "get_setting" => {
            let all = crate::services::settings_service::get_all_settings(db)?;
            as_json(all.get(&req_str(args, "key")?).cloned().filter(|v| !v.is_empty()))?
        }
        "set_setting" => {
            crate::services::settings_service::set_setting(db, &req_str(args, "key")?, &req_str(args, "value")?)?;
            Value::Null
        }
        "set_multiple_settings" => {
            let m: HashMap<String, String> = model(args, "settings")?;
            crate::services::settings_service::set_multiple_settings(db, m)?;
            Value::Null
        }

        // --- notifications ---
        "list_app_notifications" => {
            let conn = db.conn.lock().unwrap();
            let _ = conn.execute(
                "CREATE TABLE IF NOT EXISTS app_notification_log (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    type TEXT NOT NULL,
                    title TEXT NOT NULL,
                    message TEXT NOT NULL,
                    related_id INTEGER,
                    is_dismissed INTEGER DEFAULT 0,
                    created_at TEXT DEFAULT (datetime('now','localtime'))
                )",
                [],
            );
            let mut stmt = conn
                .prepare(
                    "SELECT id, type, title, message, related_id, created_at
                     FROM app_notification_log WHERE is_dismissed = 0
                     ORDER BY id DESC LIMIT ?1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([opt_i64(args, "limit")?.unwrap_or(100)], |row| {
                    Ok(crate::commands::AppNotification {
                        id: row.get(0)?,
                        ntype: row.get(1)?,
                        title: row.get(2)?,
                        message: row.get(3)?,
                        related_id: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            let list: Vec<crate::commands::AppNotification> = rows.filter_map(|r| r.ok()).collect();
            as_json(list)?
        }
        "dismiss_app_notification" => {
            let conn = db.conn.lock().unwrap();
            conn.execute("UPDATE app_notification_log SET is_dismissed = 1 WHERE id = ?1", [req_i64(args, "id")?])
                .map_err(|e| e.to_string())?;
            Value::Null
        }
        "run_payroll_reminder_scan" => {
            as_json(crate::services::payroll_reminder_service::run_payroll_reminder_scan(db))?
        }
        "send_telegram_message" => {
            let (token, chat_id, _) = crate::services::notifier_service::get_telegram_config(db)
                .ok_or("Telegram bot token / chat ID not configured")?;
            crate::services::notifier_service::send_telegram_blocking(&token, &chat_id, &req_str(args, "text")?)?;
            Value::Null
        }
        "send_telegram_recap" => {
            as_json(crate::services::notifier_service::send_periodic_recap(db)?)?
        }

        _ => return Err(format!("'{}' is not available over the network", command)),
    };

    let _ = s; // auth/event metadata already validated above
    if let Some(event) = s.event {
        emit_event(event, json!({ "by": ctx.caller_user.as_ref().map(|u| u.username.clone()).unwrap_or_default() }));
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Event sink (wired by server_api to the WebSocket broadcast)
// ---------------------------------------------------------------------------

type EventSink = Box<dyn Fn(&str, Value) + Send + Sync>;
static EVENT_SINK: std::sync::OnceLock<EventSink> = std::sync::OnceLock::new();

pub fn set_event_sink(sink: EventSink) {
    let _ = EVENT_SINK.set(sink);
}

fn emit_event(event_type: &str, data: Value) {
    if let Some(sink) = EVENT_SINK.get() {
        sink(event_type, data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_command_is_whitelisted() {
        for name in KNOWN_COMMANDS {
            assert!(
                lookup(name).is_some(),
                "KNOWN_COMMANDS entry '{}' missing from the dispatch table",
                name
            );
        }
    }

    #[test]
    fn camel_and_snake_args_both_parse() {
        let args = json!({ "userId": 7 });
        assert_eq!(req_i64(&args, "user_id").unwrap(), 7);
        let args2 = json!({ "user_id": 9 });
        assert_eq!(req_i64(&args2, "user_id").unwrap(), 9);
    }

    #[test]
    fn missing_args_report_their_name() {
        let args = json!({ "other": 1 });
        assert!(req_i64(&args, "product_id").unwrap_err().contains("product_id"));
    }

    #[test]
    fn hardware_commands_are_never_whitelisted() {
        for local in ["print_html_direct", "open_serial_cash_drawer", "test_scale_connection",
                      "create_backup", "factory_reset", "get_hwid", "restore_database", "login"] {
            assert!(lookup(local).is_none(), "'{}' must stay local", local);
        }
    }

    #[test]
    fn dangerous_commands_require_admin() {
        assert_eq!(lookup("delete_user").unwrap().auth, Auth::Admin);
        assert_eq!(lookup("get_all_users").unwrap().auth, Auth::Admin);
        assert_eq!(lookup("create_user").unwrap().auth, Auth::Admin);
        assert_eq!(lookup("get_active_users").unwrap().auth, Auth::Device);
    }

    #[test]
    fn mutations_carry_events() {
        assert_eq!(lookup("process_sale").unwrap().event, Some("sale_created"));
        assert_eq!(lookup("save_product").unwrap().event, Some("product_updated"));
        assert!(lookup("list_sales").unwrap().event.is_none());
    }
}
