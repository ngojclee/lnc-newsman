//! Secret-store tests. Everything is in memory or a temporary file; no network.

use super::*;
use crate::{AppState, config, fetch};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use rusqlite::params;
use std::sync::Arc;
use tower::ServiceExt;

fn box_with(current: u8, previous: Option<u8>) -> SecretBox {
    SecretBox {
        current: [current; KEY_BYTES],
        previous: previous.map(|value| [value; KEY_BYTES]),
    }
}

#[test]
fn round_trip_uses_a_fresh_nonce_every_write() {
    let vault = box_with(7, None);
    let db = db::Database::in_memory().unwrap();
    set(&db, &vault, "fred-api-key", "super-secret-value").unwrap();
    let first: String = db
        .conn
        .query_row(
            "SELECT ciphertext FROM secrets WHERE name='fred-api-key'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        !first.contains("super-secret-value"),
        "plaintext must never be stored"
    );
    assert_eq!(
        get(&db, &vault, "fred-api-key").unwrap(),
        "super-secret-value"
    );

    set(&db, &vault, "fred-api-key", "super-secret-value").unwrap();
    let second: String = db
        .conn
        .query_row(
            "SELECT ciphertext FROM secrets WHERE name='fred-api-key'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(
        first, second,
        "re-encrypting the same value must use a new nonce"
    );
    // Both ciphertexts still decrypt: nonce reuse would break authentication.
    assert_eq!(
        vault.open("fred-api-key", &first).unwrap(),
        "super-secret-value"
    );
    assert_eq!(
        vault.open("fred-api-key", &second).unwrap(),
        "super-secret-value"
    );
}

#[test]
fn a_ciphertext_cannot_be_moved_to_another_name() {
    let vault = box_with(3, None);
    let db = db::Database::in_memory().unwrap();
    set(&db, &vault, "one", "value").unwrap();
    let _sealed: String = db
        .conn
        .query_row("SELECT ciphertext FROM secrets WHERE name='one'", [], |r| {
            r.get(0)
        })
        .unwrap();
    db.conn
        .execute("UPDATE secrets SET name='two' WHERE name='one'", params![])
        .unwrap();
    assert_eq!(
        get(&db, &vault, "two"),
        Err(SecretError::Undecryptable),
        "the name is bound as associated data"
    );
}

#[test]
fn a_wrong_key_cannot_open_the_value() {
    let db = db::Database::in_memory().unwrap();
    set(&db, &box_with(1, None), "token", "abc").unwrap();
    assert_eq!(
        get(&db, &box_with(2, None), "token"),
        Err(SecretError::Undecryptable)
    );
}

#[test]
fn rewrap_rotates_with_a_new_nonce_and_keeps_the_plaintext() {
    let mut db = db::Database::in_memory().unwrap();
    let old = box_with(1, None);
    set(&db, &old, "provider-key", "rotated-value").unwrap();
    let before: String = db
        .conn
        .query_row(
            "SELECT ciphertext FROM secrets WHERE name='provider-key'",
            [],
            |r| r.get(0),
        )
        .unwrap();

    // Rotation: the previous key is now `LNC_MASTER_KEY_OLD`.
    let rotated = box_with(2, Some(1));
    assert_eq!(rewrap(&mut db, &rotated).unwrap(), 1);
    let after: String = db
        .conn
        .query_row(
            "SELECT ciphertext FROM secrets WHERE name='provider-key'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(before, after, "rewrap must reseal with a new nonce");
    assert_eq!(get(&db, &rotated, "provider-key").unwrap(), "rotated-value");
    assert_eq!(
        get(&db, &box_with(2, None), "provider-key").unwrap(),
        "rotated-value",
        "after rewrap the previous key is no longer needed"
    );
    assert_eq!(
        get(&db, &box_with(1, None), "provider-key"),
        Err(SecretError::Undecryptable),
        "the old key alone no longer opens the row"
    );
}

#[test]
fn a_failed_rewrap_leaves_every_row_untouched() {
    let mut db = db::Database::in_memory().unwrap();
    set(&db, &box_with(1, None), "good", "value").unwrap();
    // A row sealed by a key we do not have (for example a restored backup).
    db.conn
        .execute(
            "INSERT INTO secrets(name,ciphertext,key_version) VALUES ('foreign',?1,9)",
            [base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                [0u8; 48],
            )],
        )
        .unwrap();
    let before: Vec<(String, String)> = {
        let mut stmt = db
            .conn
            .prepare("SELECT name,ciphertext FROM secrets ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    assert!(rewrap(&mut db, &box_with(2, Some(1))).is_err());
    let after: Vec<(String, String)> = {
        let mut stmt = db
            .conn
            .prepare("SELECT name,ciphertext FROM secrets ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    assert_eq!(before, after, "a failed rotation must not rewrite anything");
}

#[test]
fn key_parsing_accepts_hex_and_base64_only() {
    let hex = "0".repeat(64);
    let base64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [9u8; KEY_BYTES]);
    assert_eq!(parse_key(&hex), Some([0u8; KEY_BYTES]));
    assert_eq!(parse_key(&base64), Some([9u8; KEY_BYTES]));
    for rejected in ["", "short", &"0".repeat(63), &"z".repeat(64), "  "] {
        assert_eq!(parse_key(rejected), None, "{rejected:?}");
    }
}

#[test]
fn names_and_sizes_are_validated() {
    let vault = box_with(1, None);
    let db = db::Database::in_memory().unwrap();
    for name in ["", "Upper", "with space", "-lead", "trail-", "a--b"] {
        assert_eq!(
            set(&db, &vault, name, "value"),
            Err(SecretError::Invalid),
            "{name:?}"
        );
    }
    assert_eq!(set(&db, &vault, "ok-name", ""), Err(SecretError::Invalid));
    let too_big = "x".repeat(MAX_SECRET_BYTES + 1);
    assert_eq!(
        set(&db, &vault, "ok-name", &too_big),
        Err(SecretError::Invalid)
    );
}

struct TempDir(std::path::PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lnc-newsman-secrets-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn state_with_vault(vault: Option<SecretBox>) -> (SharedState, TempDir) {
    let dir = TempDir::new();
    let mut app_state = AppState::new(
        config::Config {
            data_dir: dir.0.clone(),
            regions_dir: dir.0.join("regions"),
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 8000,
            management_key: Some("test-management-key".into()),
            api_keys: vec!["test-api-key".into()],
            allowed_hosts: vec!["newsman.test".into()],
            trusted_proxies: Vec::new(),
            secure_cookie: true,
            version: "test",
        },
        db::Database::in_memory().unwrap(),
        fetch::Fetcher::public_only(),
    );
    app_state.secrets = vault;
    (Arc::new(app_state), dir)
}

async fn cookie(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header(header::HOST, "newsman.test")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-requested-with", "XMLHttpRequest")
                .body(Body::from(r#"{"key":"test-management-key"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

fn call(
    method: &str,
    path: &str,
    body: &str,
    cookie: Option<&str>,
    api_key: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, "newsman.test")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-requested-with", "XMLHttpRequest");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(key) = api_key {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn the_api_never_returns_a_plaintext_secret() {
    let (state, _dir) = state_with_vault(Some(box_with(5, None)));
    let app = crate::router(state);
    let cookie = cookie(&app).await;
    let response = app
        .clone()
        .oneshot(call(
            "PUT",
            "/api/admin/secrets/coin-api",
            r#"{"value":"plaintext-should-not-leak"}"#,
            Some(&cookie),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(!String::from_utf8_lossy(&body).contains("plaintext-should-not-leak"));
    let response = app
        .clone()
        .oneshot(call("GET", "/api/admin/secrets", "", Some(&cookie), None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&body).to_string();
    assert!(text.contains("coin-api"));
    assert!(!text.contains("plaintext-should-not-leak"));
    assert!(!text.contains("ciphertext"));
}

#[tokio::test]
async fn secrets_require_a_session_and_a_configured_key() {
    let (state, _dir) = state_with_vault(Some(box_with(5, None)));
    let app = crate::router(state);
    let response = app
        .clone()
        .oneshot(call(
            "PUT",
            "/api/admin/secrets/coin-api",
            r#"{"value":"x"}"#,
            None,
            Some("test-api-key"),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "an API key cannot write secrets"
    );

    let (unconfigured, _dir2) = state_with_vault(None);
    let app = crate::router(unconfigured);
    let cookie = cookie(&app).await;
    let response = app
        .clone()
        .oneshot(call("GET", "/api/admin/secrets", "", Some(&cookie), None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
