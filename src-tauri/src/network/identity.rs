//! Permanent node/shop identity for the LAN shop network.
//!
//! The Node ID is generated once per installation and never changes (the PC
//! name is just a friendly label). IDs are stored in `app_settings` so a
//! backup/restore carries them along with the rest of the shop.

use crate::services::settings_service;
use std::collections::HashMap;

/// Random hex string (2 chars per byte) from the OS CSPRNG.
pub fn random_hex(bytes: usize) -> String {
    use rand_core::RngCore;
    let mut buf = vec![0u8; bytes];
    rand_core::OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{:02X}", b)).collect()
}

pub fn generate_node_id() -> String {
    format!("NODE-{}", random_hex(6))
}

pub fn generate_shop_id() -> String {
    format!("SHOP-{}", random_hex(6))
}

/// Stable default PC name derived from the machine + node suffix, e.g.
/// "POS-3F2A". The user renames it in the wizard; uniqueness inside the shop
/// is best-effort (the Node ID is the real identity).
pub fn default_pc_name(node_id: &str) -> String {
    let suffix = node_id.strip_prefix("NODE-").unwrap_or(node_id);
    let suffix = if suffix.len() > 4 { &suffix[suffix.len() - 4..] } else { suffix };
    let host = std::env::var("COMPUTERNAME").unwrap_or_default();
    if host.trim().is_empty() {
        format!("POS-{}", suffix)
    } else {
        format!("{} ({})", host.trim(), suffix)
    }
}

/// Load-or-create the persistent network identity from the local DB.
pub fn load_identity(db: &crate::database::DbState) -> Identity {
    let settings = settings_service::get_all_settings(db).unwrap_or_default();
    let get = |k: &str| settings.get(k).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    let node_id = get("net_node_id").unwrap_or_else(|| {
        let id = generate_node_id();
        let _ = settings_service::set_setting(db, "net_node_id", &id);
        id
    });
    let pc_name = get("net_pc_name").unwrap_or_else(|| default_pc_name(&node_id));
    Identity { node_id, pc_name }
}

#[derive(Debug, Clone)]
pub struct Identity {
    pub node_id: String,
    pub pc_name: String,
}

/// Read a HashMap<String,String> settings map with trims applied.
pub fn trim_map_get(map: &HashMap<String, String>, key: &str) -> Option<String> {
    map.get(key).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_shape() {
        let id = generate_node_id();
        assert!(id.starts_with("NODE-"));
        assert_eq!(id.len(), "NODE-".len() + 12);
    }

    #[test]
    fn shop_id_shape() {
        let id = generate_shop_id();
        assert!(id.starts_with("SHOP-"));
    }

    #[test]
    fn random_hex_is_hex_and_unique() {
        let a = random_hex(8);
        let b = random_hex(8);
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn default_pc_name_uses_suffix() {
        let n = default_pc_name(&generate_node_id());
        assert!(n.starts_with("POS-") || n.contains('('));
    }
}
