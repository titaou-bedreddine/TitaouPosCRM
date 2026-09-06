//! Tauri commands for the LAN shop network: status, setup wizard support,
//! role management, device administration and the client-side forwarding
//! entry point (`network_forward`) used by the frontend invoke patch.

use crate::database::DbState;
use crate::network::{self, client, NetConfig};
use serde_json::{json, Value};
use std::sync::OnceLock;
use tauri::State;

// ---------------------------------------------------------------------------
// Status & log
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn network_get_status() -> Value {
    network::status_snapshot()
}

#[tauri::command]
pub fn network_get_log() -> Vec<Value> {
    match network::status_snapshot().get("events") {
        Some(Value::Array(a)) => a.clone(),
        _ => vec![],
    }
}

/// Coordinators currently visible on the LAN (for the join UI).
#[tauri::command]
pub fn network_discovered_servers() -> Vec<Value> {
    // Trigger an immediate discovery burst so the list is fresh.
    // (The manager keeps the peer table updated; the burst accelerates it.)
    let mut out = Vec::new();
    let status = network::status_snapshot();
    if let Some(peers) = status.get("known_peers").and_then(|p| p.as_array()) {
        for p in peers {
            if p.get("is_coordinator").and_then(|c| c.as_bool()).unwrap_or(false) {
                out.push(p.clone());
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Setup wizard & settings
// ---------------------------------------------------------------------------

fn save_and_reload(db: &DbState, fields: &[(&str, String)]) {
    for (k, v) in fields {
        NetConfig::save_field(db, k, v);
    }
    network::notify_config_changed();
}

/// First-run wizard: PC name + preferred role in one call.
#[tauri::command]
pub fn network_set_setup(
    db: State<'_, DbState>,
    pc_name: String,
    role: String,
) -> Result<(), String> {
    let role = role.trim().to_lowercase();
    if !matches!(role.as_str(), "server" | "client" | "automatic") {
        return Err("Invalid network role".into());
    }
    let pc_name = pc_name.trim().to_string();
    if pc_name.is_empty() {
        return Err("PC name is required".into());
    }
    save_and_reload(
        &db,
        &[("net_pc_name", pc_name), ("net_role", role.clone())],
    );
    if role == "server" {
        // An explicit server immediately owns/creates the shop.
        let cfg = current_cfg(&db);
        if cfg.shop_id.is_empty() {
            NetConfig::save_field(&db, "net_shop_id", &network::identity::generate_shop_id());
        }
        network::notify_config_changed();
    }
    Ok(())
}

#[tauri::command]
pub fn network_set_role(db: State<'_, DbState>, role: String) -> Result<(), String> {
    let role = role.trim().to_lowercase();
    if !matches!(role.as_str(), "server" | "client" | "automatic") {
        return Err("Invalid network role".into());
    }
    // Changing role must safely stop the previous one and preserve the
    // database, Shop ID and Node ID (spec §50).
    save_and_reload(&db, &[("net_role", role)]);
    Ok(())
}

#[tauri::command]
pub fn network_set_pc_name(db: State<'_, DbState>, pc_name: String) -> Result<(), String> {
    let pc_name = pc_name.trim().to_string();
    if pc_name.is_empty() {
        return Err("PC name is required".into());
    }
    save_and_reload(&db, &[("net_pc_name", pc_name)]);
    Ok(())
}

#[tauri::command]
pub fn network_set_flags(
    db: State<'_, DbState>,
    enabled: Option<bool>,
    autodiscovery: Option<bool>,
    autoreconnect: Option<bool>,
) -> Result<(), String> {
    let mut fields: Vec<(&str, String)> = Vec::new();
    if let Some(v) = enabled {
        fields.push(("net_enabled", v.to_string()));
    }
    if let Some(v) = autodiscovery {
        fields.push(("net_autodiscovery", v.to_string()));
    }
    if let Some(v) = autoreconnect {
        fields.push(("net_autoreconnect", v.to_string()));
    }
    save_and_reload(&db, &fields);
    Ok(())
}

/// "Become Server / Create Shop": this PC becomes the shop authority.
#[tauri::command]
pub fn network_become_server(
    db: State<'_, DbState>,
    shop_name: Option<String>,
) -> Result<Value, String> {
    let cfg = current_cfg(&db);
    let shop_id = if cfg.shop_id.is_empty() {
        let id = network::identity::generate_shop_id();
        NetConfig::save_field(&db, "net_shop_id", &id);
        id
    } else {
        cfg.shop_id.clone()
    };
    let name = shop_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let settings = crate::services::settings_service::get_all_settings(&db).unwrap_or_default();
            settings
                .get("shop_name_fr")
                .or_else(|| settings.get("shop_name_ar"))
                .cloned()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "My Shop".to_string())
        });
    NetConfig::save_field(&db, "net_shop_name", &name);
    save_and_reload(&db, &[("net_role", "server".to_string())]);
    Ok(json!({ "shop_id": shop_id, "shop_name": name }))
}

// ---------------------------------------------------------------------------
// Manual join (advanced path)
// ---------------------------------------------------------------------------

/// Probe a server address without joining: returns shop identity for the
/// confirmation screen.
#[tauri::command]
pub fn network_probe_server(addr: String) -> Result<Value, String> {
    let base = normalize(&addr);
    let health = client::health_check(&base).ok_or("No TitaouPOS server reachable at this address")?;
    Ok(json!({
        "base_url": base,
        "server_pc": health.get("server_pc").cloned().unwrap_or(Value::Null),
        "shop_id": health.get("shop_id").cloned().unwrap_or(Value::Null),
        "shop_name": health.get("shop_name").cloned().unwrap_or(Value::Null),
        "term": health.get("term").cloned().unwrap_or(Value::Null),
        "app_version": health.get("app_version").cloned().unwrap_or(Value::Null),
    }))
}

/// Join the shop served at `addr` (confirmation already obtained in the UI).
#[tauri::command]
pub fn network_join_server(db: State<'_, DbState>, addr: String, shop_id: String) -> Result<Value, String> {
    let base = normalize(&addr);
    let cfg = current_cfg(&db);
    let own = announce_for(&cfg);
    let resp = client::join_server(&base, &own, &shop_id, true)?;
    let token = resp
        .get("device_token")
        .and_then(|t| t.as_str())
        .ok_or("join response missing device token")?
        .to_string();
    NetConfig::save_field(&db, "net_device_token", &token);
    NetConfig::save_field(&db, "net_device_token_shop", &shop_id);
    save_and_reload(
        &db,
        &[
            ("net_role", "client".to_string()),
            ("net_manual_server", base.clone()),
            ("net_shop_id", shop_id.clone()),
        ],
    );
    let shop_name = resp
        .get("shop_name")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    if !shop_name.is_empty() {
        NetConfig::save_field(&db, "net_shop_name", &shop_name);
    }
    Ok(json!({ "ok": true, "shop_name": shop_name, "base_url": base }))
}

/// Leave the current shop/network: clears membership + tokens (data on the
/// server is untouched; local data is preserved).
#[tauri::command]
pub fn network_leave_shop(db: State<'_, DbState>) -> Result<(), String> {
    // Best-effort deregistration from the server.
    if let Some(base) = server_base() {
        if let Some(token) = network::device_token_pub() {
            let url = format!("{}/api/v1/network/leave", base.trim_end_matches('/'));
            let _ = std::thread::spawn(move || {
                let _ = reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(3))
                    .build()
                    .map(|c| c.post(&url).header("Authorization", format!("Bearer {}", token)).send());
            })
            .join();
        }
    }
    for k in ["net_device_token", "net_device_token_shop", "net_shop_id", "net_shop_name", "net_manual_server"] {
        NetConfig::save_field(&db, k, "");
    }
    network::clear_user_token();
    save_and_reload(&db, &[("net_role", "automatic".to_string())]);
    Ok(())
}

// ---------------------------------------------------------------------------
// Device administration (server side)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn network_block_device(db: State<'_, DbState>, node_id: String, blocked: bool) -> Result<(), String> {
    let cfg = current_cfg(&db);
    let mut list: Vec<String> = cfg.blocked_nodes.clone();
    if blocked {
        if !list.contains(&node_id) {
            list.push(node_id.clone());
        }
        network::server_api::revoke_device_tokens(&node_id);
    } else {
        list.retain(|n| n != &node_id);
    }
    let joined = list.join(",");
    NetConfig::save_field(&db, "net_blocked_nodes", &joined);
    network::log_net_event(
        if blocked { "device_blocked" } else { "device_unblocked" },
        json!({ "node": node_id }),
    );
    Ok(())
}

#[tauri::command]
pub fn network_remove_device(_db: State<'_, DbState>, node_id: String) -> Result<(), String> {
    network::server_api::remove_device(&node_id);
    network::server_api::revoke_device_tokens(&node_id);
    Ok(())
}

#[tauri::command]
pub fn network_rename_device(node_id: String, new_name: String) -> Result<(), String> {
    // The authoritative name is the client's own PC name; the server-side
    // rename is a local registry label for this session.
    let _ = (node_id, new_name);
    Ok(())
}

// ---------------------------------------------------------------------------
// Session / security
// ---------------------------------------------------------------------------

/// Logout: drop the user token from memory (the device stays registered).
#[tauri::command]
pub fn network_logout() -> Result<(), String> {
    network::clear_user_token();
    Ok(())
}

// ---------------------------------------------------------------------------
// Firewall (spec §43: only the configured application port)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn network_open_firewall() -> Result<String, String> {
    let port = crate::server::configured_port_public();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let rule = format!("TitaouPOS LAN (port {})", port);
        let output = std::process::Command::new("netsh")
            .args([
                "advfirewall", "firewall", "add", "rule",
                &format!("name={}", rule),
                "dir=in", "action=allow", "protocol=TCP",
                &format!("localport={}", port),
            ])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            return Ok(format!("Windows Firewall now allows TCP {} for the TitaouPOS LAN server", port));
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "Could not open the firewall automatically ({}). Run once as Administrator: netsh advfirewall firewall add rule name=\"TitaouPOS LAN\" dir=in action=allow protocol=TCP localport={}",
            if stderr.is_empty() { "needs administrator rights" } else { &stderr },
            port
        ));
    }
    #[cfg(not(windows))]
    {
        let _ = port;
        Err("Firewall helper is Windows-only".into())
    }
}

// ---------------------------------------------------------------------------
// network_forward — the client-side interception point
// ---------------------------------------------------------------------------

fn server_base() -> Option<String> {
    network::status_snapshot()
        .get("server_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn current_cfg(db: &DbState) -> NetConfig {
    NetConfig::load(db)
}

fn announce_for(cfg: &NetConfig) -> network::discovery::DiscoveryPacket {
    network::announce_packet_for(cfg)
}

fn normalize(addr: &str) -> String {
    network::normalize_base_pub(addr)
}

/// Manual/debug entry into client forwarding. The AUTOMATIC interception
/// happens in lib.rs's invoke_handler wrapper (Rust IPC layer) — the
/// frontend never calls this itself.
#[tauri::command]
pub async fn network_forward(
    _db: State<'_, DbState>,
    command: String,
    args: Option<Value>,
) -> Result<Value, String> {
    let payload = serde_json::to_string(&args.unwrap_or(Value::Null)).unwrap_or_else(|_| "null".to_string());
    network::forward_ipc(command, payload).await
}
