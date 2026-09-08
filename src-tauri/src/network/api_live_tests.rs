//! LIVE end-to-end tests for the LAN shop API: a real Axum server on
//! 127.0.0.1:<random> with a real SQLite file, driven through real HTTP —
//! health → join → login → whitelisted invokes → permission denials →
//! per-terminal register isolation.

#![cfg(test)]

use crate::database::DbState;
use crate::network::server_api::{self, SharedServerState};
use crate::server::ServerState;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn open_db_at(path: &std::path::Path) -> DbState {
    let conn = Connection::open(path).expect("open test db");
    conn.execute_batch(
        "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;",
    )
    .unwrap();
    let state = DbState { conn: Mutex::new(conn) };
    state.run_migrations().unwrap();
    state.seed_default_admin().unwrap();
    state
}

fn unique_db_path(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("titaou_api_tests");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!(
        "{}_{}.sqlite",
        tag,
        crate::network::identity::random_hex(4)
    ))
}

/// Boot the /api/v1 router on a random local port; returns the base URL.
fn spawn_server() -> String {
    let state: SharedServerState = Arc::new(ServerState {
        port: 0,
        started_at: Instant::now(),
        devices: Mutex::new(std::collections::HashMap::new()),
    });
    let app = server_api::v1_router().with_state(state);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    // tokio requires a nonblocking socket before from_std.
    listener
        .set_nonblocking(true)
        .expect("set nonblocking");
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let _ = axum::serve(
                tokio::net::TcpListener::from_std(listener).unwrap(),
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await;
        });
    });
    format!("http://127.0.0.1:{}", port)
}

fn http() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

#[test]
fn live_api_join_login_invoke_and_permissions() {
    let db_path = unique_db_path("live_api");
    let db_a = open_db_at(&db_path); // network runtime handle
    let db_b = open_db_at(&db_path); // API handle (separate connection, same file)

    // Shop identity + server mode for the test runtime.
    crate::network::init_for_tests(db_a);
    crate::network::set_shop_for_tests("SHOP-TEST1", "Test Market");
    server_api::set_api_db(db_b);

    let base = spawn_server();
    let client = http();

    // Wait for the listener.
    let mut health: Option<Value> = None;
    for _ in 0..40 {
        if let Ok(r) = client.get(format!("{}/api/v1/health", base)).send() {
            if let Ok(v) = r.json::<Value>() {
                health = Some(v);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    let health = health.expect("server did not come up");
    assert_eq!(health["status"], "ok");
    assert_eq!(health["shop_id"], "SHOP-TEST1");
    assert_eq!(health["app"], crate::network::discovery::SHOP_KIND);
    assert!(health["term"].as_u64().unwrap() >= 1);

    // ISOLATION: a join from a different application family (e.g. the old
    // TitaouPOS) must be refused — it can never merge into this shop.
    {
        let foreign: Value = client
            .post(format!("{}/api/v1/network/join", base))
            .json(&json!({ "node_id": "NODE-TITAOUPOS", "pc_name": "Old POS", "shop_id": "SHOP-TEST1", "app": "titaoupos", "confirm": true }))
            .send()
            .unwrap()
            .json()
            .unwrap();
        assert_eq!(foreign["ok"], false, "foreign app must be refused");
        assert!(foreign["error"].as_str().unwrap().contains("Different application"), "unexpected error: {foreign}");
    }

    // Join (confirmation step first — no registration yet).
    let probe: Value = client
        .post(format!("{}/api/v1/network/join", base))
        .json(&json!({ "node_id": "NODE-CLI1", "pc_name": "POS-CLI1", "shop_id": "SHOP-TEST1", "app": crate::network::discovery::SHOP_KIND, "confirm": false }))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(probe["confirm_required"], true);
    assert_eq!(probe["shop_name"], "Test Market");

    // A join naming a DIFFERENT shop than the one this server owns must be
    // rejected: the client is expected to learn the real shop_id from the
    // announce/health before joining (fresh installs adopt the server's
    // shop id at this step — see tick_client in mod.rs).
    let wrong = client
        .post(format!("{}/api/v1/network/join", base))
        .json(&json!({ "node_id": "NODE-CLI2", "pc_name": "X", "shop_id": "SHOP-OTHER", "app": crate::network::discovery::SHOP_KIND, "confirm": true }))
        .send()
        .unwrap();
    assert_eq!(wrong.status(), reqwest::StatusCode::FORBIDDEN);

    // Confirm the join → device token.
    let joined: Value = client
        .post(format!("{}/api/v1/network/join", base))
        .json(&json!({ "node_id": "NODE-CLI1", "pc_name": "POS-CLI1", "role_pref": "client", "app_version": "0.0.0", "shop_id": "SHOP-TEST1", "app": crate::network::discovery::SHOP_KIND, "confirm": true }))
        .send()
        .unwrap()
        .json()
        .unwrap();
    let device_token = joined["device_token"].as_str().expect("device token").to_string();

    // Pre-login surface works with ONLY the device token.
    let users: Value = client
        .post(format!("{}/api/v1/invoke/get_active_users", base))
        .header("Authorization", format!("Bearer {}", device_token))
        .json(&json!({}))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(users["ok"], true);
    assert!(users["result"].as_array().unwrap().iter().any(|u| u["username"] == "admin"));

    // Business operations are refused before login.
    let denied = client
        .post(format!("{}/api/v1/invoke/list_customers", base))
        .header("Authorization", format!("Bearer {}", device_token))
        .json(&json!({}))
        .send()
        .unwrap();
    assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Login (reuses the existing users table).
    let login: Value = client
        .post(format!("{}/api/v1/auth/login", base))
        .header("Authorization", format!("Bearer {}", device_token))
        .json(&json!({ "username": "admin", "password": "admin" }))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(login["ok"], true);
    assert_eq!(login["user"]["username"], "admin");
    let user_token = login["token"].as_str().unwrap().to_string();

    let wrong_login = client
        .post(format!("{}/api/v1/auth/login", base))
        .header("Authorization", format!("Bearer {}", device_token))
        .json(&json!({ "username": "admin", "password": "nope" }))
        .send()
        .unwrap();
    assert_eq!(wrong_login.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Business op: save a customer over the API (camelCase arg names, like
    // the Tauri frontend sends).
    let saved: Value = client
        .post(format!("{}/api/v1/invoke/save_customer", base))
        .header("Authorization", format!("Bearer {}", user_token))
        .json(&json!({ "name": "Test Client API", "phone": "0555000111", "initialDebt": 250 }))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(saved["ok"], true, "save_customer failed: {}", saved);
    let customer_id = saved["result"].as_i64().unwrap();

    let list: Value = client
        .post(format!("{}/api/v1/invoke/list_customers", base))
        .header("Authorization", format!("Bearer {}", user_token))
        .json(&json!({}))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert!(list["result"].as_array().unwrap().iter().any(|c| c["id"] == customer_id && c["name"] == "Test Client API"));

    // Local-only operations must never be whitelisted.
    let hw = client
        .post(format!("{}/api/v1/invoke/print_html_direct", base))
        .header("Authorization", format!("Bearer {}", user_token))
        .json(&json!({ "html": "<p>x</p>", "title": "t" }))
        .send()
        .unwrap();
    assert_eq!(hw.status(), reqwest::StatusCode::NOT_FOUND);

    // Per-terminal session isolation: two terminals get different registers.
    // (Session commands are user-level: log in on each device first.)
    let open1: Value = client
        .post(format!("{}/api/v1/invoke/open_cash_session", base))
        .header("Authorization", format!("Bearer {}", user_token))
        .json(&json!({ "userId": 1, "registerId": 1, "openingAmount": 1000 }))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(open1["ok"], true, "open1: {}", open1);
    let reg1 = open1["result"]["register_id"].clone();

    let joined2: Value = client
        .post(format!("{}/api/v1/network/join", base))
        .json(&json!({ "node_id": "NODE-CLI3", "pc_name": "POS-CLI3", "shop_id": "SHOP-TEST1", "app": crate::network::discovery::SHOP_KIND, "confirm": true }))
        .send()
        .unwrap()
        .json()
        .unwrap();
    let token3 = joined2["device_token"].as_str().unwrap().to_string();
    let login3: Value = client
        .post(format!("{}/api/v1/auth/login", base))
        .header("Authorization", format!("Bearer {}", token3))
        .json(&json!({ "username": "admin", "password": "admin" }))
        .send()
        .unwrap()
        .json()
        .unwrap();
    let user_token3 = login3["token"].as_str().unwrap().to_string();
    let open2: Value = client
        .post(format!("{}/api/v1/invoke/open_cash_session", base))
        .header("Authorization", format!("Bearer {}", user_token3))
        .json(&json!({ "userId": 1, "registerId": 1, "openingAmount": 500 }))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(open2["ok"], true, "open2: {}", open2);
    let reg2 = open2["result"]["register_id"].clone();
    // The two terminals must NOT share one register — and opening one
    // terminal's session must not close the other's.
    assert_ne!(reg1, reg2);
    let s1: Value = client
        .post(format!("{}/api/v1/invoke/get_active_cash_session", base))
        .header("Authorization", format!("Bearer {}", user_token))
        .json(&json!({ "userId": 1 }))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(s1["ok"], true, "s1: {}", s1);
    assert_eq!(s1["result"]["register_id"], reg1, "terminal 1 keeps its own session");

    let devices = server_api::devices_snapshot();
    assert!(devices.iter().any(|d| d["pc_name"] == "POS-CLI1"));
    assert!(devices.iter().any(|d| d["pc_name"] == "POS-CLI3"));

    let _ = std::fs::remove_file(&db_path);
}
