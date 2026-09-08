//! LAN discovery for the TitaouPOS shop network.
//!
//! Two complementary mechanisms, both passive-safe and lightweight:
//!   1. mDNS (`_titaouposcrm._tcp.local.`) — preferred, zero-traffic when idle.
//!   2. UDP broadcast on a fixed port — fallback for Wi-Fi APs that filter
//!      multicast, and the accelerant at startup (probes answered unicast).
//!
//! Discovery only says "a TitaouPOS node exists here" — it is NEVER
//! authentication and carries no credentials.

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::Arc;
use std::time::Duration;

/// Fixed UDP discovery port (LAN-local, one listener per PC).
pub const DISCOVERY_PORT: u16 = 50110;
/// mDNS service type.
pub const MDNS_SERVICE: &str = "_titaouposcrm._tcp.local.";
const MAGIC: &str = "TITAOPOS-NET";
const PROTOCOL_VERSION: u32 = 1;

/// One announce/probe packet. Also the payload mDNS TXT records reconstruct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryPacket {
    pub magic: String,
    pub protocol: u32,
    /// true = "tell me who you are" (answered with a unicast announce).
    #[serde(default)]
    pub probe: bool,
    pub node_id: String,
    pub pc_name: String,
    /// server | client | automatic
    pub role_pref: String,
    pub is_coordinator: bool,
    pub shop_id: String,
    pub shop_name: String,
    pub term: u64,
    pub http_port: u16,
    #[serde(default)]
    pub lan_ips: Vec<String>,
    pub app_version: String,
    /// Sales+products+customers row estimate used by the election.
    #[serde(default)]
    pub data_rows: u64,
}

impl DiscoveryPacket {
    pub fn new(node_id: String, pc_name: String) -> Self {
        DiscoveryPacket {
            magic: MAGIC.to_string(),
            protocol: PROTOCOL_VERSION,
            probe: false,
            node_id,
            pc_name,
            role_pref: "automatic".into(),
            is_coordinator: false,
            shop_id: String::new(),
            shop_name: String::new(),
            term: 0,
            http_port: 8080,
            lan_ips: Vec::new(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            data_rows: 0,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == MAGIC && self.protocol == PROTOCOL_VERSION && !self.node_id.is_empty()
    }
}

/// Callback invoked for every valid packet from ANOTHER node.
pub type PacketSink = Arc<dyn Fn(DiscoveryPacket, String) + Send + Sync>;

/// Broadcast one packet to the LAN (255.255.255.255 + loopback so a same-PC
/// second profile/dev setup also sees it).
fn broadcast_packet(packet: &DiscoveryPacket) {
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else { return };
    let _ = sock.set_broadcast(true);
    let payload = match serde_json::to_vec(packet) {
        Ok(p) => p,
        Err(_) => return,
    };
    for addr in ["255.255.255.255", "127.0.0.1"] {
        let _ = sock.send_to(&payload, (addr, DISCOVERY_PORT));
    }
}

/// Answer a probe directly to its sender (unicast, ephemeral port).
fn answer_probe(packet: &DiscoveryPacket, to: &str) {
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else { return };
    if let Ok(payload) = serde_json::to_vec(packet) {
        let _ = sock.send_to(&payload, to);
    }
}

/// Long-running UDP listener: feeds every valid foreign packet to the sink
/// and answers probes with a unicast announce. `get_packet` returns the
/// current announce payload.
pub fn spawn_udp_listener(
    own_node_id: String,
    get_packet: Arc<dyn Fn() -> DiscoveryPacket + Send + Sync>,
    sink: PacketSink,
) -> std::io::Result<()> {
    let sock = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT))?;
    let _ = sock.set_read_timeout(Some(Duration::from_secs(30)));
    std::thread::Builder::new()
        .name("net-udp-listener".into())
        .spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match sock.recv_from(&mut buf) {
                    Ok((n, src)) => {
                        let Ok(pkt) = serde_json::from_slice::<DiscoveryPacket>(&buf[..n]) else {
                            continue;
                        };
                        if !pkt.is_valid() || pkt.node_id == own_node_id {
                            continue;
                        }
                        if pkt.probe {
                            answer_probe(&get_packet(), &src.to_string());
                            continue;
                        }
                        bump_diag(|d| {
                            d.announces_received += 1;
                            d.last_answer_from = src.ip().to_string();
                        });
                        sink(pkt, src.ip().to_string());
                    }
                    Err(_) => continue, // timeout → keep listening
                }
            }
        })?;
    Ok(())
}

/// Periodic announcer: broadcasts our announce packet every `interval`.
pub fn spawn_announcer(
    get_packet: Arc<dyn Fn() -> DiscoveryPacket + Send + Sync>,
    interval: Duration,
) -> Arc<std::sync::atomic::AtomicBool> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = Arc::clone(&stop);
    std::thread::Builder::new()
        .name("net-announcer".into())
        .spawn(move || {
            let sock = UdpSocket::bind("0.0.0.0:0").ok();
            if let Some(s) = &sock {
                let _ = s.set_broadcast(true);
            }
            loop {
                if stop2.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(s) = &sock {
                    if let Ok(payload) = serde_json::to_vec(&get_packet()) {
                        let pkt = get_packet();
                        bump_diag(|d| d.announces_sent += 1);
                        // Limited broadcast + directed per-subnet broadcasts +
                        // loopback: resilient to adapters/drivers that silently
                        // drop 255.255.255.255.
                        let _ = s.send_to(&payload, ("255.255.255.255", DISCOVERY_PORT));
                        for ip in &pkt.lan_ips {
                            if let Ok(a) = ip.parse::<std::net::Ipv4Addr>() {
                                let subnet_broadcast = std::net::Ipv4Addr::from(u32::from(a) | 0xFF);
                                let _ = s.send_to(&payload, (subnet_broadcast, DISCOVERY_PORT));
                            }
                        }
                        let _ = s.send_to(&payload, ("127.0.0.1", DISCOVERY_PORT));
                    }
                }
                std::thread::sleep(interval);
            }
        })
        .expect("spawn announcer");
    stop
}

/// Send a probe burst and COLLECT the answers. A probe's answer is unicast
/// back to this socket's source port, so the socket must stay alive and
/// receive — a send-only burst drops every answer on the floor (this was
/// exactly the "no discovery" failure mode on real multi-adapter LANs where
/// 255.255.255.255 broadcasts get filtered). Answers are also sent to the
/// sink so the manager learns the peer immediately.
pub fn send_probe_burst(own: &DiscoveryPacket, unicast_targets: &[String], sink: &PacketSink) {
    let mut probe = own.clone();
    probe.probe = true;
    let payload = match serde_json::to_vec(&probe) {
        Ok(p) => p,
        Err(_) => return,
    };
    let sock = UdpSocket::bind("0.0.0.0:0").expect("probe socket");
    let _ = sock.set_broadcast(true);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(450)));
    bump_diag(|d| d.probes_sent += 1);
    for _ in 0..3 {
        // Limited broadcast + directed subnet broadcast + loopback + DIRECT
        // UNICAST to known/likely servers. On LANs where adapters or
        // firewalls eat broadcast frames (common with virtual adapters),
        // the unicast path is the one that actually gets through.
        let _ = sock.send_to(&payload, ("255.255.255.255", DISCOVERY_PORT));
        for ip in &own.lan_ips {
            if let Ok(a) = ip.parse::<std::net::Ipv4Addr>() {
                let subnet_broadcast = std::net::Ipv4Addr::from(u32::from(a) | 0xFF);
                let _ = sock.send_to(&payload, (subnet_broadcast, DISCOVERY_PORT));
            }
        }
        let _ = sock.send_to(&payload, ("127.0.0.1", DISCOVERY_PORT));
        for target in unicast_targets {
            if let Ok(a) = target.parse::<std::net::Ipv4Addr>() {
                let _ = sock.send_to(&payload, (a, DISCOVERY_PORT));
            }
        }
        // Collect answers arriving within this round.
        let mut buf = [0u8; 4096];
        while let Ok((n, src)) = sock.recv_from(&mut buf) {
            if let Ok(pkt) = serde_json::from_slice::<DiscoveryPacket>(&buf[..n]) {
                if pkt.is_valid() && pkt.node_id != own.node_id && !pkt.probe {
                    bump_diag(|d| d.answers_received += 1);
                    sink(pkt, src.ip().to_string());
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Discovery diagnostics (Settings → Network: what the wire REALLY did)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DiscoveryDiag {
    pub probes_sent: u64,
    pub answers_received: u64,
    pub announces_received: u64,
    pub announces_sent: u64,
    pub last_answer_from: String,
}

static DIAG: std::sync::OnceLock<std::sync::Mutex<DiscoveryDiag>> = std::sync::OnceLock::new();

fn diag() -> &'static std::sync::Mutex<DiscoveryDiag> {
    DIAG.get_or_init(|| std::sync::Mutex::new(DiscoveryDiag::default()))
}

pub fn bump_diag(f: impl FnOnce(&mut DiscoveryDiag)) {
    if let Ok(mut d) = diag().lock() {
        f(&mut d);
    }
}

pub fn diagnostics_snapshot() -> DiscoveryDiag {
    diag().lock().map(|d| d.clone()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// mDNS
// ---------------------------------------------------------------------------

pub struct MdnsHandle {
    daemon: ServiceDaemon,
    /// Full instance name currently registered (when coordinating).
    registered: std::sync::Mutex<Option<String>>,
    browse_stopper: std::sync::Mutex<Option<String>>,
}

fn txt_from_packet(p: &DiscoveryPacket) -> HashMap<String, String> {
    let mut t: HashMap<String, String> = HashMap::new();
    t.insert("node".into(), p.node_id.clone());
    t.insert("pc".into(), p.pc_name.clone());
    t.insert("shop".into(), p.shop_id.clone());
    t.insert("shopname".into(), p.shop_name.chars().take(40).collect());
    t.insert("role".into(), p.role_pref.clone());
    t.insert("coord".into(), if p.is_coordinator { "1" } else { "0" }.into());
    t.insert("term".into(), p.term.to_string());
    t.insert("ver".into(), p.app_version.clone());
    t.insert("data".into(), p.data_rows.to_string());
    t
}

/// Rebuild a DiscoveryPacket from mDNS TXT + connection info.
fn packet_from_mdns_info(info: &ServiceInfo, from_ip: String) -> Option<DiscoveryPacket> {
    let get = |k: &str| info.get_property_val_str(k).unwrap_or("").to_string();
    let node = get("node");
    if node.is_empty() {
        return None;
    }
    Some(DiscoveryPacket {
        magic: MAGIC.into(),
        protocol: PROTOCOL_VERSION,
        probe: false,
        node_id: node,
        pc_name: get("pc"),
        role_pref: {
            let r = get("role");
            if r.is_empty() { "automatic".into() } else { r }
        },
        is_coordinator: get("coord") == "1",
        shop_id: get("shop"),
        shop_name: get("shopname"),
        term: get("term").parse().unwrap_or(0),
        http_port: info.get_port(),
        lan_ips: vec![from_ip],
        app_version: {
            let v = get("ver");
            if v.is_empty() { env!("CARGO_PKG_VERSION").into() } else { v }
        },
        data_rows: get("data").parse().unwrap_or(0),
    })
}

impl MdnsHandle {
    pub fn new() -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
        Ok(MdnsHandle {
            daemon,
            registered: std::sync::Mutex::new(None),
            browse_stopper: std::sync::Mutex::new(None),
        })
    }

    /// (Re)register our mDNS service — used when acting as coordinator.
    pub fn advertise(&self, packet: &DiscoveryPacket) {
        let mut reg = self.registered.lock().unwrap();
        if let Some(old) = reg.take() {
            let _ = self.daemon.unregister(&old);
        }
        let host = format!(
            "titaoupos-{}.local.",
            packet.node_id.strip_prefix("NODE-").unwrap_or(&packet.node_id).to_lowercase()
        );
        let props = txt_from_packet(packet);
        let Ok(info) = ServiceInfo::new(
            MDNS_SERVICE,
            &packet.node_id,
            &host,
            "",
            packet.http_port,
            Some(props),
        )
        .map(|i| i.enable_addr_auto())
        else {
            return;
        };
        if self.daemon.register(info).is_ok() {
            *reg = Some(format!("{}.{}", packet.node_id, MDNS_SERVICE));
        }
    }

    pub fn stop_advertise(&self) {
        let mut reg = self.registered.lock().unwrap();
        if let Some(old) = reg.take() {
            let _ = self.daemon.unregister(&old);
        }
    }

    /// Browse the shop network; resolved services feed the sink.
    pub fn browse(&self, sink: PacketSink) {
        {
            let mut b = self.browse_stopper.lock().unwrap();
            if b.is_some() {
                return; // already browsing
            }
            *b = Some(MDNS_SERVICE.to_string());
        }
        if let Ok(receiver) = self.daemon.browse(MDNS_SERVICE) {
            std::thread::Builder::new()
                .name("net-mdns-browse".into())
                .spawn(move || {
                    while let Ok(event) = receiver.recv_timeout(Duration::from_secs(2)) {
                        match event {
                            ServiceEvent::ServiceResolved(info) => {
                                let ip = info
                                    .get_addresses()
                                    .iter()
                                    .next()
                                    .map(|a| a.to_string())
                                    .unwrap_or_default();
                                if let Some(pkt) = packet_from_mdns_info(&info, ip) {
                                    sink(pkt, String::new());
                                }
                            }
                            ServiceEvent::SearchStopped(_) => break,
                            _ => {}
                        }
                    }
                })
                .ok();
        }
    }

    pub fn stop_browse(&self) {
        let mut b = self.browse_stopper.lock().unwrap();
        if b.take().is_some() {
            let _ = self.daemon.stop_browse(MDNS_SERVICE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_roundtrip() {
        let mut p = DiscoveryPacket::new("NODE-TEST".into(), "POS-TEST".into());
        p.is_coordinator = true;
        p.term = 9;
        p.shop_id = "SHOP-1".into();
        let v = serde_json::to_vec(&p).unwrap();
        let q: DiscoveryPacket = serde_json::from_slice(&v).unwrap();
        assert!(q.is_valid());
        assert_eq!(q.node_id, "NODE-TEST");
        assert_eq!(q.term, 9);
        assert!(q.is_coordinator);
    }

    #[test]
    fn invalid_magic_rejected() {
        let mut p = DiscoveryPacket::new("NODE-X".into(), "PC".into());
        p.magic = "EVIL".into();
        assert!(!p.is_valid());
    }

    #[test]
    fn txt_roundtrip() {
        let mut p = DiscoveryPacket::new("NODE-T".into(), "PC-T".into());
        p.term = 4;
        p.data_rows = 77;
        let txt = txt_from_packet(&p);
        let daemon_info_txt = txt;
        let map = daemon_info_txt;
        assert_eq!(map.get("node").unwrap(), "NODE-T");
        assert_eq!(map.get("term").unwrap(), "4");
        assert_eq!(map.get("data").unwrap(), "77");
    }
}
