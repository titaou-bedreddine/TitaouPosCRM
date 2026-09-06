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
    discovery::spawn_udp_listener("NODE-SRVA".to_string(), Arc::clone(&get_packet), sink)
        .expect("server listener binds");

    // --- Client node: binds the same port on loopback? No — a second bind
    // of 0.0.0.0:50110 in the SAME process fails; the client instead sends
    // probes and receives the unicast ANSWER on its ephemeral socket. We
    // assert on the server's answer arriving, which is exactly what a real
    // client processes to discover the shop (client's own listener runs on
    // its own PC). ----------------------------------------------------------
    let client_pkt = DiscoveryPacket::new("NODE-CLIA".into(), "POS-CLIA".into());

    // Ask the question a few times, spaced like the real probe burst.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut answered: Vec<DiscoveryPacket> = Vec::new();
    'outer: loop {
        // Probe + collect the unicast answer with a fresh socket per probe
        // (mirrors send_probe_burst + answer_probe).
        let probe = {
            let mut p = client_pkt.clone();
            p.probe = true;
            p
        };
        let payload = serde_json::to_vec(&probe).unwrap();
        let sock = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        sock.set_read_timeout(Some(Duration::from_millis(900))).unwrap();
        sock.set_broadcast(true).unwrap();
        let _ = sock.send_to(&payload, ("255.255.255.255", discovery::DISCOVERY_PORT));
        let _ = sock.send_to(&payload, ("127.0.0.1", discovery::DISCOVERY_PORT));
        let mut buf = [0u8; 4096];
        if let Ok((n, _)) = sock.recv_from(&mut buf) {
            if let Ok(pkt) = serde_json::from_slice::<DiscoveryPacket>(&buf[..n]) {
                if pkt.is_valid() && !pkt.probe && pkt.node_id == "NODE-SRVA" {
                    answered.push(pkt);
                    break 'outer;
                }
            }
        }
        if Instant::now() > deadline {
            break;
        }
    }

    assert!(
        !answered.is_empty(),
        "client never discovered the server via UDP probe/answer"
    );
    let found = &answered[0];
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
}
