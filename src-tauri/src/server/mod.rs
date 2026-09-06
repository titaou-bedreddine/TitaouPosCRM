use axum::{extract::State as AxState, response::Html, routing::get, routing::post, Json, Router};
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
static DIAG_DB: OnceLock<DbState> = OnceLock::new();

pub fn set_diag_db(db: DbState) {
    let _ = DIAG_DB.set(db);
}

/// All non-loopback IPv4 addresses of this machine (LAN), powers the QR.
fn lan_ip_addresses() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike '127.*' -and $_.IPAddress -notlike '169.254.*' } | Select-Object -ExpandProperty IPAddress)",
            ])
            .creation_flags(0x08000000)
            .output();
        if let Ok(o) = output {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                let l = line.trim().to_string();
                if !l.is_empty() && !out.contains(&l) {
                    out.push(l);
                }
            }
        }
    }
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
    8080
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
        "server": "TitaouPosT Host",
        "port": port,
        "paired": true,
    }))
}

async fn api_status() -> Json<serde_json::Value> {
    let port = STATE.get().map(|s| s.port).unwrap_or(8080);
    Json(serde_json::json!({
        "status": "online",
        "server": "TitaouPosT Host",
        "port": port,
    }))
}

/// Landing page at `/`: visiting http://<LAN-IP>:<port> from a phone or PC
/// shows a real page (name, status, endpoints) instead of a 404.
async fn index() -> Html<String> {
    let (port, running, devices) = match STATE.get() {
        Some(s) => (
            s.port,
            true,
            s.devices.lock().unwrap().values().filter(|d| d.last_seen.elapsed() < Duration::from_secs(300)).count(),
        ),
        None => (configured_port(DIAG_DB.get()), false, 0),
    };
    Html(format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>TitaouPOS Host</title>
<style>body{{font-family:'Segoe UI',Arial,sans-serif;background:#0f172a;color:#e2e8f0;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0}}
.card{{background:#1e293b;border:1px solid #334155;border-radius:16px;padding:32px;max-width:420px;width:92%}}
h1{{margin:0 0 4px;font-size:22px}} .ok{{color:#34d399;font-weight:800}} .bad{{color:#f87171;font-weight:800}}
code{{background:#0f172a;padding:2px 6px;border-radius:6px;font-size:13px}} li{{margin:6px 0}}</style></head>
<body><div class="card">
<h1>TitaouPOS Host</h1>
<p>Embedded server is <span class="{run_cls}">{run_txt}</span> on port <code>{port}</code></p>
<p>Connected devices: <b>{devices}</b></p>
<hr style="border-color:#334155">
<p style="font-weight:700;margin-bottom:6px">API endpoints</p>
<ul>
<li><code>GET /api/status</code> — server status</li>
<li><code>POST /api/handshake</code> — pair a mobile terminal (device_name, device_uid, device_role)</li>
<li><code>GET /api/diag/login</code> — diagnostics</li>
</ul>
<p style="color:#94a3b8;font-size:12px">TitaouPOS • Titaou Bedreddine 0553444057</p>
</div></body></html>"#,
        run_cls = if running { "ok" } else { "bad" },
        run_txt = if running { "ONLINE" } else { "OFFLINE" },
        port = port,
        devices = devices,
    ))
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
                    .route("/api/diag/login", get(api_diag_login))
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
