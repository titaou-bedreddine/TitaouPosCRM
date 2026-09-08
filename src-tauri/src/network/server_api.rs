//! LAN shop network API (mounted at `/api/v1/*` on the embedded server).
//!
//! The coordinator owns the authoritative SQLite database and serves:
//!   GET  /api/v1/health           — availability + identity + term
//!   POST /api/v1/network/join     — device registration (shop confirmation)
//!   POST /api/v1/network/leave    — device deregistration
//!   GET  /api/v1/network/status   — devices + coordinator snapshot
//!   POST /api/v1/auth/login       — user login (reuses existing users table)
//!   POST /api/v1/invoke/{command} — whitelisted business operations
//!   GET  /api/v1/ws               — real-time events (WebSocket)
//!
//! Discovery packets NEVER authenticate; every business call needs a token.

use super::invoke_registry::{self, CallerUser, InvokeContext};
use super::identity::random_hex;
use crate::database::DbState;
use axum::extract::{Path, Query, State as AxState, ws::{Message, WebSocket, WebSocketUpgrade}};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// The shared Axum state type used by the embedded server (defined in
// crate::server; aliased here to keep this module's signatures readable).
use crate::server::ServerState;
pub type SharedServerState = std::sync::Arc<ServerState>;

// ---------------------------------------------------------------------------
// API state
// ---------------------------------------------------------------------------

static API_DB: OnceLock<DbState> = OnceLock::new();
static TOKENS: OnceLock<Mutex<HashMap<String, TokenEntry>>> = OnceLock::new();
static DEVICES: OnceLock<Mutex<HashMap<String, LanDevice>>> = OnceLock::new();
static EVENT_TX: OnceLock<tokio::sync::broadcast::Sender<String>> = OnceLock::new();
pub static PROTOCOL_VERSION: u32 = 1;

/// Set the authoritative DB handle (called once at startup, after DbState).
pub fn set_api_db(db: DbState) {
    let _ = API_DB.set(db);
}

fn tokens() -> &'static Mutex<HashMap<String, TokenEntry>> {
    TOKENS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn devices() -> &'static Mutex<HashMap<String, LanDevice>> {
    DEVICES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn event_tx() -> &'static tokio::sync::broadcast::Sender<String> {
    EVENT_TX.get_or_init(|| {
        let (tx, _rx) = tokio::sync::broadcast::channel(256);
        tx
    })
}

#[derive(Debug, Clone)]
pub struct TokenEntry {
    pub kind: TokenKind,
    pub node_id: String,
    pub user: Option<CallerUser>,
    pub expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Device,
    User,
}

/// A joined LAN terminal (server-side registry).
#[derive(Debug, Clone)]
pub struct LanDevice {
    pub node_id: String,
    pub pc_name: String,
    pub role_pref: String,
    pub ip: String,
    pub app_version: String,
    pub last_seen: Instant,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn token_ttl() -> Duration {
    Duration::from_secs(24 * 3600)
}

fn issue_token(kind: TokenKind, node_id: &str, user: Option<CallerUser>) -> String {
    let token = format!("TPS-{}-{}", if kind == TokenKind::Device { "DEV" } else { "USR" }, random_hex(16));
    tokens().lock().unwrap().insert(
        token.clone(),
        TokenEntry { kind, node_id: node_id.to_string(), user, expires_at: Instant::now() + token_ttl() },
    );
    token
}

fn prune_tokens() {
    let mut map = tokens().lock().unwrap();
    map.retain(|_, t| t.expires_at > Instant::now());
}

/// Validate a bearer token and return its entry.
fn validate_token(token: &str) -> Option<TokenEntry> {
    prune_tokens();
    tokens().lock().unwrap().get(token).cloned()
}

/// Current term of this shop's coordinator (managed by the network module).
pub fn coordinator_term() -> u64 {
    super::current_term()
}

// ---------------------------------------------------------------------------
// Shop identity accessors (provided by the network manager)
// ---------------------------------------------------------------------------

fn shop_info() -> (String, String, String, String, bool) {
    super::server_shop_info()
}

// ---------------------------------------------------------------------------
// Event broadcast
// ---------------------------------------------------------------------------

/// Broadcast a business event to all connected terminals (WS) AND to this
/// PC's own UI (Tauri event) so the server screen refreshes too.
pub fn broadcast_event(event_type: &str, data: Value) {
    let payload = json!({
        "type": "event",
        "event": {
            "type": event_type,
            "data": data,
            "term": coordinator_term(),
            "ts": now_secs(),
        }
    });
    let _ = event_tx().send(payload.to_string());
    super::emit_net_event(payload);
}

// ---------------------------------------------------------------------------
// Device registry management (used by Settings UI commands)
// ---------------------------------------------------------------------------

pub fn join_device(
    node_id: String,
    pc_name: String,
    role_pref: String,
    ip: String,
    app_version: String,
) -> String {
    let pc_name_saved = pc_name.clone();
    let app_version_saved = app_version.clone();
    devices().lock().unwrap().insert(
        node_id.clone(),
        LanDevice { node_id: node_id.clone(), pc_name, role_pref, ip, app_version, last_seen: Instant::now() },
    );
    // Persist membership: a server restart wipes the in-memory maps, but a
    // reconnecting client with a cached token must be recognized, not
    // become a ghost (its token validated against this registry).
    if let Some(db) = API_DB.get() {
        let _ = crate::services::settings_service::set_setting(
            db,
            &format!("net_member_{}", node_id),
            &format!("{}|{}", pc_name_saved, app_version_saved),
        );
    }
    super::log_net_event("client_connected", json!({ "node": node_id }));
    issue_token(TokenKind::Device, &node_id, None)
}

/// Is this node_id a known shop member (persisted across restarts)?
pub fn is_registered_member(node_id: &str) -> bool {
    match API_DB.get() {
        Some(db) => crate::services::settings_service::get_all_settings(db)
            .map(|s| s.contains_key(&format!("net_member_{}", node_id)))
            .unwrap_or(false),
        None => false,
    }
}

pub fn touch_device(node_id: &str) {
    if let Some(d) = devices().lock().unwrap().get_mut(node_id) {
        d.last_seen = Instant::now();
    }
}

pub fn remove_device(node_id: &str) {
    devices().lock().unwrap().remove(node_id);
    super::log_net_event("client_disconnected", json!({ "node": node_id }));
}

pub fn devices_snapshot() -> Vec<Value> {
    let map = devices().lock().unwrap();
    map.values()
        .map(|d| {
            json!({
                "node_id": d.node_id,
                "pc_name": d.pc_name,
                "role_pref": d.role_pref,
                "ip": d.ip,
                "app_version": d.app_version,
                "online": d.last_seen.elapsed() < Duration::from_secs(45),
                "last_seen_secs_ago": d.last_seen.elapsed().as_secs(),
            })
        })
        .collect()
}

pub fn revoke_device_tokens(node_id: &str) {
    tokens().lock().unwrap().retain(|_, t| t.node_id != node_id);
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn v1_router() -> Router<SharedServerState> {
    Router::new()
        .route("/api/v1/health", get(api_health))
        .route("/api/v1/network/join", post(api_join))
        .route("/api/v1/network/leave", post(api_leave))
        .route("/api/v1/network/status", get(api_network_status))
        .route("/api/v1/auth/login", post(api_login))
        .route("/api/v1/invoke/:command", post(api_invoke))
        .route("/api/v1/ws", get(api_ws_upgrade))
}

#[derive(Deserialize)]
struct TokenQuery {
    #[serde(default)]
    token: String,
}

fn bearer_from_headers(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_string()
}

fn err_json(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, Json(json!({ "ok": false, "error": msg }))).into_response()
}

// --- health ---------------------------------------------------------------

async fn api_health() -> axum::response::Response {
    let (node_id, pc_name, shop_id, shop_name, is_coordinator) = shop_info();
    if !is_coordinator {
        return err_json(StatusCode::SERVICE_UNAVAILABLE, "This node is not the shop server");
    }
    Json(json!({
        "status": "ok",
        "server_id": node_id,
        "server_pc": pc_name,
        "shop_id": shop_id,
        "shop_name": shop_name,
        // App discriminator: a TitaouPOS client (different app) must see a
        // mismatch and refuse to adopt us as ITS shop server, exactly like
        // we refuse TitaouPOS servers.
        "app": super::discovery::SHOP_KIND,
        "app_version": env!("CARGO_PKG_VERSION"),
        "protocol_version": PROTOCOL_VERSION,
        "term": coordinator_term(),
        "timestamp": now_secs(),
        "devices_count": devices().lock().unwrap().len(),
    }))
    .into_response()
}

// --- join ------------------------------------------------------------------

#[derive(Deserialize)]
struct JoinBody {
    node_id: String,
    #[serde(default)]
    pc_name: String,
    #[serde(default)]
    role_pref: String,
    #[serde(default)]
    app_version: String,
    #[serde(default)]
    shop_id: String,
    /// Sending app ("titaouposcrm"). Empty = foreign/legacy client → refused.
    #[serde(default)]
    app: String,
    #[serde(default)]
    confirm: bool,
}

async fn api_join(
    AxState(_state): AxState<SharedServerState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    body: Json<JoinBody>,
) -> axum::response::Response {
    let (node_id, _pc, shop_id, shop_name, is_coordinator) = shop_info();
    if !is_coordinator {
        return err_json(StatusCode::SERVICE_UNAVAILABLE, "Not coordinating a shop network");
    }
    // A node must name the shop it joins; a silent join of a random nearby
    // shop is never allowed.
    if body.shop_id != shop_id {
        return err_json(StatusCode::FORBIDDEN, "This terminal targets a different shop");
    }
    // App isolation: a TitaouPOS (old app) terminal must never join a
    // TitaouPosCRM shop, and vice versa.
    if body.app != super::discovery::SHOP_KIND {
        return err_json(StatusCode::FORBIDDEN, "Different application family — this shop runs TitaouPosCRM");
    }
    if super::is_node_blocked(&body.node_id) {
        return err_json(StatusCode::FORBIDDEN, "This terminal has been blocked by the shop administrator");
    }
    let term = coordinator_term();
    if !body.confirm {
        // Confirmation step: show the shop, register nothing.
        return Json(json!({
            "ok": true,
            "confirm_required": true,
            "shop_id": shop_id,
            "shop_name": shop_name,
            "coordinator": node_id,
            "term": term,
            "devices_count": devices().lock().unwrap().len(),
        }))
        .into_response();
    }
    let token = join_device(
        body.node_id.clone(),
        if body.pc_name.trim().is_empty() { format!("Terminal ({})", addr.ip()) } else { body.pc_name.clone() },
        if body.role_pref.trim().is_empty() { "client".into() } else { body.role_pref.clone() },
        addr.ip().to_string(),
        if body.app_version.trim().is_empty() { "?".into() } else { body.app_version.clone() },
    );
    super::log_net_event("device_registered", json!({ "node": body.pc_name, "ip": addr.ip().to_string() }));
    Json(json!({
        "ok": true,
        "device_token": token,
        "shop_id": shop_id,
        "shop_name": shop_name,
        "coordinator": { "node_id": node_id, "pc_name": _pc },
        "term": term,
    }))
    .into_response()
}

// --- leave -----------------------------------------------------------------

async fn api_leave(
    AxState(_state): AxState<SharedServerState>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    let Some(tok) = validate_token(&bearer_from_headers(&headers)) else {
        return err_json(StatusCode::UNAUTHORIZED, "Invalid token");
    };
    remove_device(&tok.node_id);
    revoke_device_tokens(&tok.node_id);
    Json(json!({ "ok": true })).into_response()
}

// --- network status (for client Settings mirror) ----------------------------

async fn api_network_status(
    AxState(_state): AxState<SharedServerState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
) -> axum::response::Response {
    let bearer = bearer_from_headers(&headers);
    let tok = validate_token(if bearer.is_empty() { &q.token } else { &bearer });
    let Some(tok) = tok else {
        return err_json(StatusCode::UNAUTHORIZED, "Invalid token");
    };
    // Server restart wiped the token map but membership persists in the DB:
    // re-validate the token holder against the persisted member registry.
    if devices().lock().unwrap().get(&tok.node_id).is_none() && is_registered_member(&tok.node_id) {
        // Re-seed the live registry from the persisted member row.
        if let Some(db) = API_DB.get() {
            if let Ok(all) = crate::services::settings_service::get_all_settings(db) {
                if let Some(row) = all.get(&format!("net_member_{}", tok.node_id)) {
                    let mut parts = row.splitn(2, '|');
                    let pc_name = parts.next().unwrap_or("Terminal").to_string();
                    let ver = parts.next().unwrap_or("?").to_string();
                    devices().lock().unwrap().insert(
                        tok.node_id.clone(),
                        LanDevice {
                            node_id: tok.node_id.clone(),
                            pc_name,
                            role_pref: "client".to_string(),
                            ip: String::new(),
                            app_version: ver,
                            last_seen: Instant::now(),
                        },
                    );
                }
            }
        }
    }
    touch_device(&tok.node_id);
    let (node_id, pc_name, shop_id, shop_name, is_coordinator) = shop_info();
    if !is_coordinator {
        return err_json(StatusCode::SERVICE_UNAVAILABLE, "Not coordinating a shop network");
    }
    Json(json!({
        "ok": true,
        "shop_id": shop_id,
        "shop_name": shop_name,
        "coordinator": { "node_id": node_id, "pc_name": pc_name },
        "term": coordinator_term(),
        "devices": devices_snapshot(),
        "app_version": env!("CARGO_PKG_VERSION"),
    }))
    .into_response()
}

// --- login ------------------------------------------------------------------

#[derive(Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

async fn api_login(
    AxState(_state): AxState<SharedServerState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
    body: Json<LoginBody>,
) -> axum::response::Response {
    let bearer = bearer_from_headers(&headers);
    let tok = validate_token(if bearer.is_empty() { &q.token } else { &bearer });
    // Login requires a REGISTERED device (join first) — discovery is never auth.
    let Some(tok) = tok.filter(|t| t.kind == TokenKind::Device) else {
        return err_json(StatusCode::UNAUTHORIZED, "Device not registered on this shop server");
    };
    let Some(db) = API_DB.get() else {
        return err_json(StatusCode::INTERNAL_SERVER_ERROR, "Server database unavailable");
    };
    let user = match crate::auth::authenticate_user(db, &body.username, &body.password) {
        Ok(u) => u,
        Err(_) => None,
    };
    let Some(user) = user else {
        super::log_net_event("authentication_failed", json!({ "username": body.username }));
        return err_json(StatusCode::UNAUTHORIZED, "Wrong username or password");
    };
    let caller = CallerUser {
        user_id: user.id,
        username: user.username.clone(),
        role_id: user.role_id,
        role_name: user.role_name.clone(),
    };
    let token = issue_token(TokenKind::User, &tok.node_id, Some(caller));
    super::log_net_event("client_authenticated", json!({ "node": tok.node_id, "user": user.username }));
    Json(json!({ "ok": true, "user": user, "token": token, "term": coordinator_term() })).into_response()
}

// --- invoke -----------------------------------------------------------------

async fn api_invoke(
    AxState(_state): AxState<SharedServerState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Path(command): Path<String>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
    body: Option<Json<Value>>,
) -> axum::response::Response {
    let bearer = bearer_from_headers(&headers);
    let Some(tok) = validate_token(if bearer.is_empty() { &q.token } else { &bearer }) else {
        return err_json(StatusCode::UNAUTHORIZED, "Authentication required");
    };
    let Some(spec) = invoke_registry::lookup(&command) else {
        return err_json(StatusCode::NOT_FOUND, &format!("'{}' is not available over the network (local-only operation)", command));
    };
    // Device-only tokens may only run the pre-login surface.
    if spec.auth != invoke_registry::Auth::Device && tok.kind == TokenKind::Device {
        return err_json(StatusCode::UNAUTHORIZED, "Log in before running shop operations");
    }
    let Some(db) = API_DB.get() else {
        return err_json(StatusCode::INTERNAL_SERVER_ERROR, "Server database unavailable");
    };
    let args = body.map(|Json(v)| v).unwrap_or(Value::Null);
    touch_device(&tok.node_id);
    let ctx = InvokeContext {
        db,
        caller_node: tok.node_id.clone(),
        caller_user: tok.user.clone(),
    };
    // Stamp the CALLING terminal's PC name on records created by this
    // request (read by INSERTs via current_stamp_terminal).
    super::set_caller_terminal(&super::stamping_terminal(&tok.node_id));
    // Business operations run on the blocking thread pool: SQLite work must
    // never stall the WS/event loop.
    let command2 = command.clone();
    let caller_pc = super::stamping_terminal(&tok.node_id);
    let res = tokio::task::spawn_blocking(move || {
        super::set_caller_terminal(&caller_pc);
        let r = invoke_registry::dispatch(&ctx, &command2, &args);
        super::clear_caller_terminal();
        r
    })
    .await
    .unwrap_or_else(|e| Err(format!("dispatch join error: {}", e)));
    match res {
        Ok(result) => {
            let mut envelope = json!({ "ok": true, "result": result });
            // RFID badge login mints a user token directly (badges carry no
            // password): the terminal stores it for subsequent API calls.
            if command == "login_with_rfid" {
                if let Some(obj) = envelope.get("result") {
                    if let (Some(id), Some(username)) = (
                        obj.get("id").and_then(|v| v.as_i64()),
                        obj.get("username").and_then(|v| v.as_str()),
                    ) {
                        let caller = CallerUser {
                            user_id: id,
                            username: username.to_string(),
                            role_id: obj.get("role_id").and_then(|v| v.as_i64()),
                            role_name: obj.get("role_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        };
                        let t = issue_token(TokenKind::User, &tok.node_id, Some(caller));
                        envelope["auth_token"] = Value::String(t);
                    }
                }
            }
            Json(envelope).into_response()
        }
        Err(e) => {
            super::log_net_event("api_failure", json!({ "command": command, "from": addr.ip().to_string(), "error": e.clone() }));
            let status = if e.contains("Administrator") || e.contains("not available") {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(json!({ "ok": false, "error": e }))).into_response()
        }
    }
}

// --- websocket ----------------------------------------------------------------

async fn api_ws_upgrade(
    AxState(_state): AxState<SharedServerState>,
    Query(q): Query<TokenQuery>,
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
) -> axum::response::Response {
    let bearer = bearer_from_headers(&headers);
    let Some(tok) = validate_token(if bearer.is_empty() { &q.token } else { &bearer }) else {
        return err_json(StatusCode::UNAUTHORIZED, "Authentication required");
    };
    touch_device(&tok.node_id);
    ws.on_upgrade(move |socket| handle_ws_socket(socket, tok.node_id))
}

async fn handle_ws_socket(mut socket: WebSocket, node_id: String) {
    let mut rx = event_tx().subscribe();
    let mut ping_tick = tokio::time::interval(Duration::from_secs(15));
    // First message: full hello (also lets the client verify the term).
    let hello = json!({
        "type": "hello",
        "term": coordinator_term(),
        "ts": now_secs(),
    });
    if socket.send(Message::Text(hello.to_string())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(txt) => {
                        if socket.send(Message::Text(txt)).await.is_err() { break; }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            _ = ping_tick.tick() => {
                let ping = json!({ "type": "ping", "term": coordinator_term(), "ts": now_secs() });
                if socket.send(Message::Text(ping.to_string())).await.is_err() { break; }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(_)) => { /* client pongs/ignores */ }
                    _ => break,
                }
            }
        }
    }
    touch_device(&node_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_expire_and_validate() {
        let t = issue_token(TokenKind::Device, "NODE-T", None);
        assert!(validate_token(&t).is_some());
        assert_eq!(validate_token(&t).unwrap().kind, TokenKind::Device);
        // Force-expire then prune.
        if let Some(e) = tokens().lock().unwrap().get_mut(&t) {
            e.expires_at = Instant::now() - Duration::from_secs(1);
        }
        assert!(validate_token(&t).is_none());
    }

    #[test]
    fn user_tokens_differ_from_device_tokens() {
        let d = issue_token(TokenKind::Device, "NODE-T", None);
        let u = issue_token(
            TokenKind::User,
            "NODE-T",
            Some(CallerUser { user_id: 1, username: "admin".into(), role_id: Some(1), role_name: Some("Administrator".into()) }),
        );
        assert_ne!(d, u);
        assert_eq!(validate_token(&u).unwrap().user.as_ref().unwrap().is_admin(), true);
    }
}
