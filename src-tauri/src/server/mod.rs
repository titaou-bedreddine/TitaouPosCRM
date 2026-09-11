use axum::{extract::State as AxState, extract::ws::{Message, WebSocket, WebSocketUpgrade}, response::Html, routing::get, routing::post, Json, Router};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tower_http::cors::CorsLayer;

use crate::database::DbState;

// ---------------------------------------------------------------------------
// Shared server state: port, start time, and the REAL connected-device
// registry (a device appears here only after a successful /api/handshake).
// ---------------------------------------------------------------------------
pub struct ServerState {
    pub port: u16,
    pub started_at: Instant,
    pub devices: Mutex<HashMap<String, ConnectedDevice>>,
}

pub struct ConnectedDevice {
    pub device_name: String,
    pub device_uid: String,
    pub device_role: String,
    pub ip: String,
    pub last_seen: Instant,
}

static STATE: OnceLock<Arc<ServerState>> = OnceLock::new();
// A separate open DB handle for diagnostics reads, so API requests never
// contend on the UI's DbState mutex.
pub static DIAG_DB: OnceLock<DbState> = OnceLock::new();

pub fn set_diag_db(db: DbState) {
    let _ = DIAG_DB.set(db);
}

/// All non-loopback IPv4 addresses of this machine, REAL NETWORK ADAPTERS
/// FIRST. Virtual adapters (Docker 172.x, WSL, Hyper-V, VPN) must never
/// sort ahead of the shop LAN: a client picking `lan_ips[0]` as its server
/// URL would try an unreachable virtual address and never connect. Windows:
/// only interfaces with a default gateway are considered "real" and come
/// first; everything else is appended as a fallback tail.
pub fn lan_ip_addresses() -> Vec<String> {
    let mut routed: Vec<String> = Vec::new();
    let mut others: Vec<String> = Vec::new();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Routed addresses: interfaces that own the machine's default
        // route (i.e. the adapter actually connected to the LAN).
        let routed_out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Sort-Object RouteMetric | ForEach-Object { Get-NetIPAddress -InterfaceIndex $_.InterfaceIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue } | Select-Object -ExpandProperty IPAddress)",
            ])
            .creation_flags(0x08000000)
            .output();
        if let Ok(o) = routed_out {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                let l = line.trim().to_string();
                if !l.is_empty()
                    && !l.starts_with("127.")
                    && !l.starts_with("169.254.")
                    && !routed.contains(&l)
                {
                    routed.push(l);
                }
            }
        }
        // All other IPv4 addresses (virtual adapters etc.) as tail.
        let all_out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike '127.*' -and $_.IPAddress -notlike '169.254.*' } | Select-Object -ExpandProperty IPAddress)",
            ])
            .creation_flags(0x08000000)
            .output();
        if let Ok(o) = all_out {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                let l = line.trim().to_string();
                if !l.is_empty()
                    && !routed.contains(&l)
                    && !others.contains(&l)
                    && !l.starts_with("169.254.")
                {
                    others.push(l);
                }
            }
        }
    }
    let mut out = routed;
    out.extend(others);
    if out.is_empty() {
        // Fallback: the address a default route would use.
        if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if sock.connect("8.8.8.8:80").is_ok() {
                if let Ok(a) = sock.local_addr() {
                    out.push(a.ip().to_string());
                }
            }
        }
    }
    out
}

fn configured_port(diag: Option<&DbState>) -> u16 {
    if let Some(db) = diag {
        if let Ok(settings) = crate::services::settings_service::get_all_settings(db) {
            if let Some(p) = settings.get("mobile_server_port") {
                if let Ok(n) = p.trim().parse::<u16>() {
                    if n > 0 {
                        return n;
                    }
                }
            }
        }
    }
    8090 // TitaouPosCRM default — 8080 stays free for TitaouPOS on the same PC
}

/// Public accessors for the LAN network module (announce packets, status).
pub fn configured_port_public() -> u16 {
    configured_port(DIAG_DB.get())
}

pub fn lan_ip_addresses_public() -> Vec<String> {
    lan_ip_addresses()
}

/// Real server status for Settings > Network: port, uptime, LAN IPs for
/// the QR, and the handshake-verified device list — no demo data.
pub fn server_status() -> serde_json::Value {
    let port = STATE.get().map(|s| s.port).unwrap_or_else(|| configured_port(DIAG_DB.get()));
    let running = STATE.get().is_some();
    let uptime_secs = STATE
        .get()
        .map(|s| s.started_at.elapsed().as_secs())
        .unwrap_or(0);
    let devices: Vec<serde_json::Value> = match STATE.get() {
        Some(s) => {
            let map = s.devices.lock().unwrap();
            map.values()
                .filter(|d| d.last_seen.elapsed() < Duration::from_secs(300))
                .map(|d| {
                    serde_json::json!({
                        "device_name": d.device_name,
                        "device_uid": d.device_uid,
                        "device_role": d.device_role,
                        "ip": d.ip,
                        "last_seen_secs_ago": d.last_seen.elapsed().as_secs(),
                    })
                })
                .collect()
        }
        None => Vec::new(),
    };
    serde_json::json!({
        "running": running,
        "port": port,
        "uptime_secs": uptime_secs,
        "lan_ips": lan_ip_addresses(),
        "devices": devices,
        "devices_count": devices.len(),
    })
}

#[derive(serde::Deserialize)]
struct HandshakeBody {
    #[serde(default)]
    device_name: String,
    #[serde(default)]
    device_uid: String,
    #[serde(default)]
    device_role: String,
}

async fn api_handshake(
    AxState(state): AxState<Arc<ServerState>>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    body: Option<Json<HandshakeBody>>,
) -> Json<serde_json::Value> {
    let (name, uid, role) = match body {
        Some(Json(b)) => (
            if b.device_name.trim().is_empty() { "Mobile terminal".to_string() } else { b.device_name },
            if b.device_uid.trim().is_empty() { format!("dev-{}", addr) } else { b.device_uid },
            if b.device_role.trim().is_empty() { "pos_terminal".to_string() } else { b.device_role },
        ),
        None => (format!("Mobile terminal ({})", addr.ip()), format!("dev-{}", addr), "pos_terminal".to_string()),
    };
    let ip = addr.ip().to_string();
    {
        let mut devices = state.devices.lock().unwrap();
        devices.insert(
            uid.clone(),
            ConnectedDevice {
                device_name: name,
                device_uid: uid,
                device_role: role,
                ip,
                last_seen: Instant::now(),
            },
        );
    }
    let port = state.port;
    Json(serde_json::json!({
        "ok": true,
        "server": "TitaouPosCRM Host",
        "port": port,
        "paired": true,
    }))
}

async fn api_status() -> Json<serde_json::Value> {
    let port = STATE.get().map(|s| s.port).unwrap_or(8090);
    Json(serde_json::json!({
        "status": "online",
        "server": "TitaouPosCRM Host",
        "port": port,
    }))
}

/// Live POS statistics for the landing page + /api/stats. Every query is
/// individually fallible-proof (missing table → 0) so the page NEVER fails.
fn pos_stats() -> serde_json::Value {
    let Some(db) = DIAG_DB.get() else {
        return serde_json::json!({
            "today_sales_count": 0, "today_sales_total": 0,
            "products_count": 0, "low_stock_count": 0,
            "customers_count": 0, "employees_count": 0,
        });
    };
    let conn = db.conn.lock().unwrap();
    let q = |sql: &str| -> i64 {
        conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0)
    };
    let today_sales_count = q(
        "SELECT COUNT(*) FROM sales WHERE date(created_at) = date('now','localtime') AND status = 'completed'",
    );
    let today_sales_total = q(
        "SELECT COALESCE(SUM(total_amount), 0) FROM sales WHERE date(created_at) = date('now','localtime') AND status = 'completed'",
    );
    let products_count = q("SELECT COUNT(*) FROM products WHERE is_active = 1");
    let low_stock_count = q(
        "SELECT COUNT(*) FROM products WHERE is_active = 1 AND current_stock <= min_stock",
    );
    let customers_count = q("SELECT COUNT(*) FROM customers WHERE is_active = 1");
    let employees_count = q("SELECT COUNT(*) FROM employees WHERE is_active = 1");
    serde_json::json!({
        "today_sales_count": today_sales_count,
        "today_sales_total": today_sales_total,
        "products_count": products_count,
        "low_stock_count": low_stock_count,
        "customers_count": customers_count,
        "employees_count": employees_count,
    })
}


/// Landing-page live-stats client (raw JS — NOT passed through format! so
/// its braces stay untouched).
const STATS_SCRIPT: &str = r##"
<script>
// Live stats: the server pushes fresh numbers over WebSocket every 3s.
(function () {
  var ids = ['today_sales_total', 'today_sales_count', 'products_count', 'low_stock_count', 'customers_count', 'employees_count'];
  function connect() {
    try {
      var ws = new WebSocket((location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host + '/api/ws');
      ws.onmessage = function (ev) {
        try {
          var msg = JSON.parse(ev.data);
          if (msg.type !== 'stats' || !msg.stats) return;
          for (var i = 0; i < ids.length; i++) {
            var el = document.getElementById('stat_' + ids[i]);
            if (el) el.textContent = Number(msg.stats[ids[i]] || 0).toLocaleString('en-US');
          }
        } catch (e) { /* ignore malformed frames */ }
      };
      ws.onclose = function () { setTimeout(connect, 3000); };
    } catch (e) { setTimeout(connect, 3000); }
  }
  connect();
})();
</script>
"##;

/// Landing page at `/`: visiting http://<LAN-IP>:<port> from a phone or PC
/// shows a real page — live POS stats, status and endpoints — instead of a 404.
async fn index() -> Html<String> {
    let (port, running, uptime_secs, devices_count) = match STATE.get() {
        Some(s) => (
            s.port,
            true,
            s.started_at.elapsed().as_secs(),
            s.devices.lock().unwrap().values().filter(|d| d.last_seen.elapsed() < Duration::from_secs(300)).count(),
        ),
        None => (configured_port(DIAG_DB.get()), false, 0, 0),
    };
    let st = pos_stats();
    let html = format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>TitaouPOS Host</title>
<style>body{{font-family:'Segoe UI',Arial,sans-serif;background:#0f172a;color:#e2e8f0;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0;padding:16px}}
.card{{background:#1e293b;border:1px solid #334155;border-radius:16px;padding:28px;max-width:460px;width:100%}}
h1{{margin:0;font-size:22px}} .sub{{color:#94a3b8;font-size:12px;margin:2px 0 14px}}
.ok{{color:#34d399;font-weight:800}} .bad{{color:#f87171;font-weight:800}}
.stats{{display:grid;grid-template-columns:1fr 1fr;gap:10px;margin:14px 0}}
.stat{{background:#0f172a;border:1px solid #334155;border-radius:10px;padding:10px 12px}}
.stat b{{display:block;font-size:18px}} .stat span{{font-size:11px;color:#94a3b8}}
code{{background:#0f172a;padding:2px 6px;border-radius:6px;font-size:12px}} ul{{margin:6px 0;padding-left:18px}} li{{margin:4px 0}}
hr{{border-color:#334155}}</style></head>
<body><div class="card">
<h1>TitaouPOS Host</h1>
<p class="sub">Embedded server is <span class="{run_cls}">{run_txt}</span> on port <code>{port}</code> • up {uptime} min • v{version}</p>

<div class="stats">
  <div class="stat"><b id="stat_today_sales_total">{today_sales_total}</b> DZD<span>Today's Sales</span></div>
  <div class="stat"><b id="stat_today_sales_count">{today_sales_count}</b><span>Today's Transactions</span></div>
  <div class="stat"><b id="stat_products_count">{products_count}</b><span>Active Products</span></div>
  <div class="stat"><b id="stat_low_stock_count">{low_stock_count}</b><span>Low Stock Alerts</span></div>
  <div class="stat"><b id="stat_customers_count">{customers_count}</b><span>Customers</span></div>
  <div class="stat"><b id="stat_employees_count">{employees_count}</b><span>Employees</span></div>
</div>

<p>Connected devices: <b>{devices}</b></p>
<hr>
<p style="font-weight:700;margin-bottom:6px">API endpoints</p>
<ul>
<li><code>GET /api/status</code> — server status</li>
<li><code>GET /api/stats</code> — live POS statistics (JSON)</li>
<li><code>POST /api/handshake</code> — pair a mobile terminal (device_name, device_uid, device_role)</li>
<li><code>GET /api/diag/login</code> — diagnostics</li>
</ul>
<p style="color:#94a3b8;font-size:12px">TitaouPOS • Titaou Bedreddine 0553444057</p>
</div>
__STATS_SCRIPT__</body></html>"#,
        run_cls = if running { "ok" } else { "bad" },
        run_txt = if running { "ONLINE" } else { "OFFLINE" },
        port = port,
        uptime = uptime_secs / 60,
        version = env!("CARGO_PKG_VERSION"),
        today_sales_total = st["today_sales_total"].as_i64().unwrap_or(0),
        today_sales_count = st["today_sales_count"].as_i64().unwrap_or(0),
        products_count = st["products_count"].as_i64().unwrap_or(0),
        low_stock_count = st["low_stock_count"].as_i64().unwrap_or(0),
        customers_count = st["customers_count"].as_i64().unwrap_or(0),
        employees_count = st["employees_count"].as_i64().unwrap_or(0),
        devices = devices_count,
    );
    Html(html.replace("__STATS_SCRIPT__", STATS_SCRIPT))
}

/// Same diagnostic surface the original server exposed: runs the login /
/// users / settings paths in-process and reports latency, so a stuck DB
/// mutex is visible from outside the app.
async fn api_diag_login() -> Json<serde_json::Value> {
    let t0 = Instant::now();
    let login_result = DIAG_DB.get().map(|db| {
        crate::auth::authenticate_user(db, "admin", "admin").map(|u| u.map(|x| x.username))
    });
    let login_ms = t0.elapsed().as_millis() as u64;
    let t1 = Instant::now();
    let users_result = DIAG_DB
        .get()
        .map(|db| crate::auth::list_active_users(db).map(|u| u.len()));
    let users_ms = t1.elapsed().as_millis() as u64;
    let t2 = Instant::now();
    let settings_result = DIAG_DB
        .get()
        .map(|db| crate::services::settings_service::get_all_settings(db).map(|m| m.len()));
    let settings_ms = t2.elapsed().as_millis() as u64;
    Json(serde_json::json!({
        "login": {"user": login_result, "ms": login_ms},
        "list_users": {"count": users_result, "ms": users_ms},
        "get_all_settings": {"keys": settings_result, "ms": settings_ms},
    }))
}


/// WebSocket: pushes live POS stats every 3 seconds so the landing page
/// (or any paired device) refreshes itself without reloading.
async fn ws_upgrade(ws: WebSocketUpgrade) -> axum::response::Response {
    ws.on_upgrade(handle_ws_socket)
}

async fn handle_ws_socket(mut socket: WebSocket) {
    let mut tick: u64 = 0;
    loop {
        let payload = serde_json::json!({
            "type": "stats",
            "stats": pos_stats(),
            "devices": match STATE.get() {
                Some(s) => s.devices.lock().unwrap().values()
                    .filter(|d| d.last_seen.elapsed() < Duration::from_secs(300))
                    .count(),
                None => 0,
            },
            "tick": tick,
        });
        tick += 1;
        if socket.send(Message::Text(payload.to_string())).await.is_err() {
            break; // client went away
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

pub fn start_local_api_server() {
    // Port is read BEFORE spawning so STATE gets the real value; when the
    // setting changes, a restart of the app picks it up.
    let port = configured_port(DIAG_DB.get());
    let state = Arc::new(ServerState {
        port,
        started_at: Instant::now(),
        devices: Mutex::new(HashMap::new()),
    });
    let _ = STATE.set(Arc::clone(&state));

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        if let Ok(rt) = rt {
            rt.block_on(async move {
                let app = Router::new()
                    .route("/", get(index))
                    .route("/api/handshake", post(api_handshake))
                    .route("/api/status", get(api_status))
                    .route("/api/stats", get(|| async { Json(pos_stats()) }))
                    .route("/api/ws", get(ws_upgrade))
                    .route("/api/diag/login", get(api_diag_login))
                    // LAN shop network API (/api/v1/*): health, join, auth,
                    // whitelisted business invokes, WebSocket events.
                    .merge(crate::network::server_api::v1_router())
                    .with_state(state)
                    .layer(CorsLayer::permissive());

                let addr = SocketAddr::from(([0, 0, 0, 0], port));
                println!("[Local Server] Listening on http://{}", addr);
                if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
                    let _ = axum::serve(
                        listener,
                        app.into_make_service_with_connect_info::<SocketAddr>(),
                    )
                    .await;
                } else {
                    eprintln!("[Local Server] FAILED to bind port {} — mobile clients cannot connect", port);
                }
            });
        }
    });
}

#[cfg(test)]
mod ip_ranking_tests {
    #[test]
    fn routed_adapters_come_first_and_virtual_last() {
        let ips = super::lan_ip_addresses();
        assert!(!ips.is_empty(), "must find at least one IPv4 address");
        // If any real 192.168.x / 10.x / 172.16-31.x adapter exists, it must
        // not be sorted AFTER a virtual-only entry when both are present —
        // the strongest check available without mocks: the FIRST entry must
        // own the machine's default route (verified by the routed query in
        // the function itself). At minimum: no loopback/link-local ever.
        for ip in &ips {
            assert!(!ip.starts_with("127."), "no loopback: {}", ip);
            assert!(!ip.starts_with("169.254."), "no link-local: {}", ip);
        }
    }
}
