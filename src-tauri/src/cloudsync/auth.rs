//! GoTrue (Supabase Auth) password sign-in + token refresh. The POS signs in
//! once with the owner's admin account; RLS then scopes every call to the
//! org. The service-role key never ships with the app.

use super::http::SupabaseClient;
use serde_json::{json, Value};
use std::time::Duration;

pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds when access_token expires (GoTrue default: 1 hour).
    pub expires_at: i64,
}

impl AuthSession {
    pub fn expired(&self) -> bool {
        chrono::Utc::now().timestamp() + 60 >= self.expires_at
    }
}

/// POST /auth/v1/token?grant_type=password → session.
pub fn sign_in(
    url: &str,
    anon_key: &str,
    email: &str,
    password: &str,
) -> Result<AuthSession, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("{}/auth/v1/token?grant_type=password", url.trim_end_matches('/')))
        .header("apikey", anon_key)
        .json(&json!({ "email": email, "password": password }))
        .send()
        .map_err(|e| format!("network: {e}"))?;
    let status = resp.status();
    let body: Value = resp.json().map_err(|e| format!("bad json: {e}"))?;
    if !status.is_success() {
        let msg = body["error_description"]
            .as_str()
            .or_else(|| body["msg"].as_str())
            .or_else(|| body["message"].as_str())
            .unwrap_or("invalid credentials");
        return Err(format!("login failed: {msg}"));
    }
    parse_session(&body)
}

/// POST /auth/v1/token?grant_type=refresh_token → fresh session.
pub fn refresh(
    url: &str,
    anon_key: &str,
    refresh_token: &str,
) -> Result<AuthSession, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("{}/auth/v1/token?grant_type=refresh_token", url.trim_end_matches('/')))
        .header("apikey", anon_key)
        .json(&json!({ "refresh": refresh_token }))
        .send()
        .map_err(|e| format!("network: {e}"))?;
    let status = resp.status();
    let body: Value = resp.json().map_err(|e| format!("bad json: {e}"))?;
    if !status.is_success() {
        let msg = body["error_description"]
            .as_str()
            .or_else(|| body["msg"].as_str())
            .unwrap_or("session expired");
        return Err(format!("refresh failed: {msg}"));
    }
    parse_session(&body)
}

fn parse_session(body: &Value) -> Result<AuthSession, String> {
    let access = body["access_token"].as_str().ok_or("no access_token")?;
    let refresh = body["refresh_token"].as_str().ok_or("no refresh_token")?;
    let expires_in = body["expires_in"].as_i64().unwrap_or(3600);
    Ok(AuthSession {
        access_token: access.to_string(),
        refresh_token: refresh.to_string(),
        expires_at: chrono::Utc::now().timestamp() + expires_in,
    })
}

/// The signed-in user's profile row (id, role, organization_id, full_name,
/// is_active). Cloud sync requires role = admin — the POS is the owner's
/// station. Mirrors the Flutter auth gate (deactivated users are blocked).
pub fn fetch_profile(client: &SupabaseClient) -> Result<Value, String> {
    let rows = client.select(
        "profiles",
        "id, full_name, role, organization_id, is_active, email",
        &[("id", format!("eq.{}", profile_id_from_jwt(client)))],
    )?;
    rows.into_iter()
        .next()
        .ok_or_else(|| "profile row not found".to_string())
}

/// Extract the sub (user id) claim from the client's own JWT (no base64 dep —
/// the JWT payload is always the middle dot-separated segment, standard
/// base64url). For the profile lookup only; the signature is verified by
/// Supabase itself on every request.
fn profile_id_from_jwt(client: &SupabaseClient) -> String {
    let token = &client.access_token;
    let payload_b64 = token.split('.').nth(1).unwrap_or("");
    // base64url → base64, pad.
    let mut b64: String = payload_b64.replace('-', "+").replace('_', "/");
    while b64.len() % 4 != 0 {
        b64.push('=');
    }
    let bytes = b64_decode(&b64).unwrap_or_default();
    let payload: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    payload["sub"].as_str().unwrap_or("").to_string()
}

/// Minimal standard-base64 decoder (needed only for the JWT sub claim).
fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c == b'=' {
            break;
        }
        let v = TABLE
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64_roundtrip() {
        // eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9 → {"alg":"HS256","typ":"JWT"}
        let decoded = b64_decode("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9").unwrap();
        let s = String::from_utf8(decoded).unwrap();
        assert!(s.contains("\"alg\""));
        assert!(s.contains("HS256"));
    }

    #[test]
    fn b64_rejects_garbage() {
        assert!(b64_decode("!!not-base64!!").is_err());
    }
}
