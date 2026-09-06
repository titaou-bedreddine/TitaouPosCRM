//! LAN client role: find the shop server, join it, stay connected, forward
//! business operations over the authenticated API and surface real-time
//! events to the local UI.

use super::discovery::DiscoveryPacket;
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Duration;

pub struct ServerCandidate {
    pub base_url: String,
    pub node_id: String,
    pub pc_name: String,
    pub shop_id: String,
    pub shop_name: String,
    pub term: u64,
    pub data_rows: u64,
}

fn http() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(4))
            .connect_timeout(Duration::from_secs(2))
            .build()
            .expect("http client")
    })
}

/// Verify a candidate server and pull its identity via /api/v1/health.
pub fn health_check(base_url: &str) -> Option<Value> {
    let url = format!("{}/api/v1/health", base_url.trim_end_matches('/'));
    let resp = http().get(&url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().ok()?;
    if v.get("status").and_then(|s| s.as_str()) == Some("ok") {
        Some(v)
    } else {
        None
    }
}

/// POST /api/v1/network/join. `confirm=false` returns the shop identity for
/// the confirmation UI; `confirm=true` registers this terminal and returns
/// the device token.
pub fn join_server(
    base_url: &str,
    own: &DiscoveryPacket,
    shop_id: &str,
    confirm: bool,
) -> Result<Value, String> {
    let url = format!("{}/api/v1/network/join", base_url.trim_end_matches('/'));
    let body = json!({
        "node_id": own.node_id,
        "pc_name": own.pc_name,
        "role_pref": own.role_pref,
        "app_version": own.app_version,
        "shop_id": shop_id,
        "confirm": confirm,
    });
    let resp = http().post(&url).json(&body).send().map_err(|e| format!("join failed: {}", e))?;
    let status = resp.status();
    let v: Value = resp.json().map_err(|e| format!("bad join response: {}", e))?;
    if !status.is_success() {
        return Err(v
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("join rejected")
            .to_string());
    }
    Ok(v)
}

/// POST /api/v1/auth/login → (user JSON, user token).
pub fn login_on_server(
    base_url: &str,
    device_token: &str,
    username: &str,
    password: &str,
) -> Result<(Value, String), String> {
    let url = format!("{}/api/v1/auth/login", base_url.trim_end_matches('/'));
    let resp = http()
        .post(&url)
        .header("Authorization", format!("Bearer {}", device_token))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .map_err(|e| format!("cannot reach the shop server: {}", e))?;
    let status = resp.status();
    let v: Value = resp.json().map_err(|e| format!("bad login response: {}", e))?;
    if !status.is_success() {
        return Err(v.get("error").and_then(|e| e.as_str()).unwrap_or("Login failed").to_string());
    }
    let token = v
        .get("token")
        .and_then(|t| t.as_str())
        .ok_or("login response missing token")?
        .to_string();
    Ok((v.get("user").cloned().unwrap_or(Value::Null), token))
}

/// Blocking invoke used by the manager thread (rare; diagnostics only).
pub fn invoke_blocking(
    base_url: &str,
    token: &str,
    command: &str,
    args: &Value,
) -> Result<Value, String> {
    let url = format!("{}/api/v1/invoke/{}", base_url.trim_end_matches('/'), command);
    let resp = http()
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(args)
        .send()
        .map_err(|e| format!("cannot reach the shop server: {}", e))?;
    parse_invoke_response(resp, command)
}

fn parse_invoke_response(resp: reqwest::blocking::Response, command: &str) -> Result<Value, String> {
    let status = resp.status();
    let v: Value = resp.json().map_err(|e| format!("bad response from server: {}", e))?;
    if v.get("ok").and_then(|o| o.as_bool()) == Some(true) {
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    } else {
        Err(v
            .get("error")
            .and_then(|e| e.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("server rejected '{}' (HTTP {})", command, status)))
    }
}

/// Build the server candidate from a discovery announce + a health probe.
pub fn candidate_from_announce(pkt: &DiscoveryPacket) -> Option<ServerCandidate> {
    let ip = pkt.lan_ips.first()?;
    if !pkt.is_coordinator {
        return None;
    }
    Some(ServerCandidate {
        base_url: format!("http://{}:{}", ip, pkt.http_port),
        node_id: pkt.node_id.clone(),
        pc_name: pkt.pc_name.clone(),
        shop_id: pkt.shop_id.clone(),
        shop_name: pkt.shop_name.clone(),
        term: pkt.term,
        data_rows: pkt.data_rows,
    })
}

// ---------------------------------------------------------------------------
// WebSocket event listener (own tokio runtime thread)
// ---------------------------------------------------------------------------

pub enum WsMessage {
    /// Business event (sale_created, ...) as JSON.
    Event(Value),
    /// Server hello/ping carrying the coordinator term.
    Term(u64),
}

/// Connect to the server WS and stream messages into `sink` until the
/// connection dies or `stop` flips to true. Blocks its thread; the manager
/// restarts it whenever the target server changes.
pub fn run_ws_listener(
    base_url: String,
    token: String,
    stop: tokio::sync::watch::Receiver<bool>,
    sink: impl Fn(WsMessage) + Send + 'static,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let Ok(rt) = rt else { return };
    let ws_url = base_url.trim_end_matches('/').replacen("http", "ws", 1);
    let _ = rt.block_on(async move {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message as WsMsg;
        let mut stop = stop;
        let url = format!("{}/api/v1/ws?token={}", ws_url, token);
        let Ok((mut ws, _)) = tokio_tungstenite::connect_async(url).await else {
            return;
        };
        // Reader + light keep-alive; exits on stop signal or socket death.
        loop {
            tokio::select! {
                _ = stop.changed() => {
                    if *stop.borrow() { return; }
                }
                msg = ws.next() => {
                    match msg {
                        Some(Ok(WsMsg::Text(txt))) => {
                            if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                                match v.get("type").and_then(|t| t.as_str()) {
                                    Some("event") => sink(WsMessage::Event(v.get("event").cloned().unwrap_or(Value::Null))),
                                    Some("hello") | Some("ping") => {
                                        let term = v.get("term").and_then(|t| t.as_u64()).unwrap_or(0);
                                        sink(WsMessage::Term(term));
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Some(Ok(WsMsg::Ping(p))) => { let _ = ws.send(WsMsg::Pong(p)).await; }
                        Some(Ok(_)) => {}
                        _ => return,
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_requires_coordinator_flag() {
        let mut p = DiscoveryPacket::new("NODE-S".into(), "SRV".into());
        p.lan_ips = vec!["192.168.1.5".into()];
        p.http_port = 8080;
        assert!(candidate_from_announce(&p).is_none());
        p.is_coordinator = true;
        p.shop_id = "SHOP-1".into();
        let c = candidate_from_announce(&p).unwrap();
        assert_eq!(c.base_url, "http://192.168.1.5:8080");
        assert_eq!(c.shop_id, "SHOP-1");
    }
}
