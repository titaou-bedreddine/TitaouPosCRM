//! LAN shop networking for TitaouPOS (this phase: ONE shop, one LAN, no
//! cloud). Every installation carries both roles:
//!
//!   Server     → prefer coordinator (advertise + serve the authoritative
//!                SQLite via /api/v1)
//!   Client     → seek + join the shop server; all business ops go through
//!                the authenticated API (never a shared SQLite file)
//!   Automatic  → discover peers and deterministically elect the coordinator
//!
//! Determinism: explicit Server > richer local data > lowest Node ID, with a
//! monotonic term guarding against stale leadership (split-brain).
//! Hardware (printers, drawer, scale) always stays local to each PC.

pub mod client;
pub mod discovery;
pub mod election;
pub mod identity;
pub mod invoke_registry;
pub mod server_api;

#[cfg(test)]
pub mod api_live_tests;
#[cfg(test)]
pub mod discovery_live_tests;

use crate::database::DbState;
use crate::services::settings_service;
use discovery::DiscoveryPacket;
use election::{Peer, Resolution};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Config (persisted in app_settings — net_* keys are per-PC, never forwarded)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NetConfig {
    pub enabled: bool,
    pub role: String, // server | client | automatic
    pub node_id: String,
    pub pc_name: String,
    pub shop_id: String,
    pub shop_name: String,
    pub manual_server: String, // host[:port] — advanced override
    pub autodiscovery: bool,
    pub autoreconnect: bool,
    pub device_token: String,
    pub device_token_shop: String,
    /// host[:port] of the last server this node successfully connected to
    /// or joined — tried FIRST on every startup, immune to discovery
    /// problems (broadcast filtering etc.).
    pub last_server: String,
    pub blocked_nodes: Vec<String>,
}

impl NetConfig {
    pub fn load(db: &DbState) -> Self {
        let s = settings_service::get_all_settings(db).unwrap_or_default();
        let get = |k: &str| s.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let raw_node = get("net_node_id").unwrap_or_default();
        let node_id = if raw_node.is_empty() {
            identity::load_identity(db).node_id
        } else {
            raw_node
        };
        let pc = get("net_pc_name").unwrap_or_else(|| identity::default_pc_name(&node_id));
        NetConfig {
            enabled: get("net_enabled").map(|v| v == "true").unwrap_or(true),
            role: get("net_role").unwrap_or_else(|| "automatic".into()),
            node_id,
            pc_name: pc,
            shop_id: get("net_shop_id").unwrap_or_default(),
            shop_name: get("net_shop_name").unwrap_or_default(),
            manual_server: get("net_manual_server").unwrap_or_default(),
            autodiscovery: get("net_autodiscovery").map(|v| v == "true").unwrap_or(true),
            autoreconnect: get("net_autoreconnect").map(|v| v == "true").unwrap_or(true),
            device_token: get("net_device_token").unwrap_or_default(),
            device_token_shop: get("net_device_token_shop").unwrap_or_default(),
            last_server: get("net_last_server").unwrap_or_default(),
            blocked_nodes: get("net_blocked_nodes")
                .map(|v| v.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
                .unwrap_or_default(),
        }
    }

    pub fn save_field(db: &DbState, key: &str, value: &str) {
        let _ = settings_service::set_setting(db, key, value);
    }
}

/// Settings that describe THIS PC only (hardware, licensing, UI, network
/// identity) — never forwarded to the shop server, and always won locally
/// when settings are merged on a client terminal.
pub const LOCAL_ONLY_SETTINGS: &[&str] = &[
    "invoice_printer_name",
    "receipt_printer_dpi",
    "label_printer_name",
    "drawer_com_port",
    "drawer_baud_rate",
    "mobile_server_port",
    "ui_language",
    "first_setup_completed",
    "net_enabled",
    "net_role",
    "net_pc_name",
    "net_node_id",
    "net_shop_id",
    "net_shop_name",
    "net_manual_server",
    "net_autodiscovery",
    "net_autoreconnect",
    "net_device_token",
    "net_device_token_shop",
    "net_last_server",
    "net_blocked_nodes",
    "net_term",
    "net_cached_users",
];

pub fn is_local_only_setting(key: &str) -> bool {
    LOCAL_ONLY_SETTINGS.contains(&key) || key.starts_with("net_") || key.starts_with("scale_")
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Disabled,
    /// Serving as the shop authority (explicit Server role).
    Server,
    /// Automatic node with no peers: acts as its own (single-PC) authority.
    Standalone,
    /// Connected to a shop server as a client terminal.
    Connected,
    /// Looking for a server (client start / discovery).
    Searching,
    /// Server lost; retrying.
    Reconnecting,
    /// Explicit client that cannot reach its server (stays offline, never
    /// silently invents a second authority).
    Offline,
}

impl Mode {
    fn as_str(&self) -> &'static str {
        match self {
            Mode::Disabled => "disabled",
            Mode::Server => "server",
            Mode::Standalone => "standalone",
            Mode::Connected => "connected",
            Mode::Searching => "searching",
            Mode::Reconnecting => "reconnecting",
            Mode::Offline => "offline",
        }
    }
    pub fn serving(&self) -> bool {
        matches!(self, Mode::Server | Mode::Standalone)
    }
}

pub struct NetRuntime {
    pub db: OnceLock<DbState>,
    pub mode: Mutex<Mode>,
    pub peers: Mutex<HashMap<String, Peer>>,
    pub coordinator: Mutex<Option<Peer>>,
    pub server_base: Mutex<Option<String>>,
    pub device_token: Mutex<Option<String>>,
    pub user_token: Mutex<Option<String>>,
    pub term: AtomicU64,
    pub highest_seen_term: AtomicU64,
    pub events: Mutex<VecDeque<Value>>,
    pub last_event: Mutex<Option<Value>>,
    pub mdns: Mutex<Option<discovery::MdnsHandle>>,
    pub wake: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    pub ws_stop: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    pub data_rows_cache: Mutex<Option<(Instant, u64)>>,
    pub lan_ips_cache: Mutex<Option<(Instant, Vec<String>)>>,
    pub users_cache: Mutex<Option<Value>>,
}

static NET: OnceLock<NetRuntime> = OnceLock::new();

fn net_opt() -> Option<&'static NetRuntime> {
    NET.get()
}

fn net() -> &'static NetRuntime {
    net_opt().expect("network runtime not initialized")
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// UI emitter injected at startup (keeps the networking layer decoupled from
/// Tauri — and keeps Tauri's GUI object code out of test binaries).
type StatusEmitter = Box<dyn Fn(Value) + Send + Sync>;
static STATUS_EMITTER: OnceLock<StatusEmitter> = OnceLock::new();
type EventUiEmitter = Box<dyn Fn(Value) + Send + Sync>;
static EVENT_UI_EMITTER: OnceLock<EventUiEmitter> = OnceLock::new();

/// Push one network event into the local webview (no-op until wired).
pub(crate) fn emit_net_event(entry: Value) {
    if let Some(emit) = EVENT_UI_EMITTER.get() {
        emit(entry);
    }
}

/// One network event log line (never contains passwords or tokens).
pub fn log_net_event(event_type: &str, data: Value) {
    let Some(rt) = net_opt() else { return };
    let entry = json!({ "type": event_type, "data": data, "ts": now_secs() });
    {
        let mut ev = rt.events.lock().unwrap();
        ev.push_front(entry.clone());
        ev.truncate(50);
    }
    *rt.last_event.lock().unwrap() = Some(entry.clone());
    emit_status();
    println!("[net] {} {}", event_type, data);
}

fn emit_status() {
    if let Some(emit) = STATUS_EMITTER.get() {
        emit(status_snapshot());
    }
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

pub fn init(app: tauri::AppHandle, db: DbState) {
    let runtime = NetRuntime {
        db: OnceLock::new(),
        mode: Mutex::new(Mode::Searching),
        peers: Mutex::new(HashMap::new()),
        coordinator: Mutex::new(None),
        server_base: Mutex::new(None),
        device_token: Mutex::new(None),
        user_token: Mutex::new(None),
        term: AtomicU64::new(1),
        highest_seen_term: AtomicU64::new(1),
        events: Mutex::new(VecDeque::new()),
        last_event: Mutex::new(None),
        mdns: Mutex::new(None),
        wake: Mutex::new(None),
        ws_stop: Mutex::new(None),
        data_rows_cache: Mutex::new(None),
        lan_ips_cache: Mutex::new(None),
        users_cache: Mutex::new(None),
    };
    let _ = NET.set(runtime);
    let rt = net();

    let _ = rt.db.set(db);

    // UI status emitter (the only place the network layer touches Tauri).
    let emitter_app = app.clone();
    let _ = STATUS_EMITTER.set(Box::new(move |snapshot| {
        use tauri::Emitter;
        let _ = emitter_app.emit("network://status", snapshot);
    }));
    let event_app = app.clone();
    let _ = EVENT_UI_EMITTER.set(Box::new(move |entry| {
        use tauri::Emitter;
        let _ = event_app.emit("network://event", entry);
    }));

    // Wire the registry's mutation events to the WS broadcast + local UI.
    invoke_registry::set_event_sink(Box::new(|event_type, data| {
        server_api::broadcast_event(event_type, data);
    }));

    let db_handle = rt.db.get().unwrap();
    let cfg = NetConfig::load(db_handle);
    let term: u64 = settings_service::get_all_settings(db_handle)
        .ok()
        .and_then(|s| s.get("net_term").and_then(|v| v.parse().ok()))
        .unwrap_or(0);
    let term = term.max(1);
    rt.term.store(term, Ordering::Relaxed);
    rt.highest_seen_term.store(term, Ordering::Relaxed);
    if !cfg.device_token.is_empty() {
        *rt.device_token.lock().unwrap() = Some(cfg.device_token);
    }

    // mDNS handle (best-effort — a blocked mDNS stack degrades to UDP).
    match discovery::MdnsHandle::new() {
        Ok(h) => {
            *rt.mdns.lock().unwrap() = Some(h);
        }
        Err(e) => println!("[net] mDNS unavailable: {} (UDP broadcast only)", e),
    }

    // UDP listener: feeds peers and answers probes with our announce.
    let get_packet: Arc<dyn Fn() -> DiscoveryPacket + Send + Sync> =
        Arc::new(build_announce_packet);
    let sink: discovery::PacketSink = Arc::new(|pkt, ip| on_packet(pkt, ip));
    let _ = discovery::spawn_udp_listener(cfg.node_id.clone(), Arc::clone(&get_packet), sink);

    // Periodic announcer.
    discovery::spawn_announcer(Arc::new(build_announce_packet), Duration::from_secs(5));

    // Manager loop (wake channel allows immediate re-evaluation on changes).
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    *rt.wake.lock().unwrap() = Some(tx);
    std::thread::Builder::new()
        .name("net-manager".into())
        .spawn(move || manager_loop(rx))
        .expect("spawn net manager");

    // Startup discovery burst (after a short settle delay).
    std::thread::Builder::new()
        .name("net-probe".into())
        .spawn(|| {
            std::thread::sleep(Duration::from_millis(1500));
            {
                let sink: discovery::PacketSink = Arc::new(|pkt, ip| on_packet(pkt, ip));
                let cfg = current_config();
            let targets = probe_unicast_targets(&cfg);
            discovery::send_probe_burst(&build_announce_packet(), &targets, &sink);
            }
        })
        .ok();

    println!(
        "[net] LAN shop network initialized — node {} role {}",
        cfg.node_id, cfg.role
    );
}

fn manager_loop(rx: std::sync::mpsc::Receiver<()>) {
    loop {
        manager_tick();
        // Wake early when something requests an immediate re-evaluation.
        let _ = rx.recv_timeout(Duration::from_secs(4));
    }
}

// ---------------------------------------------------------------------------
// Peers & announce
// ---------------------------------------------------------------------------

fn on_packet(pkt: DiscoveryPacket, ip: String) {
    let Some(rt) = net_opt() else { return };
    let peer = Peer {
        node_id: pkt.node_id.clone(),
        pc_name: pkt.pc_name.clone(),
        role_pref: pkt.role_pref.clone(),
        is_coordinator: pkt.is_coordinator,
        shop_id: pkt.shop_id.clone(),
        shop_name: pkt.shop_name.clone(),
        term: pkt.term,
        http_port: pkt.http_port,
        // Prefer the packet's SOURCE address — it physically reached us,
        // so it is a working route to that node. Advertised lan_ips stay
        // available via the announce (may lead with virtual adapters).
        ip: if ip.is_empty() { pkt.lan_ips.first().cloned().unwrap_or_default() } else { ip },
        app_version: pkt.app_version.clone(),
        data_rows: pkt.data_rows,
        last_seen_ms: now_secs() * 1000,
    };
    let changed = {
        let mut peers = rt.peers.lock().unwrap();
        let same_as_before = peers
            .get(&peer.node_id)
            .map(|p| {
                p.is_coordinator == peer.is_coordinator
                    && p.shop_id == peer.shop_id
                    && p.term == peer.term
                    && p.role_pref == peer.role_pref
            })
            .unwrap_or(false);
        peers.insert(peer.node_id.clone(), peer.clone());
        !same_as_before
    };
    // Split-brain detection: another coordinator for MY shop appeared →
    // the lower-candidacy node stands down deterministically.
    let cfg = current_config();
    if peer.is_coordinator && !cfg.shop_id.is_empty() && peer.shop_id == cfg.shop_id {
        let serving = rt.mode.lock().unwrap().serving();
        if serving && peer.node_id != cfg.node_id {
            let self_peer = self_peer_for_election(&cfg);
            if election::should_stand_down(&self_peer, &peer) {
                log_net_event(
                    "coordinator_conflict",
                    json!({ "rival": peer.pc_name, "action": "stand_down" }),
                );
                stand_down_and_follow(&peer);
            }
        }
    }
    if changed {
        emit_status();
    }
}

fn current_config() -> NetConfig {
    match net_opt() {
        Some(rt) => rt
            .db
            .get()
            .map(NetConfig::load)
            .expect("network runtime has no db"),
        None => NetConfig {
            enabled: false,
            role: "automatic".into(),
            node_id: String::new(),
            pc_name: String::new(),
            shop_id: String::new(),
            shop_name: String::new(),
            manual_server: String::new(),
            autodiscovery: false,
            autoreconnect: false,
            device_token: String::new(),
            device_token_shop: String::new(),
            last_server: String::new(),
            blocked_nodes: vec![],
        },
    }
}

fn self_peer_for_election(cfg: &NetConfig) -> Peer {
    let serving = net_opt().map(|r| r.mode.lock().unwrap().serving()).unwrap_or(false);
    Peer {
        node_id: cfg.node_id.clone(),
        pc_name: cfg.pc_name.clone(),
        role_pref: cfg.role.clone(),
        is_coordinator: serving,
        shop_id: cfg.shop_id.clone(),
        shop_name: cfg.shop_name.clone(),
        term: current_term(),
        http_port: local_http_port(),
        ip: lan_ips().first().cloned().unwrap_or_default(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        data_rows: if serving { data_rows() } else { 0 },
        last_seen_ms: now_secs() * 1000,
    }
}

fn build_announce_packet() -> DiscoveryPacket {
    let cfg = current_config();
    let serving = net_opt().map(|r| r.mode.lock().unwrap().serving()).unwrap_or(false);
    let mut p = DiscoveryPacket::new(cfg.node_id.clone(), cfg.pc_name.clone());
    p.role_pref = cfg.role.clone();
    p.is_coordinator = serving;
    p.shop_id = cfg.shop_id.clone();
    p.shop_name = cfg.shop_name.clone();
    p.term = current_term();
    p.http_port = local_http_port();
    p.lan_ips = lan_ips();
    p.app_version = env!("CARGO_PKG_VERSION").to_string();
    p.data_rows = if serving { data_rows() } else { 0 };
    p
}

fn local_http_port() -> u16 {
    crate::server::configured_port_public()
}

fn lan_ips() -> Vec<String> {
    let Some(rt) = net_opt() else {
        return crate::server::lan_ip_addresses_public();
    };
    {
        let cache = rt.lan_ips_cache.lock().unwrap();
        if let Some((at, ips)) = cache.clone() {
            if at.elapsed() < Duration::from_secs(60) {
                return ips;
            }
        }
    }
    let ips = crate::server::lan_ip_addresses_public();
    *rt.lan_ips_cache.lock().unwrap() = Some((Instant::now(), ips.clone()));
    ips
}

/// Rough size of the local shop data (election hint: richer DB wins).
fn data_rows() -> u64 {
    let Some(rt) = net_opt() else { return 0 };
    {
        let cache = rt.data_rows_cache.lock().unwrap();
        if let Some((at, n)) = *cache {
            if at.elapsed() < Duration::from_secs(60) {
                return n;
            }
        }
    }
    let n = rt
        .db
        .get()
        .map(|db| {
            let conn = db.conn.lock().unwrap();
            conn.query_row(
                "SELECT (SELECT COUNT(*) FROM sales) + (SELECT COUNT(*) FROM products) + (SELECT COUNT(*) FROM customers)",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
        })
        .unwrap_or(0) as u64;
    *rt.data_rows_cache.lock().unwrap() = Some((Instant::now(), n));
    n
}

// ---------------------------------------------------------------------------
// Manager state machine
// ---------------------------------------------------------------------------

fn set_mode(mode: Mode) -> bool {
    let Some(rt) = net_opt() else { return false };
    let mut m = rt.mode.lock().unwrap();
    if *m != mode {
        *m = mode;
        drop(m);
        update_advertising();
        emit_status();
        true
    } else {
        false
    }
}

/// Start/stop mDNS advertisement + browse per current mode.
fn update_advertising() {
    let Some(rt) = net_opt() else { return };
    let serving = rt.mode.lock().unwrap().serving();
    let mdns_guard = rt.mdns.lock().unwrap();
    let Some(mdns) = mdns_guard.as_ref() else { return };
    if serving {
        mdns.advertise(&build_announce_packet());
        mdns.stop_browse();
    } else {
        mdns.stop_advertise();
        let sink: discovery::PacketSink = Arc::new(|pkt, ip| on_packet(pkt, ip));
        mdns.browse(sink);
    }
}

fn manager_tick() {
    let cfg = current_config();
    let Some(rt) = net_opt() else { return };

    if !cfg.enabled {
        set_mode(Mode::Disabled);
        return;
    }

    // Prune peers we have not heard from in 15s (heartbeat window).
    {
        let cutoff = (now_secs() - 15) * 1000;
        rt.peers.lock().unwrap().retain(|_, p| p.last_seen_ms > cutoff);
    }

    match cfg.role.as_str() {
        "server" => tick_server(&cfg),
        "client" => tick_client(&cfg),
        _ => tick_automatic(&cfg),
    }
}

// --- explicit SERVER --------------------------------------------------------

fn tick_server(cfg: &NetConfig) {
    set_mode(Mode::Server);
    *net().coordinator.lock().unwrap() = Some(self_peer_for_election(cfg));
    *net().server_base.lock().unwrap() = None;
}

/// Pure membership rule (spec §28): a node that has ALREADY joined a shop
/// (device token bound to that shop) must never silently switch to a
/// different one. Fresh installs (no prior join) are free to adopt any
/// shop — their auto-minted shop id is just a placeholder.
fn membership_blocks_switch(cfg: &NetConfig, server_shop: &str) -> bool {
    let was_member = !cfg.shop_id.is_empty() && cfg.device_token_shop == cfg.shop_id;
    was_member && server_shop != cfg.shop_id
}

// --- explicit CLIENT --------------------------------------------------------

fn tick_client(cfg: &NetConfig) {
    // Target priority: manual override → LAST-KNOWN SERVER → discovery.
    // The last-server memory makes reconnection immune to discovery
    // problems (broadcast filtering, mDNS blocks...). Discovery is never
    // shop-filtered (spec §9); a fresh client ADOPTS the server's shop at
    // join — its own auto-minted id is just a placeholder.
    let target = if !cfg.manual_server.is_empty() {
        Some(normalize_base(&cfg.manual_server))
    } else if !cfg.last_server.is_empty() {
        Some(cfg.last_server.clone())
    } else {
        best_coordinator_base(None)
    };

    let Some(base) = target else {
        let had = net().server_base.lock().unwrap().take();
        if had.is_some() {
            set_mode(Mode::Reconnecting);
        } else {
            set_mode(Mode::Searching);
            if cfg.autodiscovery {
                {
                let sink: discovery::PacketSink = Arc::new(|pkt, ip| on_packet(pkt, ip));
                let cfg = current_config();
            let targets = probe_unicast_targets(&cfg);
            discovery::send_probe_burst(&build_announce_packet(), &targets, &sink);
            }
            }
        }
        return;
    };

    match client::health_check(&base) {
        Some(health) => {
            let shop = health
                .get("shop_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // The manual-override path may point at a server of a DIFFERENT
            // shop (admin deliberately moved us); adoption happens at join.
            // Only an EXPLICITLY configured member (a prior join recorded
            // this exact shop) refuses a different shop.
            if membership_blocks_switch(cfg, &shop) {
                // We previously joined THIS shop; the reachable server now
                // serves a different one — stay offline instead of silently
                // switching membership (spec §28).
                if set_mode(Mode::Offline) {
                    log_net_event(
                        "server_changed",
                        json!({ "detail": "configured server now serves a different shop" }),
                    );
                }
                return;
            }
            connect_client(&base, &health, cfg, &shop);
        }
        None => {
            if net().server_base.lock().unwrap().is_some() {
                set_mode(Mode::Reconnecting);
            } else {
                set_mode(Mode::Searching);
            }
            // The old IP may be dead: discovery finds the new one without
            // user action (spec: IP change must not need reconfiguration).
            if cfg.autodiscovery && cfg.manual_server.is_empty() {
                {
                let sink: discovery::PacketSink = Arc::new(|pkt, ip| on_packet(pkt, ip));
                let cfg = current_config();
            let targets = probe_unicast_targets(&cfg);
            discovery::send_probe_burst(&build_announce_packet(), &targets, &sink);
            }
            }
        }
    }
}

/// Validate our device token against the LIVE server: a restart wipes its
/// in-memory registry, so a cached token can be stale even though the join
/// once succeeded. Returns true when the server recognizes the device.
fn device_token_valid(base: &str, token: &str) -> bool {
    let url = format!("{}/api/v1/network/status", base.trim_end_matches('/'));
    let Ok(resp) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .and_then(|c| c.get(&url).header("Authorization", format!("Bearer {}", token)).send())
    else {
        return false;
    };
    if !resp.status().is_success() {
        return false;
    }
    let Ok(v) = resp.json::<Value>() else { return false };
    v.get("ok").and_then(|o| o.as_bool()).unwrap_or(false)
}

/// Join (if needed) and stay connected to a verified server.
fn connect_client(base: &str, health: &Value, cfg: &NetConfig, shop_id: &str) {
    let server_term = health.get("term").and_then(|v| v.as_u64()).unwrap_or(0);
    note_server_term(server_term);

    // Ensure a device token bound to THIS shop — and VALID on the live
    // server (restarts wipe its registry; stale tokens are re-joined).
    let mut have_token = net().device_token.lock().unwrap().clone();
    let token_ok = match &have_token {
        Some(t) => cfg.device_token_shop == shop_id && device_token_valid(base, t),
        None => false,
    };
    if !token_ok {
        match client::join_server(base, &build_announce_packet(), shop_id, true) {
            Ok(v) => {
                if let Some(t) = v.get("device_token").and_then(|x| x.as_str()) {
                    have_token = Some(t.to_string());
                    *net().device_token.lock().unwrap() = have_token.clone();
                    let db = net().db.get().unwrap();
                    NetConfig::save_field(db, "net_device_token", t);
                    NetConfig::save_field(db, "net_device_token_shop", shop_id);
                    // Adoption: remember the shop we joined (fresh install,
                    // or deliberately moved by an admin action).
                    NetConfig::save_field(db, "net_shop_id", shop_id);
                    let name = v.get("shop_name").and_then(|x| x.as_str()).unwrap_or("");
                    if !name.is_empty() {
                        NetConfig::save_field(db, "net_shop_name", name);
                    }
                    log_net_event("shop_adopted", json!({ "shop_id": shop_id }));
                }
            }
            Err(e) => {
                if set_mode(Mode::Offline) {
                    log_net_event("join_failed", json!({ "error": e }));
                }
                return;
            }
        }
    }

    let Some(token) = have_token else { return };
    *net().server_base.lock().unwrap() = Some(base.to_string());
    // A user token from a previous session can be stale (a server restart
    // wipes its in-memory token map). Verify it with one cheap User-level
    // call; when rejected, drop it — the UI sees logged_in flip false and
    // sends the user back to the login screen instead of failing every
    // operation with "Log in before running shop operations".
    if let Some(ut) = net().user_token.lock().unwrap().clone() {
        if !user_token_valid(base, &ut) {
            clear_user_token();
            log_net_event("user_token_invalid", json!({ "action": "logout_required" }));
        }
    }
    // Remember the coordinator identity for the UI (its announce may never
    // arrive on broadcast-filtered LANs — health already told us who).
    {
        let server_peer = Peer {
            node_id: health.get("server_id").and_then(|v| v.as_str()).unwrap_or("server").to_string(),
            pc_name: health.get("server_pc").and_then(|v| v.as_str()).unwrap_or("Shop Server").to_string(),
            role_pref: "server".into(),
            is_coordinator: true,
            shop_id: shop_id.to_string(),
            shop_name: health.get("shop_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            term: server_term,
            http_port: base.rsplit(':').next().and_then(|p| p.parse().ok()).unwrap_or(8080),
            ip: base.trim_start_matches("http://").split(':').next().unwrap_or("").to_string(),
            app_version: health.get("app_version").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            data_rows: 0,
            last_seen_ms: now_secs() * 1000,
        };
        let mut c = net().coordinator.lock().unwrap();
        if c.as_ref().map(|cur| cur.node_id != server_peer.node_id).unwrap_or(true) {
            *c = Some(server_peer);
        }
    }
    // Remember the server for instant reconnection on next startups.
    if let Some(db) = net_opt().and_then(|r| r.db.get()) {
        if cfg.last_server != base {
            NetConfig::save_field(db, "net_last_server", base);
        }
    }
    ensure_ws_listener(base.to_string(), token);
    if set_mode(Mode::Connected) {
        log_net_event("connected_to_server", json!({ "server": base, "term": server_term }));
    }
}

// --- AUTOMATIC --------------------------------------------------------------

fn tick_automatic(cfg: &NetConfig) {
    let rt = net();
    let self_peer = self_peer_for_election(cfg);

    // Currently following another coordinator? Verify it first.
    let following = rt.coordinator.lock().unwrap().clone();
    if let Some(coord) = following {
        if coord.node_id != cfg.node_id {
            match coord_base(&coord).and_then(|b| client::health_check(&b).map(|h| (b, h))) {
                Some((base, health)) => {
                    let shop = health
                        .get("shop_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(coord.shop_id.as_str())
                        .to_string();
                    *rt.coordinator.lock().unwrap() = Some(coord.clone());
                    connect_client(&base, &health, cfg, &shop);
                    return;
                }
                None => {
                    // Coordinator lost → re-election happens below; term is
                    // bumped when a node takes over.
                    log_net_event("coordinator_lost", json!({ "coordinator": coord.pc_name }));
                    *rt.coordinator.lock().unwrap() = None;
                    *rt.server_base.lock().unwrap() = None;
                    stop_ws_listener();
                    set_mode(Mode::Reconnecting);
                }
            }
        }
    }

    // Alive peers. Discovery is never shop-filtered for unaffiliated nodes
    // (two fresh installs must find each other); a node that JOINED a shop
    // stays inside it (device_token_shop is the membership record).
    let affiliated = !cfg.shop_id.is_empty() && cfg.device_token_shop == cfg.shop_id;
    let alive: Vec<Peer> = {
        let peers = rt.peers.lock().unwrap();
        peers
            .values()
            .filter(|p| !affiliated || p.shop_id == cfg.shop_id)
            .cloned()
            .collect()
    };

    match election::resolve_role(&self_peer, &alive) {
        Resolution::Standalone | Resolution::Coordinate(_) => {
            let was_following = rt.coordinator.lock().unwrap().as_ref().map(|c| c.node_id.clone())
                .map(|id| id != cfg.node_id)
                .unwrap_or(false);
            if was_following {
                take_over_term(); // old coordinator disappeared → takeover
            }
            *rt.coordinator.lock().unwrap() = Some(self_peer);
            *rt.server_base.lock().unwrap() = None;
            stop_ws_listener();
            if cfg.shop_id.is_empty() {
                create_shop_record("My Shop / متجري");
            }
            set_mode(Mode::Standalone);
        }
        Resolution::Follow(c) => {
            // Unaffiliated node adopting a shop: remember it.
            if cfg.shop_id.is_empty() && !c.shop_id.is_empty() {
                let db = rt.db.get().unwrap();
                NetConfig::save_field(db, "net_shop_id", &c.shop_id);
                if !c.shop_name.is_empty() {
                    NetConfig::save_field(db, "net_shop_name", &c.shop_name);
                }
                log_net_event("shop_adopted", json!({ "shop": c.shop_name, "coordinator": c.pc_name }));
            }
            if let Some(base) = coord_base(&c) {
                if let Some(health) = client::health_check(&base) {
                    let shop = health
                        .get("shop_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(c.shop_id.as_str())
                        .to_string();
                    *rt.coordinator.lock().unwrap() = Some(c);
                    connect_client(&base, &health, cfg, &shop);
                    return;
                }
            }
            // Announce present but not reachable yet: keep searching.
            if !matches!(*rt.mode.lock().unwrap(), Mode::Connected) {
                set_mode(Mode::Searching);
            }
        }
    }
}

/// Deterministic stand-down: adopt the rival's shop and follow it.
fn stand_down_and_follow(rival: &Peer) {
    if let Some(db) = net_opt().and_then(|r| r.db.get()) {
        NetConfig::save_field(db, "net_shop_id", &rival.shop_id);
        if !rival.shop_name.is_empty() {
            NetConfig::save_field(db, "net_shop_name", &rival.shop_name);
        }
    }
    note_server_term(rival.term);
    if let Some(rt) = net_opt() {
        *rt.coordinator.lock().unwrap() = Some(rival.clone());
    }
    wake_manager();
}

fn take_over_term() {
    let Some(rt) = net_opt() else { return };
    let next = election::next_term(
        rt.highest_seen_term
            .load(Ordering::Relaxed)
            .max(rt.term.load(Ordering::Relaxed)),
    );
    rt.term.store(next, Ordering::Relaxed);
    rt.highest_seen_term.store(next, Ordering::Relaxed);
    if let Some(db) = rt.db.get() {
        NetConfig::save_field(db, "net_term", &next.to_string());
    }
    log_net_event("coordinator_changed", json!({ "new_term": next }));
}

fn note_server_term(term: u64) {
    let Some(rt) = net_opt() else { return };
    let h = rt.highest_seen_term.load(Ordering::Relaxed);
    if term > h {
        rt.highest_seen_term.store(term, Ordering::Relaxed);
    }
    // Followers adopt the server's term so any later takeover starts
    // strictly above it (stale-leadership protection).
    let t = rt.term.load(Ordering::Relaxed);
    if term > t {
        rt.term.store(term, Ordering::Relaxed);
        if let Some(db) = rt.db.get() {
            NetConfig::save_field(db, "net_term", &term.to_string());
        }
    }
}

fn create_shop_record(name: &str) {
    let Some(rt) = net_opt() else { return };
    let Some(db) = rt.db.get() else { return };
    let cfg = current_config();
    if cfg.shop_id.is_empty() {
        NetConfig::save_field(db, "net_shop_id", &identity::generate_shop_id());
    }
    if cfg.shop_name.is_empty() {
        NetConfig::save_field(db, "net_shop_name", name);
    }
    log_net_event("shop_created", json!({ "shop": name }));
}

fn coord_base(peer: &Peer) -> Option<String> {
    if peer.ip.is_empty() {
        return None;
    }
    Some(format!("http://{}:{}", peer.ip, peer.http_port))
}

fn normalize_base(s: &str) -> String {
    let s = s.trim().trim_end_matches('/');
    if s.starts_with("http://") || s.starts_with("https://") {
        s.to_string()
    } else if s.contains(':') {
        format!("http://{}", s)
    } else {
        format!("http://{}:{}", s, local_http_port())
    }
}

/// Public alias for the commands module.
pub fn normalize_base_pub(s: &str) -> String {
    normalize_base(s)
}

/// Build the announce packet for an explicit config (manual join flow).
pub fn announce_packet_for(cfg: &NetConfig) -> DiscoveryPacket {
    let serving = net_opt().map(|r| r.mode.lock().unwrap().serving()).unwrap_or(false);
    let mut p = DiscoveryPacket::new(cfg.node_id.clone(), cfg.pc_name.clone());
    p.role_pref = cfg.role.clone();
    p.is_coordinator = serving;
    p.shop_id = cfg.shop_id.clone();
    p.shop_name = cfg.shop_name.clone();
    p.term = current_term();
    p.http_port = local_http_port();
    p.lan_ips = lan_ips();
    p.app_version = env!("CARGO_PKG_VERSION").to_string();
    p.data_rows = if serving { data_rows() } else { 0 };
    p
}

/// The device token issued by the shop server (persisted locally).
pub fn device_token_pub() -> Option<String> {
    net_opt()
        .and_then(|r| r.device_token.lock().unwrap().clone())
        .filter(|t| !t.is_empty())
}

/// Active API token: the logged-in user token, else the device token.
pub fn active_token_pub() -> Option<String> {
    if let Some(t) = net_opt().and_then(|r| r.user_token.lock().unwrap().clone()) {
        if !t.is_empty() {
            return Some(t);
        }
    }
    device_token_pub()
}

fn user_token_valid(base: &str, token: &str) -> bool {
    client::invoke_blocking(base, token, "get_units", &json!({}))
        .map(|_| true)
        .unwrap_or(false)
}

pub fn store_user_token(token: &str) {
    if let Some(rt) = net_opt() {
        *rt.user_token.lock().unwrap() = Some(token.to_string());
    }
    emit_status();
}

pub fn clear_user_token() {
    if let Some(rt) = net_opt() {
        *rt.user_token.lock().unwrap() = None;
    }
    emit_status();
}

fn best_coordinator_base(shop_filter: Option<&str>) -> Option<String> {
    let peers = net().peers.lock().unwrap();
    let mut best: Option<&Peer> = None;
    for p in peers.values() {
        if !p.is_coordinator {
            continue;
        }
        if let Some(shop) = shop_filter {
            if !shop.is_empty() && p.shop_id != shop {
                continue;
            }
        }
        if best.map(|b| p.candidacy() > b.candidacy()).unwrap_or(true) {
            best = Some(p);
        }
    }
    best.and_then(coord_base)
}


/// Direct-probe targets: the last-known server's IP + every peer address we
/// ever saw (persisted last_server wins). These are addresses unicast is
/// PROVEN to reach when broadcast is filtered.
fn probe_unicast_targets(cfg: &NetConfig) -> Vec<String> {
    let mut targets: Vec<String> = Vec::new();
    let push_host = |t: &mut Vec<String>, addr: &str| {
        let host = addr
            .trim()
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if !host.is_empty() && host.parse::<std::net::Ipv4Addr>().is_ok() && !t.contains(&host) {
            t.push(host);
        }
    };
    push_host(&mut targets, &cfg.last_server);
    push_host(&mut targets, &cfg.manual_server);
    if let Some(rt) = net_opt() {
        for p in rt.peers.lock().unwrap().values() {
            push_host(&mut targets, &p.ip);
        }
    }
    targets
}

fn wake_manager() {
    if let Some(tx) = net_opt().and_then(|r| r.wake.lock().unwrap().clone()) {
        let _ = tx.send(());
    }
}

// ---------------------------------------------------------------------------
// WebSocket events listener
// ---------------------------------------------------------------------------

fn ensure_ws_listener(base: String, token: String) {
    let Some(rt) = net_opt() else { return };
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    {
        let mut guard = rt.ws_stop.lock().unwrap();
        if let Some(old) = guard.take() {
            let _ = old.send(true);
        }
        *guard = Some(stop_tx);
    }
    std::thread::Builder::new()
        .name("net-ws-client".into())
        .spawn(move || {
            client::run_ws_listener(base, token, stop_rx, move |msg| match msg {
                client::WsMessage::Event(ev) => {
                    let entry = json!({
                        "type": ev.get("type").cloned().unwrap_or(Value::Null),
                        "data": ev.get("data").cloned().unwrap_or(Value::Null),
                        "ts": now_secs(),
                        "source": "server",
                    });
                    {
                        let mut events = net().events.lock().unwrap();
                        events.push_front(entry.clone());
                        events.truncate(50);
                    }
                    *net().last_event.lock().unwrap() = Some(entry.clone());
                    emit_net_event(entry);
                }
                client::WsMessage::Term(t) => note_server_term(t),
            })
        })
        .ok();
}

fn stop_ws_listener() {
    let Some(rt) = net_opt() else { return };
    if let Some(tx) = rt.ws_stop.lock().unwrap().take() {
        let _ = tx.send(true);
    }
}

// ---------------------------------------------------------------------------
// Public API used by server_api + Tauri commands
// ---------------------------------------------------------------------------

/// Term of the current coordinator (this node when serving).
pub fn current_term() -> u64 {
    net_opt().map(|r| r.term.load(Ordering::Relaxed)).unwrap_or(0)
}

/// (node_id, pc_name, shop_id, shop_name, is_coordinator) for /api/v1.
pub fn server_shop_info() -> (String, String, String, String, bool) {
    let cfg = current_config();
    let serving = net_opt().map(|r| r.mode.lock().unwrap().serving()).unwrap_or(false);
    (
        cfg.node_id,
        cfg.pc_name,
        cfg.shop_id,
        if cfg.shop_name.is_empty() { "My Shop".to_string() } else { cfg.shop_name },
        serving,
    )
}

/// This PC's terminal name for stamping records (fall back to hostname).
/// Opens a RAW connection on purpose — this runs from inside database
/// migrations (row backfill), so going through DbState::new() would recurse
/// into the very migration that calls it (stack overflow).
pub fn terminal_name_for_this_pc() -> String {
    let path = crate::database::get_database_path();
    if let Ok(conn) = rusqlite::Connection::open(&path) {
        if let Ok(name) = conn.query_row(
            "SELECT value FROM app_settings WHERE key = 'net_pc_name'",
            [],
            |r| r.get::<_, String>(0),
        ) {
            let name = name.trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".to_string())
}

thread_local! {
    /// Per-call terminal identity: the API layer sets this for the duration
    /// of a networked request so every INSERT stamps the CALLING terminal's
    /// PC name; locally it stays unset (records stamp this PC).
    static CALLER_TERMINAL: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Record the calling terminal for the current network request (server API).
pub fn set_caller_terminal(name: &str) {
    CALLER_TERMINAL.with(|c| *c.borrow_mut() = Some(name.to_string()));
}

/// Clear after the request (never leaks into the next local operation).
pub fn clear_caller_terminal() {
    CALLER_TERMINAL.with(|c| *c.borrow_mut() = None);
}

/// The terminal name to STAMP on a record being created right now.
pub fn current_stamp_terminal() -> String {
    if let Some(name) = CALLER_TERMINAL.with(|c| c.borrow().clone()) {
        return name;
    }
    terminal_name_for_this_pc()
}

/// The terminal name to STAMP on a record being created: on the server,
/// the calling terminal's PC name (from its device token); locally, this PC.
pub fn stamping_terminal(caller_node: &str) -> String {
    if caller_node.is_empty() {
        return terminal_name_for_this_pc();
    }
    // The registry keeps the caller's PC name from its join.
    for d in server_api::devices_snapshot() {
        if d.get("node_id").and_then(|v| v.as_str()) == Some(caller_node) {
            if let Some(name) = d.get("pc_name").and_then(|v| v.as_str()) {
                if !name.trim().is_empty() {
                    return name.to_string();
                }
            }
        }
    }
    terminal_name_for_this_pc()
}

pub fn is_node_blocked(node_id: &str) -> bool {
    let cfg = current_config();
    cfg.blocked_nodes.iter().any(|n| n == node_id)
}

/// Is this PC the shop authority (explicit Server or Standalone automatic
/// node)? Cloud sync may only run on such a node — LAN client terminals
/// forward their business ops here, so exactly one cloud writer exists.
pub fn is_server_or_standalone() -> bool {
    match net_opt() {
        Some(rt) => rt.mode.lock().unwrap().serving(),
        None => false,
    }
}

/// Full status snapshot for the UI.
pub fn status_snapshot() -> Value {
    let cfg = current_config();
    let Some(rt) = net_opt() else {
        return json!({ "enabled": false, "mode": "disabled", "role": cfg.role });
    };
    let mode = rt.mode.lock().unwrap().clone();
    let coordinator = rt.coordinator.lock().unwrap().clone();
    let base = rt.server_base.lock().unwrap().clone();
    let serving = mode.serving();
    let devices = if serving { Value::Array(server_api::devices_snapshot()) } else { Value::Null };
    let events: Vec<Value> = rt.events.lock().unwrap().iter().take(20).cloned().collect();
    json!({
        "enabled": cfg.enabled,
        "role": cfg.role,
        "mode": mode.as_str(),
        "serving": serving,
        "node_id": cfg.node_id,
        "pc_name": cfg.pc_name,
        "shop_id": cfg.shop_id,
        "shop_name": if cfg.shop_name.is_empty() { "My Shop".to_string() } else { cfg.shop_name.clone() },
        "coordinator": coordinator.map(|c| json!({"node_id": c.node_id, "pc_name": c.pc_name})),
        "server_url": base,
        "term": rt.term.load(Ordering::Relaxed),
        "devices": devices,
        "devices_count": if serving { server_api::devices_snapshot().len() } else { 0 },
        "lan_ips": lan_ips(),
        "port": local_http_port(),
        "autodiscovery": cfg.autodiscovery,
        "autoreconnect": cfg.autoreconnect,
        "manual_server": cfg.manual_server,
        "last_server": cfg.last_server,
        "discovery_diag": discovery::diagnostics_snapshot(),
        "logged_in": rt.user_token.lock().unwrap().is_some(),
        "last_event": rt.last_event.lock().unwrap().clone(),
        "events": events,
        "known_peers": rt.peers.lock().unwrap().values().map(|p| json!({
            "node_id": p.node_id, "pc_name": p.pc_name, "role": p.role_pref,
            "is_coordinator": p.is_coordinator, "shop_name": p.shop_name,
            "shop_id": p.shop_id, "ip": p.ip, "port": p.http_port, "term": p.term,
        })).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// Settings merge helpers (client mode)
// ---------------------------------------------------------------------------

/// `get_all_settings` over the network: server (shop) settings overlaid with
/// this PC's local-only settings (local wins for its own keys).
pub fn merge_settings_with_local(server: Value, db: &DbState) -> Value {
    let mut out = match server {
        Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    if let Ok(local) = settings_service::get_all_settings(db) {
        for (k, v) in &local {
            if is_local_only_setting(k) && !v.is_empty() {
                out.insert(k.clone(), Value::String(v.clone()));
            }
        }
    }
    Value::Object(out)
}

// ---------------------------------------------------------------------------
// Offline login-screen cache
// ---------------------------------------------------------------------------

pub fn cache_users(payload: &Value) {
    let Some(rt) = net_opt() else { return };
    if let Some(db) = rt.db.get() {
        if let Ok(txt) = serde_json::to_string(payload) {
            NetConfig::save_field(db, "net_cached_users", &txt);
        }
    }
    *rt.users_cache.lock().unwrap() = Some(payload.clone());
}

pub fn cached_users() -> Option<Value> {
    let rt = net_opt()?;
    if let Some(v) = rt.users_cache.lock().unwrap().clone() {
        return Some(v);
    }
    let db = rt.db.get()?;
    let raw = settings_service::get_all_settings(db).ok()?.get("net_cached_users").cloned()?;
    serde_json::from_str(&raw).ok()
}

// ---------------------------------------------------------------------------
// Client-mode invoke forwarding (Rust-side IPC interception)
// ---------------------------------------------------------------------------

/// True when the given command should be forwarded to the shop server:
/// it is on the whitelisted business surface AND this PC is a connected
/// client. Everything else (hardware, backups, licensing, network commands,
/// plugin commands) runs locally by design.
pub fn should_forward_ipc(command: &str) -> bool {
    let Some(rt) = net_opt() else { return false };
    // `login` is the one command that must forward even though it is not a
    // registry business op: on a client terminal the REAL user accounts
    // live in the server's database. Forwarding it mints a server-side user
    // token; validating against the local (stale) database would leave the
    // terminal "logged in" locally with no shop permissions — the exact
    // ghost-login bug seen in the field.
    if command == "login" {
        if matches!(*rt.mode.lock().unwrap(), Mode::Connected) {
            return true;
        }
        // A CLIENT-role terminal never falls back to a local login — the
        // authoritative accounts live on the server. When disconnected the
        // interception returns the clear "shop server unreachable" error
        // (spec §14: an explicit client stays offline, never authoritative).
        let cfg = current_config();
        return cfg.enabled && cfg.role == "client";
    }
    if !invoke_registry::KNOWN_COMMANDS.contains(&command) {
        return false;
    }
    matches!(*rt.mode.lock().unwrap(), Mode::Connected)
}

/// Execute one whitelisted command on the shop server. Called by the
/// invoke-handler wrapper in lib.rs (and by the `network_forward` command).
pub async fn forward_ipc(command: String, payload_json: String) -> Result<Value, String> {
    let status = status_snapshot();
    let connected = status.get("mode").and_then(|m| m.as_str()) == Some("connected");
    let base = status
        .get("server_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if !connected || base.is_none() {
        if command == "get_active_users" {
            // Offline login screen: serve the last known user list.
            if let Some(cached) = cached_users() {
                return Ok(cached);
            }
            return Err("No TitaouPOS server was found on this network.".into());
        }
        if command == "login" || command == "login_with_rfid" {
            return Err(
                "Cannot reach the shop server. Wait for reconnection or switch this PC's network role."
                    .into(),
            );
        }
        return Err("Not connected to the TitaouPOS shop server. Shop data lives on the server PC — check the network status indicator.".into());
    }
    let base = base.unwrap();
    let args: Value = serde_json::from_str(&payload_json).unwrap_or(Value::Null);
    // `login` is forwarded before the registry whitelist (see
    // should_forward_ipc) — forward_command_core's "login" arm performs the
    // server-side authentication and stores the returned user token.
    forward_command_core(&base, &command, args).await
}

/// The full client-side forwarding pipeline: settings split (per-PC vs
/// shop-wide), login token minting, and the generic invoke envelope.
async fn forward_command_core(
    base: &str,
    command: &str,
    args: Value,
) -> Result<Value, String> {
    match command {
        "get_setting" => {
            let key = args
                .get("key")
                .and_then(|k| k.as_str())
                .unwrap_or("")
                .to_string();
            if is_local_only_setting(&key) {
                let all = net()
                    .db
                    .get()
                    .map(|db| settings_service::get_all_settings(db).unwrap_or_default())
                    .unwrap_or_default();
                return Ok(all
                    .get(&key)
                    .cloned()
                    .filter(|v| !v.is_empty())
                    .map(Value::String)
                    .unwrap_or(Value::Null));
            }
        }
        "set_setting" => {
            let key = args.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string();
            let value = args
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if is_local_only_setting(&key) {
                if let Some(db) = net().db.get() {
                    settings_service::set_setting(db, &key, &value)?;
                }
                return Ok(Value::Null);
            }
        }
        "set_multiple_settings" => {
            if let Some(settings) = args.get("settings").and_then(|s| s.as_object()) {
                let mut local = serde_json::Map::new();
                let mut remote = serde_json::Map::new();
                for (k, v) in settings {
                    if is_local_only_setting(k) {
                        local.insert(k.clone(), v.clone());
                    } else {
                        remote.insert(k.clone(), v.clone());
                    }
                }
                if !local.is_empty() {
                    if let Some(db) = net().db.get() {
                        let map: std::collections::HashMap<String, String> =
                            serde_json::from_value(Value::Object(local)).unwrap_or_default();
                        settings_service::set_multiple_settings(db, map)?;
                    }
                }
                if remote.is_empty() {
                    return Ok(Value::Null);
                }
                return forward_envelope(base, command, json!({ "settings": remote }))
                    .await
                    .map(|(r, _)| r);
            }
        }
        "login" => {
            let device_tok =
                device_token_pub().ok_or("This terminal is not registered on the shop server")?;
            let username = args
                .get("username")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let password = args
                .get("password")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let base_owned = base.to_string();
            let (result, user_token) =
                tokio::task::spawn_blocking(move || {
                    client::login_on_server(&base_owned, &device_tok, &username, &password)
                })
                .await
                .map_err(|e| e.to_string())?
                .map(|(user, tok)| (user, Some(tok)))?;
            if let Some(t) = user_token {
                store_user_token(&t);
            }
            return Ok(result);
        }
        _ => {}
    }

    let (result, auth_token) = forward_envelope(base, command, args).await?;
    if let Some(t) = auth_token {
        store_user_token(&t);
    }
    // Cache the user list for the offline login screen.
    if command == "get_active_users" {
        cache_users(&result);
    }
    Ok(result)
}

/// POST /api/v1/invoke/{command}; returns (result, optional auth_token for
/// login-style commands).
async fn forward_envelope(
    base: &str,
    command: &str,
    args: Value,
) -> Result<(Value, Option<String>), String> {
    // Prefer the user token (permission identity); fall back to the device
    // token for the pre-login surface.
    let token =
        active_token_pub().ok_or("This terminal is not registered on the shop server")?;
    let url = format!("{}/api/v1/invoke/{}", base.trim_end_matches('/'), command);
    static HTTP: OnceLock<reqwest::Client> = OnceLock::new();
    let client = HTTP.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .expect("forward http client")
    });
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&args)
        .send()
        .await
        .map_err(|_| {
            "Cannot reach the TitaouPOS shop server. Shop data lives on the server PC — check the LAN connection."
                .to_string()
        })?;
    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| format!("bad response: {}", e))?;
    if v.get("ok").and_then(|o| o.as_bool()) == Some(true) {
        let auth_token = v
            .get("auth_token")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
        Ok((v.get("result").cloned().unwrap_or(Value::Null), auth_token))
    } else {
        Err(v
            .get("error")
            .and_then(|e| e.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("server rejected '{}' (HTTP {})", command, status)))
    }
}

// ---------------------------------------------------------------------------
// Config-change wake
// ---------------------------------------------------------------------------

pub fn notify_config_changed() {
    wake_manager();
    update_advertising();
    emit_status();
}

// ---------------------------------------------------------------------------
// Test support: runtime without threads/AppHandle so live API tests can run
// the real server path in-process.
// ---------------------------------------------------------------------------

#[cfg(test)]
pub fn init_for_tests(db: DbState) {
    let runtime = NetRuntime {
        db: OnceLock::new(),
        mode: Mutex::new(Mode::Searching),
        peers: Mutex::new(HashMap::new()),
        coordinator: Mutex::new(None),
        server_base: Mutex::new(None),
        device_token: Mutex::new(None),
        user_token: Mutex::new(None),
        term: AtomicU64::new(1),
        highest_seen_term: AtomicU64::new(1),
        events: Mutex::new(VecDeque::new()),
        last_event: Mutex::new(None),
        mdns: Mutex::new(None),
        wake: Mutex::new(None),
        ws_stop: Mutex::new(None),
        data_rows_cache: Mutex::new(None),
        lan_ips_cache: Mutex::new(None),
        users_cache: Mutex::new(None),
    };
    let _ = NET.set(runtime);
    let rt = net();
    let _ = rt.db.set(db);
    set_mode(Mode::Server);
}

#[cfg(test)]
pub fn set_shop_for_tests(shop_id: &str, shop_name: &str) {
    let rt = net();
    let db = rt.db.get().unwrap();
    NetConfig::save_field(db, "net_node_id", "NODE-SRV-TEST");
    NetConfig::save_field(db, "net_pc_name", "POS-SRV-TEST");
    NetConfig::save_field(db, "net_role", "server");
    NetConfig::save_field(db, "net_shop_id", shop_id);
    NetConfig::save_field(db, "net_shop_name", shop_name);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_only_settings_cover_network_keys() {
        assert!(is_local_only_setting("net_role"));
        assert!(is_local_only_setting("scale_barcode_type"));
        assert!(is_local_only_setting("drawer_com_port"));
        assert!(!is_local_only_setting("shop_name_fr"));
        assert!(!is_local_only_setting("telegram_bot_token"));
    }

    #[test]
    fn normalize_base_variants() {
        assert_eq!(normalize_base("192.168.1.5:8080"), "http://192.168.1.5:8080");
        assert_eq!(normalize_base("http://192.168.1.5:8080"), "http://192.168.1.5:8080");
        assert_eq!(
            normalize_base("192.168.1.5"),
            format!("http://192.168.1.5:{}", local_http_port())
        );
    }

    #[test]
    fn fresh_installs_connect_despite_different_auto_shop_ids() {
        // The user's exact scenario: two fresh installs each auto-minted a
        // DIFFERENT shop id before seeing each other. The client must NOT
        // refuse the server's (different) shop — it adopts it at join.
        let fresh_client = NetConfig {
            enabled: true,
            role: "client".into(),
            node_id: "NODE-C".into(),
            pc_name: "PC-C".into(),
            shop_id: "SHOP-AUTO-C".into(),       // auto-minted placeholder
            shop_name: String::new(),
            manual_server: String::new(),
            autodiscovery: true,
            autoreconnect: true,
            device_token: String::new(),          // never joined anything
            device_token_shop: String::new(),
            last_server: String::new(),
            blocked_nodes: vec![],
        };
        assert!(
            !membership_blocks_switch(&fresh_client, "SHOP-SERVER"),
            "fresh client must be allowed to join the server's shop"
        );
        // Even with a stale device token from another shop (manually moved),
        // a non-member still adopts.
        let moved = NetConfig {
            device_token: "TPS-DEV-x".into(),
            device_token_shop: "SHOP-OLD".into(),
            shop_id: "SHOP-AUTO-C".into(),
            ..fresh_client.clone()
        };
        assert!(!membership_blocks_switch(&moved, "SHOP-SERVER"));

        // A TRUE member (token bound to THIS shop id) refuses a different shop.
        let member = NetConfig {
            device_token: "TPS-DEV-y".into(),
            device_token_shop: "SHOP-YES".into(),
            shop_id: "SHOP-YES".into(),
            ..fresh_client.clone()
        };
        assert!(membership_blocks_switch(&member, "SHOP-OTHER"));
        assert!(!membership_blocks_switch(&member, "SHOP-YES"));
    }

    #[test]
    fn mode_strings_and_serving() {
        assert_eq!(Mode::Connected.as_str(), "connected");
        assert!(Mode::Standalone.serving());
        assert!(Mode::Server.serving());
        assert!(!Mode::Connected.serving());
        assert!(!Mode::Offline.serving());
        assert!(!Mode::Disabled.serving());
    }
}
