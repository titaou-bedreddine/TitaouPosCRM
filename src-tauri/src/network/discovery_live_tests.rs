//! Live two-node UDP discovery test: a "server" node (announcer + probe
//! answerer) and a "client" node (prober) exchange real datagrams over
//! 127.0.0.1 and the client must learn the server's identity — the same
//! wire path two PCs use on a LAN (firewall aside).

#![cfg(test)]

use crate::network::discovery::{self, DiscoveryPacket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn live_udp_discovery_client_finds_server() {
    // --- Server node: coordinator announce + probe answering -------------
    let mut server_pkt = DiscoveryPacket::new("NODE-SRVA".into(), "POS-SRVA".into());
    server_pkt.is_coordinator = true;
    server_pkt.shop_id = "SHOP-LIVE-DISC".into();
    server_pkt.shop_name = "Live Discovery Shop".into();
    server_pkt.term = 7;
    server_pkt.http_port = 8080;
    server_pkt.lan_ips = vec!["127.0.0.1".into()];
    let server_current = server_pkt.clone();

    let seen_by_client: Arc<Mutex<Vec<DiscoveryPacket>>> = Arc::new(Mutex::new(Vec::new()));
    let sink: discovery::PacketSink = {
        let sink_seen = Arc::clone(&seen_by_client);
        Arc::new(move |pkt, _ip| {
            sink_seen.lock().unwrap().push(pkt);
        })
    };
    let get_packet: Arc<dyn Fn() -> DiscoveryPacket + Send + Sync> =
        Arc::new(move || server_current.clone());
    // The discovery port is one-per-PC: if the real app (or another test
    // process) currently holds it, this live wire test cannot run — skip
    // rather than fail (the pure protocol tests still cover the format).
    if discovery::spawn_udp_listener("NODE-SRVA".to_string(), Arc::clone(&get_packet), sink).is_err() {
        eprintln!("UDP 50110 busy (app running?) — skipping live discovery wire test");
        return;
    }

    // --- Client node identity ---
    let client_pkt = DiscoveryPacket::new("NODE-CLIA".into(), "POS-CLIA".into());

    // --- Client node: run the REAL production probe path. Its answers feed
    // a sink (the same type the manager registers); the peer must land
    // there — this exercises send_probe_burst's receive loop, the exact
    // code a client PC runs (bug #4 regression: a send-only burst used to
    // drop every answer). ---------------------------------------------------
    let mut got_peer: Option<DiscoveryPacket> = None;
    let client_seen: std::sync::Mutex<Option<DiscoveryPacket>> = std::sync::Mutex::new(None);
    let client_seen = std::sync::Arc::new(client_seen);
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let sink: discovery::PacketSink = {
            let cs = Arc::clone(&client_seen);
            Arc::new(move |pkt, _ip| {
                let mut g = cs.lock().unwrap();
                if g.is_none() {
                    *g = Some(pkt);
                }
            })
        };
        discovery::send_probe_burst(&client_pkt, &sink);
        if let Some(p) = client_seen.lock().unwrap().clone() {
            got_peer = Some(p);
            break;
        }
        if Instant::now() > deadline {
            break;
        }
    }

    let answered = got_peer.expect("client never discovered the server via the real probe path");
    let found = &answered;
    assert!(found.is_coordinator);
    assert_eq!(found.shop_id, "SHOP-LIVE-DISC");
    assert_eq!(found.pc_name, "POS-SRVA");
    assert_eq!(found.http_port, 8080);
    assert!(found.lan_ips.contains(&"127.0.0.1".to_string()));

    // The announce (non-probe) path must also reach a sink on a foreign
    // listener — the periodic announcer's packet. Send one broadcast from
    // the "server" side directly and confirm the client-side sink route:
    // here we just re-verify packet validity both ways.
    let server_announce = get_packet();
    let v = serde_json::to_vec(&server_announce).unwrap();
    let back: DiscoveryPacket = serde_json::from_slice(&v).unwrap();
    assert!(back.is_valid() && back.is_coordinator);
}

/// A second bind of the discovery port must not panic — it reports an
/// error that startup ignores (mDNS + API still work). Runs AFTER the
/// main test inside the same #[test] because both need exclusive use of
/// the single global discovery port (parallel tests would race).
#[test]
fn live_udp_discovery_and_double_bind_safety() {
    live_udp_discovery_client_finds_server();

    // The discovery listener from the first part still holds 0.0.0.0:50110
    // (its thread loops forever) — exactly the "port taken" state to verify.
    let pkt = DiscoveryPacket::new("NODE-OWN".into(), "PC".into());
    let sink: discovery::PacketSink = Arc::new(|_p, _i| {});
    let get: Arc<dyn Fn() -> DiscoveryPacket + Send + Sync> = Arc::new(move || pkt.clone());
    let result = discovery::spawn_udp_listener("NODE-OTHER".into(), get, sink);
    assert!(result.is_err(), "second bind should report an error, not panic");
    let _ = Instant::now();
}
