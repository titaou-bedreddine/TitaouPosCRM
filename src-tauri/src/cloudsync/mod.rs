//! Cloud sync orchestrator — the ONLY loop that talks to Supabase, running
//! exclusively on the LAN coordinator (clients forward their business ops to
//! the coordinator, so there is exactly one cloud writer).
//!
//! States: disabled → connecting → online → error (recoverable). Push first,
//! then pull (LWW: POS edits win for POS-origin rows). Cycle every ~5s while
//! online; config lives in app_settings under cloud_* keys (LOCAL_ONLY —
//! never LAN-forwarded).

pub mod auth;
pub mod cloud_cmds;
pub mod http;
pub mod mapping;
pub mod outbox;
pub mod pull;
pub mod push;
pub mod secrets;

use crate::database::DbState;
use http::SupabaseClient;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

pub struct CloudState {
    pub online: AtomicBool,
    pub last_push: Mutex<Option<String>>,
    pub last_pull: Mutex<Option<String>>,
    pub last_error: Mutex<Option<String>>,
    pub last_cycle_counts: Mutex<(usize, usize, usize)>, // pushed, failed, retried
    pub org_id: Mutex<Option<String>>,
    pub session: Mutex<Option<auth::AuthSession>>, // refreshable session
    pub config: Mutex<CloudConfig>,
}

#[derive(Clone, Default)]
pub struct CloudConfig {
    pub url: String,
    pub anon_key: String,
    pub email: String,
    pub refresh_token: String,
    /// AES-GCM(HWID)-encrypted CRM password — persisted so a non-technical
    /// customer never re-types credentials. Empty = not stored.
    pub password_enc: String,
    pub enabled: bool,
}

impl CloudState {
    fn new() -> Self {
        Self {
            online: AtomicBool::new(false),
            last_push: Mutex::new(None),
            last_pull: Mutex::new(None),
            last_error: Mutex::new(None),
            last_cycle_counts: Mutex::new((0, 0, 0)),
            org_id: Mutex::new(None),
            session: Mutex::new(None),
            config: Mutex::new(CloudConfig::default()),
        }
    }
}

static STATE: OnceLock<CloudState> = OnceLock::new();

pub fn state() -> &'static CloudState {
    STATE.get_or_init(CloudState::new)
}

/// Load cloud_* settings from the DB into the state (startup + after
/// configure). Returns whether cloud sync is configured + enabled.
pub fn load_config(db: &DbState) -> bool {
    let cfg = CloudConfig {
        url: get_setting(db, "cloud_url"),
        anon_key: get_setting(db, "cloud_anon_key"),
        email: get_setting(db, "cloud_email"),
        refresh_token: get_setting(db, "cloud_refresh_token"),
        password_enc: get_setting(db, "cloud_password_enc"),
        enabled: get_setting(db, "cloud_enabled") == "true",
    };
    let configured = !cfg.url.is_empty() && !cfg.anon_key.is_empty() && !cfg.email.is_empty();
    let enabled = cfg.enabled;
    *state().config.lock().unwrap() = cfg;
    configured && enabled
}

fn get_setting(db: &DbState, key: &str) -> String {
    let conn = db.conn.lock().unwrap();
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        rusqlite::params![key],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_default()
}

/// Build a client with a fresh-enough access token: refresh when the stored
/// session is near expiry. Sign in fresh when there's no session.
pub fn ensure_session() -> Result<SupabaseClient, String> {
    let cfg = state().config.lock().unwrap().clone();
    if cfg.url.is_empty() || cfg.anon_key.is_empty() {
        return Err("cloud sync not configured".into());
    }

    let mut session_slot = state().session.lock().unwrap();
    let needs_fresh = match session_slot.as_ref() {
        Some(s) => s.expired(),
        None => true,
    };
    if needs_fresh {
        // Renewal order: refresh token → stored (encrypted) password.
        // The password path means a customer NEVER re-types credentials —
        // only when the password was rotated on the server does the UI ask.
        let stored_password = || -> Result<String, String> {
            if cfg.password_enc.is_empty() {
                return Err("no stored password — connect in Settings → Cloud Sync".into());
            }
            secrets::decrypt(&cfg.password_enc)
        };
        let session = match session_slot.as_ref() {
            Some(s) => auth::refresh(&cfg.url, &cfg.anon_key, &s.refresh_token)
                .or_else(|_| stored_password().and_then(|pw| auth::sign_in(&cfg.url, &cfg.anon_key, &cfg.email, &pw))),
            None => stored_password()
                .and_then(|pw| auth::sign_in(&cfg.url, &cfg.anon_key, &cfg.email, &pw)),
        }?;
        *session_slot = Some(session);
    }

    let session = session_slot.as_ref().unwrap();
    Ok(SupabaseClient::new(
        &cfg.url,
        &cfg.anon_key,
        &session.access_token,
    ))
}

/// One full sync cycle: ensure session → verify admin profile → push → pull.
/// Called by the background loop (5s) and by cloud_sync_now.
pub fn cycle(db: &DbState) -> Result<Value, String> {
    let client = ensure_session()?;

    // Gate: the POS must be an admin of the org (deactivated → stop).
    let profile = auth::fetch_profile(&client)?;
    let role = profile["role"].as_str().unwrap_or("");
    let is_active = profile["is_active"].as_bool().unwrap_or(false);
    if role != "admin" || !is_active {
        state().online.store(false, Ordering::SeqCst);
        return Err(format!("CRM account is not an active admin (role: {role})"));
    }
    let org = profile["organization_id"].as_str().unwrap_or_default().to_string();
    if org.is_empty() {
        return Err("CRM profile has no organization".into());
    }
    *state().org_id.lock().unwrap() = Some(org.clone());

    // Push first (LWW: POS-origin edits win ties), then pull.
    let mut pushed_total = 0;
    let mut failed_total = 0;
    let mut retried_total = 0;
    loop {
        let mut conn = db.conn.lock().unwrap();
        let (p, f, r) = push::drain(&mut conn, &client, 100)?;
        pushed_total += p;
        failed_total += f;
        retried_total += r;
        if p == 0 || retried_total > 0 {
            break; // queue empty, or a transient error paused the batch
        }
    }
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if retried_total == 0 {
        *state().last_push.lock().unwrap() = Some(now.clone());
    }

    let pull_errors = {
        let mut conn = db.conn.lock().unwrap();
        pull::pull_cycle(&mut conn, &client, &org)
    };
    *state().last_pull.lock().unwrap() = Some(now.clone());

    state().online.store(pull_errors.is_empty(), Ordering::SeqCst);
    let error = pull_errors
        .iter()
        .map(|(s, e)| format!("{s}: {e}"))
        .chain(std::iter::once(format!("push retries: {retried_total}")).filter(|_| retried_total > 0))
        .collect::<Vec<_>>()
        .join("; ");
    if !error.is_empty() {
        *state().last_error.lock().unwrap() = Some(error);
    } else {
        *state().last_error.lock().unwrap() = None;
    }
    *state().last_cycle_counts.lock().unwrap() = (pushed_total, failed_total, retried_total);

    Ok(serde_json::json!({
        "pushed": pushed_total,
        "failed": failed_total,
        "retried": retried_total,
        "online": state().online.load(Ordering::SeqCst),
    }))
}

/// Background loop entry — spawned from lib.rs when config loads enabled.
pub fn spawn_loop(db: std::sync::Arc<DbState>) {
    std::thread::spawn(move || loop {
        if !load_config(&db) {
            state().online.store(false, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_secs(10));
            continue;
        }
        if let Err(e) = cycle(&db) {
            *state().last_error.lock().unwrap() = Some(e);
            state().online.store(false, Ordering::SeqCst);
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
    });
}

/// Status snapshot for the UI (cloud_get_status).
pub fn status(db: &DbState) -> Value {
    let pending = {
        let conn = db.conn.lock().unwrap();
        outbox::pending_count(&conn)
    };
    let failed = {
        let conn = db.conn.lock().unwrap();
        outbox::failed_count(&conn)
    };
    let (pushed, failed_c, retried) = *state().last_cycle_counts.lock().unwrap();
    serde_json::json!({
        "enabled": load_config(db),
        "online": state().online.load(Ordering::SeqCst),
        "url": state().config.lock().unwrap().url,
        "email": state().config.lock().unwrap().email,
        "org_id": state().org_id.lock().unwrap().clone(),
        "pending_outbox": pending,
        "failed_outbox": failed,
        "last_push": state().last_push.lock().unwrap().clone(),
        "last_pull": state().last_pull.lock().unwrap().clone(),
        "last_error": state().last_error.lock().unwrap().clone(),
        "last_pushed": pushed,
        "last_failed": failed_c,
        "last_retried": retried,
    })
}

/// Are cloud commands available at all? Only meaningful on the coordinator —
/// a LAN client terminal shows "runs on the server terminal" instead.
pub fn is_coordinator() -> bool {
    crate::network::is_server_or_standalone()
}
