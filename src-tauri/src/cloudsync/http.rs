//! Thin Supabase client over the existing `reqwest` dependency.
//! PostgREST + GoTrue + RPCs, no new crates. The anon key travels as
//! apikey + Authorization bearer (user JWT) — RLS scopes everything.

use serde_json::{json, Value};
use std::time::Duration;

pub struct SupabaseClient {
    pub url: String,      // https://<project>.supabase.co
    pub anon_key: String,
    pub access_token: String, // user JWT (GoTrue)
    client: reqwest::blocking::Client,
}

impl SupabaseClient {
    pub fn new(url: &str, anon_key: &str, access_token: &str) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            anon_key: anon_key.to_string(),
            access_token: access_token.to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
        }
    }

    fn headers(&self, prefer: Option<&str>) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        let _ = h.insert("apikey", self.anon_key.parse().unwrap());
        let _ = h.insert(
            "Authorization",
            format!("Bearer {}", self.access_token).parse().unwrap(),
        );
        if let Some(p) = prefer {
            let _ = h.insert("Prefer", p.parse().unwrap());
        }
        h
    }

    /// POST /rest/v1/rpc/{fn} — call a Postgres function.
    pub fn rpc(&self, fn_name: &str, params: Value) -> Result<Value, String> {
        let resp = self
            .client
            .post(format!("{}/rest/v1/rpc/{}", self.url, fn_name))
            .headers(self.headers(None))
            .json(&params)
            .send()
            .map_err(|e| format!("network: {e}"))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .map_err(|e| format!("bad json from {fn_name}: {e}"))?;
        if !status.is_success() {
            return Err(rpc_error(fn_name, status, &body));
        }
        Ok(body)
    }

    /// GET /rest/v1/{table}?query — select rows (embedded resources allowed
    /// via the select param, e.g. `*, client:clients(name)`).
    pub fn select(
        &self,
        table: &str,
        select: &str,
        query: &[(&str, String)],
    ) -> Result<Vec<Value>, String> {
        let resp = self
            .client
            .get(format!("{}/rest/v1/{}", self.url, table))
            .headers(self.headers(None))
            .query(&[("select", select)])
            .query(query)
            .send()
            .map_err(|e| format!("network: {e}"))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .map_err(|e| format!("bad json from {table}: {e}"))?;
        if !status.is_success() {
            return Err(rpc_error(table, status, &body));
        }
        match body {
            Value::Array(rows) => Ok(rows),
            other => Ok(vec![other]),
        }
    }

    /// POST /rest/v1/{table} — insert rows, return them.
    pub fn insert(
        &self,
        table: &str,
        rows: Value,
    ) -> Result<Vec<Value>, String> {
        let resp = self
            .client
            .post(format!("{}/rest/v1/{}", self.url, table))
            .headers(self.headers(Some("return=representation")))
            .json(&rows)
            .send()
            .map_err(|e| format!("network: {e}"))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .map_err(|e| format!("bad json from {table}: {e}"))?;
        if !status.is_success() {
            return Err(rpc_error(table, status, &body));
        }
        match body {
            Value::Array(rows) => Ok(rows),
            Value::Null => Ok(vec![]),
            other => Ok(vec![other]),
        }
    }

    /// PATCH /rest/v1/{table}?query — update matching rows.
    pub fn patch(
        &self,
        table: &str,
        row: Value,
        query: &[(&str, String)],
    ) -> Result<(), String> {
        let resp = self
            .client
            .patch(format!("{}/rest/v1/{}", self.url, table))
            .headers(self.headers(None))
            .query(query)
            .json(&row)
            .send()
            .map_err(|e| format!("network: {e}"))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let body: Value = resp.json().unwrap_or(Value::Null);
        Err(rpc_error(table, status, &body))
    }

    /// DELETE /rest/v1/{table}?query — remove matching rows.
    pub fn delete(&self, table: &str, query: &[(&str, String)]) -> Result<(), String> {
        let resp = self
            .client
            .delete(format!("{}/rest/v1/{}", self.url, table))
            .headers(self.headers(None))
            .query(query)
            .send()
            .map_err(|e| format!("network: {e}"))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let body: Value = resp.json().unwrap_or(Value::Null);
        Err(rpc_error(table, status, &body))
    }

    /// Invoke an Edge Function (invite-user, manage-user).
    pub fn function(&self, name: &str, body: Value) -> Result<Value, String> {
        let resp = self
            .client
            .post(format!("{}/functions/v1/{}", self.url, name))
            .headers(self.headers(None))
            .json(&body)
            .send()
            .map_err(|e| format!("network: {e}"))?;
        let status = resp.status();
        let body: Value = resp
            .json()
            .map_err(|e| format!("bad json from function {name}: {e}"))?;
        if !status.is_success() {
            return Err(rpc_error(name, status, &body));
        }
        Ok(body)
    }
}

/// Extract a readable error from a PostgREST/GoTrue error body:
/// {"code":"PGRST202","message":"...","details":...,"hint":...}.
fn rpc_error(ctx: &str, status: reqwest::StatusCode, body: &Value) -> String {
    let code = body["code"].as_str().unwrap_or("");
    let msg = body["message"]
        .as_str()
        .or_else(|| body["error_description"].as_str())
        .or_else(|| body["msg"].as_str())
        .unwrap_or("unknown error");
    if code.is_empty() {
        format!("{ctx} failed (HTTP {status}): {msg}")
    } else {
        format!("{ctx} failed [{code}]: {msg}")
    }
}

/// Convenience for building an rpc params object.
pub fn params(p: Value) -> Value {
    p
}

#[allow(unused)]
pub fn p(v: Value) -> Value {
    json!(v)
}
