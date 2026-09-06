//! Deterministic coordinator election for the LAN shop network.
//!
//! Priority (highest first), all derived from one announce snapshot so every
//! node computes the SAME answer:
//!   1. explicit Server role wins,
//!   2. then the richer local database (protects existing shops from being
//!      shadowed by a fresh install),
//!   3. then the lowest Node ID (final deterministic tie-break).
//! Clients never become coordinator. A coordinator claim carries a term;
//! claims with a lower term than one we already accepted are stale and are
//! ignored (split-brain / returning-old-server protection).

use serde::{Deserialize, Serialize};

pub const ROLE_SERVER: &str = "server";
pub const ROLE_CLIENT: &str = "client";
pub const ROLE_AUTOMATIC: &str = "automatic";

/// What one node knows about another (or itself) at election time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub node_id: String,
    pub pc_name: String,
    /// Preferred startup behavior.
    pub role_pref: String,
    /// True when the peer is currently acting as coordinator for its shop.
    pub is_coordinator: bool,
    pub shop_id: String,
    pub shop_name: String,
    pub term: u64,
    pub http_port: u16,
    pub ip: String,
    pub app_version: String,
    /// Sales+products+customers row estimate — "who owns the real data".
    pub data_rows: u64,
    pub last_seen_ms: u64,
}

impl Peer {
    /// Coordinator candidacy rank. Higher wins. Explicit server first, then
    /// data richness, then lowest node id.
    pub fn candidacy(&self) -> (u8, u64, std::cmp::Reverse<String>) {
        let role_rank = match self.role_pref.as_str() {
            ROLE_SERVER => 2u8,
            ROLE_AUTOMATIC => 1u8,
            _ => 0u8,
        };
        (
            role_rank,
            self.data_rows,
            std::cmp::Reverse(self.node_id.clone()),
        )
    }
}

/// Pick the coordinator among alive peers (the caller filters freshness,
/// including self). Returns None when only clients are present.
pub fn elect_coordinator(peers: &[&Peer]) -> Option<Peer> {
    peers
        .iter()
        .filter(|p| p.role_pref != ROLE_CLIENT)
        .copied()
        .max_by_key(|p| p.candidacy())
        .cloned()
}

/// A coordinator claim we may accept must not be stale: its term must be at
/// least the highest term we have ever accepted for leadership information.
pub fn claim_is_fresh(claim_term: u64, highest_seen_term: u64) -> bool {
    claim_term >= highest_seen_term
}

/// The next term for a node taking over coordination: strictly greater than
/// anything it has ever seen or held itself.
pub fn next_term(highest_seen_term: u64) -> u64 {
    highest_seen_term.saturating_add(1)
}

/// Decide this node's effective part in the network given what it sees.
/// `self_peer` must be present in `alive` too. Returns the coordinator peer
/// (possibly self) or None (only clients around → remain standalone).
pub fn resolve_role(self_peer: &Peer, alive: &[Peer]) -> Resolution {
    let refs: Vec<&Peer> = alive.iter().collect();
    match elect_coordinator(&refs) {
        None => Resolution::Standalone,
        Some(c) => {
            if c.node_id == self_peer.node_id {
                Resolution::Coordinate(c)
            } else if self_peer.role_pref == ROLE_SERVER && c.role_pref != ROLE_SERVER {
                // Explicit Server never follows an automatic node; the
                // candidate beat us only on data — still, explicit Server
                // has rank 2 so this branch is unreachable via candidacy.
                // Kept as a safety invariant.
                Resolution::Coordinate(self_peer.clone())
            } else {
                Resolution::Follow(c)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Resolution {
    /// No eligible coordinator visible: act as our own authority.
    Standalone,
    /// I am the coordinator (with my winning claim).
    Coordinate(Peer),
    /// Another node is the coordinator; connect to it.
    Follow(Peer),
}

/// Split-brain guard: when two different nodes claim coordination for the
/// same shop, the one with the LOWER candidacy must stand down.
pub fn should_stand_down(self_peer: &Peer, rival: &Peer) -> bool {
    self_peer.node_id != rival.node_id && rival.candidacy() > self_peer.candidacy()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(node: &str, role: &str, data: u64, coord: bool) -> Peer {
        Peer {
            node_id: node.to_string(),
            pc_name: node.to_string(),
            role_pref: role.to_string(),
            is_coordinator: coord,
            shop_id: "SHOP-1".into(),
            shop_name: "S".into(),
            term: 1,
            http_port: 8080,
            ip: "192.168.1.9".into(),
            app_version: "0.0.0".into(),
            data_rows: data,
            last_seen_ms: 0,
        }
    }

    #[test]
    fn explicit_server_beats_everything() {
        let a = peer("NODE-A", ROLE_AUTOMATIC, 9999, false);
        let b = peer("NODE-B", ROLE_SERVER, 0, false);
        let c = peer("NODE-C", ROLE_AUTOMATIC, 5000, false);
        let alive = vec![a, b.clone(), c];
        let r = resolve_role(&b, &alive);
        match r {
            Resolution::Coordinate(p) => assert_eq!(p.node_id, "NODE-B"),
            other => panic!("expected self coordination, got {:?}", other),
        }
    }

    #[test]
    fn richer_db_wins_among_automatics() {
        let a = peer("NODE-A", ROLE_AUTOMATIC, 5000, false); // smaller id, less data
        let b = peer("NODE-B", ROLE_AUTOMATIC, 20000, false);
        let alive = vec![a.clone(), b.clone()];
        // From A's point of view: B coordinates.
        match resolve_role(&a, &alive) {
            Resolution::Follow(c) => assert_eq!(c.node_id, "NODE-B"),
            other => panic!("expected follow, got {:?}", other),
        }
        // From B's point of view: B coordinates.
        match resolve_role(&b, &alive) {
            Resolution::Coordinate(c) => assert_eq!(c.node_id, "NODE-B"),
            other => panic!("expected coordinate, got {:?}", other),
        }
    }

    #[test]
    fn lowest_node_id_breaks_ties() {
        let a = peer("NODE-A", ROLE_AUTOMATIC, 100, false);
        let b = peer("NODE-B", ROLE_AUTOMATIC, 100, false);
        let alive = vec![a.clone(), b.clone()];
        match resolve_role(&b, &alive) {
            Resolution::Follow(c) => assert_eq!(c.node_id, "NODE-A"),
            other => panic!("expected follow, got {:?}", other),
        }
    }

    #[test]
    fn clients_never_coordinate() {
        let a = peer("NODE-A", ROLE_CLIENT, 0, false);
        match resolve_role(&a, &[a.clone()]) {
            Resolution::Standalone => {}
            other => panic!("clients must not coordinate, got {:?}", other),
        }
    }

    #[test]
    fn stale_term_claims_rejected() {
        assert!(claim_is_fresh(18, 17));
        assert!(claim_is_fresh(17, 17));
        assert!(!claim_is_fresh(16, 17));
        assert_eq!(next_term(17), 18);
    }

    #[test]
    fn split_brain_stand_down() {
        // Data-rich rival beats me even though I also claim coordination.
        let me = peer("NODE-Z", ROLE_AUTOMATIC, 10, true);
        let rival = peer("NODE-A", ROLE_AUTOMATIC, 999, true);
        assert!(should_stand_down(&me, &rival));
        assert!(!should_stand_down(&rival, &me));
    }
}
