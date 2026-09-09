//! Machine-bound secret storage: the CRM password is AES-256-GCM encrypted
//! with a key derived (Argon2id) from this PC's HWID — a copied database file
//! is useless on another machine. Ciphertext = base64(nonce ‖ ciphertext).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;

/// Constant pepper; the entropy comes from the machine HWID (salt input).
const PEPPER: &[u8] = b"titaouposcrm-secret-v1";
const NONCE_LEN: usize = 12;

/// Machine key material (HWID) — cached; stable per PC.
fn key_material() -> String {
    crate::services::settings_service::get_hwid()
}

fn derive_key(salt: &[u8]) -> Result<[u8; 32], String> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(PEPPER, salt, &mut key)
        .map_err(|e| format!("key derivation failed: {e}"))?;
    Ok(key)
}

// ── minimal standard base64 (no external crate) ────────────────────────────

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut buf = 0u32;
    let mut bits = 0u32;
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    for c in input.bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' {
            continue;
        }
        let v = B64
            .iter()
            .position(|&t| t == c)
            .ok_or_else(|| "bad base64".to_string())? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

// ── encrypt / decrypt ──────────────────────────────────────────────────────

pub fn encrypt(plain: &str) -> Result<String, String> {
    let key = derive_key(key_material().as_bytes())?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;

    use rand_core::RngCore;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand_core::OsRng.fill_bytes(&mut nonce_bytes);

    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plain.as_bytes())
        .map_err(|e| format!("encrypt failed: {e}"))?;

    let mut packed = Vec::with_capacity(NONCE_LEN + ct.len());
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&ct);
    Ok(b64_encode(&packed))
}

pub fn decrypt(packed_b64: &str) -> Result<String, String> {
    let packed = b64_decode(packed_b64)?;
    if packed.len() <= NONCE_LEN {
        return Err("ciphertext too short".into());
    }
    let (nonce_bytes, ct) = packed.split_at(NONCE_LEN);
    let key = derive_key(key_material().as_bytes())?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let plain = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|_| "decrypt failed (wrong machine or corrupted secret)")?;
    String::from_utf8(plain).map_err(|_| "decrypt produced invalid utf-8".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let enc = encrypt("S3cret-Passw0rd!").unwrap();
        assert_ne!(enc, "S3cret-Passw0rd!");
        assert_eq!(decrypt(&enc).unwrap(), "S3cret-Passw0rd!");
    }

    #[test]
    fn unique_nonces() {
        let a = encrypt("same").unwrap();
        let b = encrypt("same").unwrap();
        assert_ne!(a, b, "random nonce must change ciphertext");
        assert_eq!(decrypt(&a).unwrap(), "same");
        assert_eq!(decrypt(&b).unwrap(), "same");
    }

    #[test]
    fn b64_roundtrip() {
        for len in [0usize, 1, 2, 3, 12, 31, 64] {
            let data: Vec<u8> = (0..len as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&data)).unwrap(), data);
        }
    }

    #[test]
    fn b64_known_vectors() {
        assert_eq!(b64_encode(b"any carnal pleas"), "YW55IGNhcm5hbCBwbGVhcw==");
        assert_eq!(b64_encode(b"any carnal pleasu"), "YW55IGNhcm5hbCBwbGVhc3U=");
        assert_eq!(b64_encode(b"any carnal pleasur"), "YW55IGNhcm5hbCBwbGVhc3Vy");
        assert_eq!(b64_decode("YW55IGNhcm5hbCBwbGVhcw==").unwrap(), b"any carnal pleas");
    }

    #[test]
    fn rejects_garbage() {
        assert!(decrypt("not-base64!!").is_err());
        let short = b64_encode(&[1, 2, 3]);
        assert!(decrypt(&short).is_err(), "too short to contain a nonce");
    }
}
