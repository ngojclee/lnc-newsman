#[cfg(test)]
mod tests {
    use crate::{
        AppState, agent_bridge, auth, collector, config, connector, connectors, db, hub, llm,
        oidc, regions, router, secrets, security,
    };
    use axum::{
        Json, Router,
        body::Body,
        extract::State as AxumState,
        http::{Request, StatusCode, header},
        routing::{get, post},
    };
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce, aead::Payload};
    use http_body_util::BodyExt;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, jwk::JwkSet};
    use serde::Deserialize;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::{
        collections::HashSet,
        net::SocketAddr,
        sync::{Arc, Mutex, atomic::AtomicBool},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use tower::ServiceExt;

    fn test_now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs()
    }

    fn state_with_vault(oidc_cfg: oidc::OidcConfig) -> Arc<AppState> {
        let db = db::Database::in_memory().unwrap();
        let master = [0x42u8; 32];
        let prev = [0x24u8; 32];
        let vault = secrets::SecretBox::from_bytes(master, Some(prev));
        let agent_bridge = agent_bridge::AgentBridgeRuntime::new(&db);
        let cfg = config::Config {
            data_dir: ".".into(),
            regions_dir: ".".into(),
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 8000,
            management_key: Some("bootstrap-env-mgmt-key-123456".into()),
            api_keys: vec!["env-api-key-123456".into()],
            allowed_hosts: vec!["newsman.test".into(), "127.0.0.1".into()],
            trusted_proxies: Vec::new(),
            secure_cookie: true,
            version: "test",
        };
        let _ = db.ensure_bootstrap_management_key(cfg.management_key.as_deref(), test_now());
        let mut sec = security::SecurityRuntime::for_test(&cfg);
        sec.oidc = oidc_cfg;
        Arc::new(AppState {
            cfg,
            db: Mutex::new(db),
            login_limiter: Mutex::new(auth::LoginLimiter::default()),
            started_at: test_now(),
            fetcher: crate::fetch::Fetcher::public_only(),
            in_flight: Mutex::new(HashSet::new()),
            connector_in_flight: Mutex::new(HashSet::new()),
            host_gates: Mutex::new(std::collections::HashMap::new()),
            host_gap: collector::HOST_GAP,
            secrets: Some(vault),
            connector_config: connectors::ConnectorConfig::default(),
            llm: llm::LlmConfig {
                base_url: None,
                model: "mock".into(),
            },
            hub: hub::ProviderConfig::default(),
            telegram: crate::telegram::TelegramConfig::default(),
            connector: connector::ConnectorConfig::default(),
            connector_nonces: Mutex::new(std::collections::HashMap::new()),
            llm_enabled: false,
            pack: Mutex::new(regions::PackStatus::default()),
            alerts_running: AtomicBool::new(false),
            agent_bridge,
            security: Arc::new(sec),
        })
    }

    fn json_req(
        method: &str,
        path: &str,
        body: &str,
        cookie: Option<&str>,
        bearer: Option<&str>,
        csrf: bool,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "newsman.test")
            .header(header::CONTENT_TYPE, "application/json")
            .extension(axum::extract::ConnectInfo(
                "127.0.0.1:41000".parse::<SocketAddr>().unwrap(),
            ));
        if let Some(c) = cookie {
            builder = builder.header(header::COOKIE, c);
        }
        if let Some(b) = bearer {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {b}"));
        }
        if csrf {
            builder = builder.header("x-requested-with", "XMLHttpRequest");
        }
        builder.body(Body::from(body.to_owned())).unwrap()
    }

    async fn read_json(resp: axum::response::Response) -> (StatusCode, header::HeaderMap, Value) {
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let val = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
        (status, headers, val)
    }

    async fn admin_cookie(app: &Router, key: &str) -> String {
        let resp = app
            .clone()
            .oneshot(json_req(
                "POST",
                "/api/auth/login",
                &json!({ "key": key }).to_string(),
                None,
                None,
                true,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        resp.headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    #[test]
    fn secrets_legacy_xchacha_zero_loss_and_aes_gcm_aad() {
        let state = state_with_vault(oidc::OidcConfig::default());
        let vault = state.secrets.as_ref().unwrap();

        // 1. Legacy pre-Blueprint XChaCha20-Poly1305 encrypted secret with raw LNC_MASTER_KEY_OLD
        let legacy_key = [0x24u8; 32];
        let legacy_nonce = [0x99u8; 24];
        let cipher = XChaCha20Poly1305::new((&legacy_key).into());
        let ct = chacha20poly1305::aead::Aead::encrypt(
            &cipher,
            XNonce::from_slice(&legacy_nonce),
            Payload {
                msg: b"legacy-hub-bearer-token-987",
                aad: b"legacy-key",
            },
        )
        .unwrap();
        let mut blob = Vec::new();
        blob.extend_from_slice(&legacy_nonce);
        blob.extend_from_slice(&ct);
        let encoded = base64::engine::general_purpose::STANDARD.encode(blob);
        {
            let db = state.db.lock().unwrap();
            db.conn
                .execute(
                    "INSERT INTO secrets(name, ciphertext, key_version, updated_at) VALUES (?1, ?2, 1, '2026-01-01T00:00:00Z')",
                    rusqlite::params!["legacy-key", encoded],
                )
                .unwrap();
            let opened = secrets::get(&db, vault, "legacy-key").unwrap();
            assert_eq!(opened, "legacy-hub-bearer-token-987");
        }

        // 2. Blueprint AES-256-GCM envelope with HKDF vault_kek & record AAD isolation
        let env_cipher = vault
            .seal_envelope("api_key:ak-1", "nm_key_secret_payload_abc")
            .unwrap();
        assert!(env_cipher.starts_with("aesgcm:v1:"));
        assert_eq!(
            vault.open_envelope("api_key:ak-1", &env_cipher).unwrap(),
            "nm_key_secret_payload_abc"
        );
        assert!(vault.open_envelope("api_key:ak-2", &env_cipher).is_err());

        // 3. Key-ring fallback across LNC_MASTER_KEY_OLD -> LNC_MASTER_KEY
        let old_vault = secrets::SecretBox::from_bytes([0x24u8; 32], None);
        let old_env = old_vault.seal_envelope("provider:p1", "sk-old-secret").unwrap();
        assert_eq!(
            vault.open_envelope("provider:p1", &old_env).unwrap(),
            "sk-old-secret"
        );
    }

    #[tokio::test]
    async fn management_key_argon2id_rotation_and_session_epoch_revocation() {
        let state = state_with_vault(oidc::OidcConfig::default());
        {
            let db = state.db.lock().unwrap();
            let rt = db.runtime_auth_state().unwrap();
            assert!(rt.mgmt_key_hash.starts_with("$argon2id$v=19$m=65536,t=3,p=4$"));
            assert_eq!(rt.mgmt_source, "env");
        }

        let app = router(state.clone());
        let old_cookie = admin_cookie(&app, "bootstrap-env-mgmt-key-123456").await;

        // Settings accessible while session is live
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req("GET", "/api/settings", "", Some(&old_cookie), None, false))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);

        // Rotate MANAGEMENT_KEY
        let (st, hdrs, body) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    "/api/admin/mgmt/rotate",
                    r#"{"new_key":"rotated-mgmt-key-super-999"}"#,
                    Some(&old_cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["source"], "runtime");
        assert!(hdrs.get(header::CACHE_CONTROL).unwrap().to_str().unwrap().contains("no-store"));

        // Old cookie is immediately invalidated by session_epoch bump
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req("GET", "/api/settings", "", Some(&old_cookie), None, false))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        // Old bootstrap env key must fail closed
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    "/api/auth/login",
                    r#"{"key":"bootstrap-env-mgmt-key-123456"}"#,
                    None,
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        // Re-running bootstrap on restart must NOT overwrite runtime-rotated hash
        {
            let db = state.db.lock().unwrap();
            db.ensure_bootstrap_management_key(Some("bootstrap-env-mgmt-key-123456"), test_now())
                .unwrap();
            assert_eq!(db.runtime_auth_state().unwrap().mgmt_source, "runtime");
        }

        // Rotated key succeeds, then revoke-all nukes it instantly
        let new_cookie = admin_cookie(&app, "rotated-mgmt-key-super-999").await;
        let (st, _, rev) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    "/api/admin/sessions/revoke-all",
                    "{}",
                    Some(&new_cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(rev["revoked_all"], true);
        let (st, _, _) = read_json(
            app.oneshot(json_req("GET", "/api/settings", "", Some(&new_cookie), None, false))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn blueprint_session_idle_and_max_lifetime_enforced() {
        let db = db::Database::in_memory().unwrap();
        let now_ts = test_now();
        let tok_idle = auth::token_hash("tok-idle-check");
        db.create_blueprint_session(&tok_idle, now_ts + 10, now_ts + 100, 1, "oidc", "louis", now_ts)
            .unwrap();
        // Valid before idle expiry; extends idle window by 10s (to now_ts + 15)
        assert!(db.validate_and_touch_session(&tok_idle, now_ts + 5, 10).unwrap().is_some());
        // Idle expired past extended window
        assert!(db.validate_and_touch_session(&tok_idle, now_ts + 16, 10).unwrap().is_none());

        let tok_max = auth::token_hash("tok-max-check");
        db.create_blueprint_session(&tok_max, now_ts + 50, now_ts + 100, 1, "oidc", "louis", now_ts)
            .unwrap();
        // Expired past max_expires_at
        assert!(db.validate_and_touch_session(&tok_max, now_ts + 101, 50).unwrap().is_none());
    }

    #[tokio::test]
    async fn oidc_pkce_s256_loopback_mock_and_local_only_logout() {
        // Deterministic RFC 8032 Ed25519 PKCS#8 v1 fixture ([7u8; 32] seed -> no PEM/secret literal)
        let mut pkcs8 = vec![
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        pkcs8.extend_from_slice(&[7u8; 32]);
        let enc_key = EncodingKey::from_ed_der(&pkcs8);
        let jwks_json = json!({
            "keys": [{
                "kty": "OKP",
                "crv": "Ed25519",
                "kid": "test-ed-1",
                "alg": "EdDSA",
                "use": "sig",
                "x": "6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw"
            }]
        });
        let jwk_set: JwkSet = serde_json::from_value(jwks_json.clone()).unwrap();

        #[derive(Clone)]
        struct MockIdp {
            base_url: Arc<Mutex<String>>,
            pending: Arc<Mutex<Option<(String, String, String)>>>, // (code_challenge, nonce, user)
            enc_key: Arc<EncodingKey>,
            jwks: Value,
            end_session_hits: Arc<Mutex<usize>>,
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://127.0.0.1:{}", addr.port());
        let idp = MockIdp {
            base_url: Arc::new(Mutex::new(base_url.clone())),
            pending: Arc::new(Mutex::new(None)),
            enc_key: Arc::new(enc_key),
            jwks: jwks_json,
            end_session_hits: Arc::new(Mutex::new(0)),
        };

        #[derive(Deserialize)]
        struct TokenReq {
            code: String,
            code_verifier: String,
            client_id: String,
        }

        let idp_router = Router::new()
            .route(
                "/.well-known/openid-configuration",
                get(|AxumState(st): AxumState<MockIdp>| async move {
                    let b = st.base_url.lock().unwrap().clone();
                    Json(json!({
                        "issuer": b,
                        "authorization_endpoint": format!("{b}/authorize"),
                        "token_endpoint": format!("{b}/token"),
                        "jwks_uri": format!("{b}/jwks"),
                        "end_session_endpoint": format!("{b}/logout")
                    }))
                }),
            )
            .route(
                "/jwks",
                get(|AxumState(st): AxumState<MockIdp>| async move { Json(st.jwks) }),
            )
            .route(
                "/logout",
                get(|AxumState(st): AxumState<MockIdp>| async move {
                    *st.end_session_hits.lock().unwrap() += 1;
                    StatusCode::OK
                }),
            )
            .route(
                "/token",
                post(
                    |AxumState(st): AxumState<MockIdp>, axum::extract::Form(form): axum::extract::Form<TokenReq>| async move {
                        let (expected_challenge, nonce, user) =
                            st.pending.lock().unwrap().take().expect("pending auth tx");
                        assert_eq!(form.code, "auth-code-1");
                        assert_eq!(form.client_id, "lnc-newsman");
                        let got_challenge =
                            URL_SAFE_NO_PAD.encode(Sha256::digest(form.code_verifier.as_bytes()));
                        assert_eq!(got_challenge, expected_challenge);
                        let now_ts = test_now() as usize;
                        let claims = json!({
                            "iss": *st.base_url.lock().unwrap(),
                            "aud": "lnc-newsman",
                            "sub": format!("sub-{user}"),
                            "preferred_username": user,
                            "nonce": nonce,
                            "exp": now_ts + 300,
                            "iat": now_ts
                        });
                        let mut hdr = Header::new(Algorithm::EdDSA);
                        hdr.kid = Some("test-ed-1".to_string());
                        let id_token = encode(&hdr, &claims, &st.enc_key).unwrap();
                        Json(json!({
                            "access_token": "mock-access",
                            "id_token": id_token,
                            "token_type": "Bearer"
                        }))
                    },
                ),
            )
            .with_state(idp.clone());

        tokio::spawn(async move {
            axum::serve(listener, idp_router).await.unwrap();
        });

        let oidc_cfg = oidc::OidcConfig {
            enabled: true,
            issuer_url: base_url.clone(),
            client_id: "lnc-newsman".into(),
            client_secret: "client-secret-test".into(),
            redirect_uri: "https://newsman.test/api/auth/oidc/callback".into(),
            allowed_users: vec!["louis".into()],
            allowed_hosts: vec!["127.0.0.1".into()],
        };
        let state = state_with_vault(oidc_cfg.clone());
        let app = router(state.clone());

        // Unit check on verify_id_token_claims: rejects HS256 & mismatched nonce
        assert_eq!(
            oidc::verify_id_token_claims("eyJhbGciOiJIUzI1NiJ9.e30.abc", &jwk_set, &[&base_url], "lnc-newsman", "n1")
                .err()
                .as_deref(),
            Some("oidc_disallowed_alg")
        );

        // 1. Allowed user (`louis`) full PKCE S256 round trip
        let login_resp = app
            .clone()
            .oneshot(json_req("GET", "/api/auth/oidc/login", "", None, None, false))
            .await
            .unwrap();
        assert_eq!(login_resp.status(), StatusCode::SEE_OTHER);
        let loc = login_resp.headers().get(header::LOCATION).unwrap().to_str().unwrap();
        let parsed = url::Url::parse(loc).unwrap();
        let qp: std::collections::HashMap<String, String> = parsed.query_pairs().into_owned().collect();
        assert_eq!(qp.get("code_challenge_method").map(String::as_str), Some("S256"));
        let tx_cookie = login_resp
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();

        *idp.pending.lock().unwrap() = Some((
            qp["code_challenge"].clone(),
            qp["nonce"].clone(),
            "louis".into(),
        ));
        let cb_uri = format!(
            "/api/auth/oidc/callback?code=auth-code-1&state={}",
            urlencoding_encode(&qp["state"])
        );
        let cb_resp = app
            .clone()
            .oneshot(json_req("GET", &cb_uri, "", Some(&tx_cookie), None, false))
            .await
            .unwrap();
        assert_eq!(cb_resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(cb_resp.headers().get(header::LOCATION).unwrap(), "/admin");
        let sess_cookie = cb_resp
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find(|v| v.starts_with("lnc_newsman_session="))
            .expect("session cookie minted")
            .split(';')
            .next()
            .unwrap()
            .to_string();

        // /api/auth/status reports authenticated + oidc user
        let (st, _, status_val) = read_json(
            app.clone()
                .oneshot(json_req("GET", "/api/auth/status", "", Some(&sess_cookie), None, false))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(status_val["authenticated"], true);
        assert_eq!(status_val["auth_method"], "oidc");
        assert_eq!(status_val["username"], "louis");

        // Local-only logout clears session and NEVER calls IdP end_session_endpoint
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    "/api/auth/logout",
                    "{}",
                    Some(&sess_cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(*idp.end_session_hits.lock().unwrap(), 0);

        // 2. Non-allowlisted user (`eve`) is denied and audited
        let login_resp2 = app
            .clone()
            .oneshot(json_req("GET", "/api/auth/oidc/login", "", None, None, false))
            .await
            .unwrap();
        let parsed2 = url::Url::parse(login_resp2.headers().get(header::LOCATION).unwrap().to_str().unwrap()).unwrap();
        let qp2: std::collections::HashMap<String, String> = parsed2.query_pairs().into_owned().collect();
        let tx_cookie2 = login_resp2.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().split(';').next().unwrap().to_string();
        *idp.pending.lock().unwrap() = Some((qp2["code_challenge"].clone(), qp2["nonce"].clone(), "eve".into()));
        let cb2 = app
            .clone()
            .oneshot(json_req(
                "GET",
                &format!("/api/auth/oidc/callback?code=auth-code-1&state={}", urlencoding_encode(&qp2["state"])),
                "",
                Some(&tx_cookie2),
                None,
                false,
            ))
            .await
            .unwrap();
        assert_eq!(cb2.status(), StatusCode::SEE_OTHER);
        assert!(
            cb2.headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("oidc_user_not_allowed")
        );

        // 3. Empty OIDC_ALLOWED_USERS fails closed
        let mut empty_allow = oidc_cfg;
        empty_allow.allowed_users.clear();
        assert!(empty_allow.user_allowed(&[Some("louis")]).is_none());
    }

    fn urlencoding_encode(input: &str) -> String {
        url::form_urlencoded::Serializer::new(String::new())
            .append_pair("v", input)
            .finish()
            .strip_prefix("v=")
            .unwrap_or(input)
            .to_string()
    }

    #[tokio::test]
    async fn security_headers_csrf_xfh_spoof_query_token_and_bearer_isolation() {
        let state = state_with_vault(oidc::OidcConfig::default());
        let app = router(state.clone());

        // 1. Pre-auth /api/auth/status exposes zero secrets and sets security + no-store headers
        let (st, hdrs, val) = read_json(
            app.clone()
                .oneshot(json_req("GET", "/api/auth/status", "", None, None, false))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(val["authenticated"], false);
        assert!(hdrs.get(header::CACHE_CONTROL).unwrap().to_str().unwrap().contains("no-store"));
        assert_eq!(hdrs.get("x-frame-options").unwrap(), "DENY");
        assert_eq!(hdrs.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(hdrs.get("referrer-policy").unwrap(), "same-origin");
        let csp = hdrs.get("content-security-policy").unwrap().to_str().unwrap();
        assert!(csp.contains("script-src 'self'"));
        assert!(csp.contains("frame-ancestors 'none'"));

        // 2. Reject query token leakage on any path
        for qp in ["token=secret", "key=secret", "api_key=secret", "access_token=secret"] {
            let (st, _, body) = read_json(
                app.clone()
                    .oneshot(json_req("GET", &format!("/api/v1/articles?{qp}"), "", None, Some("env-api-key-123456"), false))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(st, StatusCode::BAD_REQUEST, "query param {qp}");
            assert_eq!(body["error"], "query_token_disabled");
        }

        // 3. MANAGEMENT_KEY Bearer cannot authenticate /api/v1/* or /mcp
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req(
                    "GET",
                    "/api/v1/articles",
                    "",
                    None,
                    Some("bootstrap-env-mgmt-key-123456"),
                    false,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        // 4. Spoofed Origin + X-Forwarded-Host must return 403 Forbidden
        let cookie = admin_cookie(&app, "bootstrap-env-mgmt-key-123456").await;
        let spoof_req = Request::builder()
            .method("POST")
            .uri("/api/admin/api-keys")
            .header(header::HOST, "newsman.test")
            .header("x-forwarded-host", "evil.attacker")
            .header(header::ORIGIN, "https://evil.attacker")
            .header(header::COOKIE, &cookie)
            .header("x-requested-with", "XMLHttpRequest")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"label":"spoof"}"#))
            .unwrap();
        let (st, _, _) = read_json(app.clone().oneshot(spoof_req).await.unwrap()).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn api_keys_providers_cascade_ssrf_mcp_and_copy_rate_limit() {
        // Spin up local loopback mock HTTP server for OpenAI /v1/models and MCP JSON-RPC initialize + tools/list
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let mock_base = format!("http://127.0.0.1:{port}");
        let mock_srv = Router::new()
            .route(
                "/v1/models",
                get(|| async {
                    Json(json!({ "data": [{ "id": "gpt-5.4-mini" }, { "id": "gpt-5.4" }] }))
                }),
            )
            .route(
                "/mcp",
                post(|Json(v): Json<Value>| async move {
                    let m = v["method"].as_str().unwrap_or("");
                    if m == "initialize" {
                        Json(json!({ "jsonrpc": "2.0", "id": v["id"], "result": { "capabilities": {} } }))
                    } else {
                        Json(json!({ "jsonrpc": "2.0", "id": v["id"], "result": { "tools": [{ "name": "mock_tool" }] } }))
                    }
                }),
            );
        tokio::spawn(async move {
            axum::serve(listener, mock_srv).await.unwrap();
        });

        let state = state_with_vault(oidc::OidcConfig::default());
        let app = router(state.clone());
        let cookie = admin_cookie(&app, "bootstrap-env-mgmt-key-123456").await;
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req(
                    "PUT",
                    "/api/app-agents/bridge-config",
                    r#"{"allowed_hosts":["127.0.0.1"]}"#,
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);

        // 1. API Key Copy · Rotate · Revoke · Remove + 15/5m Copy Rate Limit
        let (st, _, created) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    "/api/admin/api-keys",
                    r#"{"label":"CI Reader","scopes":["newsman:read"]}"#,
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        let key_id = created["id"].as_str().unwrap().to_string();
        let secret1 = created["secret"].as_str().unwrap().to_string();

        // New key authenticates /api/v1/articles
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req("GET", "/api/v1/articles", "", None, Some(&secret1), false))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);

        // Copy endpoint returns secret with no-store & enforces 15/5m limit
        let (st, hdrs, copied) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    &format!("/api/admin/api-keys/{key_id}/copy"),
                    "{}",
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(copied["secret"], secret1);
        assert!(hdrs.get(header::CACHE_CONTROL).unwrap().to_str().unwrap().contains("no-store"));

        // Rotate immediately invalidates old secret
        let (st, _, rot) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    &format!("/api/admin/api-keys/{key_id}/rotate"),
                    "{}",
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let secret2 = rot["secret"].as_str().unwrap().to_string();
        assert_ne!(secret1, secret2);

        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req("GET", "/api/v1/articles", "", None, Some(&secret1), false))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req("GET", "/api/v1/articles", "", None, Some(&secret2), false))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);

        // Revoke + Remove
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    &format!("/api/admin/api-keys/{key_id}/revoke"),
                    "{}",
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req("GET", "/api/v1/articles", "", None, Some(&secret2), false))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req(
                    "DELETE",
                    &format!("/api/admin/api-keys/{key_id}"),
                    "",
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);

        // 2. Provider SSRF block + omitBlankSecrets + Cascade Delete
        let (st, _, ssrf) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    "/api/admin/providers",
                    r#"{"name":"Meta","base_url":"http://169.254.169.254/latest","api_key":"sk-bad"}"#,
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert_eq!(ssrf["error"], "ssrf_blocked_private_url");

        let (st, _, prov) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    "/api/admin/providers",
                    &json!({
                        "name": "Local OpenAI",
                        "base_url": format!("{mock_base}/v1"),
                        "api_key": "sk-original-12345678"
                    })
                    .to_string(),
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let prov_id = prov["id"].as_str().unwrap().to_string();

        // Update with masked/blank secret must preserve existing secret (omitBlankSecrets)
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req(
                    "PUT",
                    &format!("/api/admin/providers/{prov_id}"),
                    &json!({ "name": "Local OpenAI v2", "api_key": "********" }).to_string(),
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);

        let (st, _, copied_prov) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    &format!("/api/admin/providers/{prov_id}/copy"),
                    "{}",
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(copied_prov["secret"], "sk-original-12345678");

        // Fetch models + insert binding -> verify CASCADE/SET NULL on delete
        let (st, _, fetched) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    &format!("/api/admin/providers/{prov_id}/fetch-models"),
                    "{}",
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(fetched["fetched_count"], 2);
        {
            let db = state.db.lock().unwrap();
            db.conn
                .execute(
                    "INSERT INTO ai_consumer_bindings(role_key, provider_id, model_id, updated_at)
                     VALUES ('test_role', ?1, 'gpt-5.4-mini', '2026-01-01T00:00:00Z')",
                    [&prov_id],
                )
                .unwrap();
        }
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req(
                    "DELETE",
                    &format!("/api/admin/providers/{prov_id}"),
                    "",
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        {
            let db = state.db.lock().unwrap();
            let cat_count: i64 = db
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM ai_model_catalog WHERE provider_id = ?1",
                    [&prov_id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(cat_count, 0);
            let bind_prov: Option<String> = db
                .conn
                .query_row(
                    "SELECT provider_id FROM ai_consumer_bindings WHERE role_key = 'test_role'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(bind_prov, None);
        }

        // 3. MCP Inbound + Outbound live tools/list test
        let (st, _, in_test) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    "/api/admin/mcp-connectors/inbound-mcp/test",
                    "{}",
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert!(in_test["tool_count"].as_u64().unwrap() > 0);

        let (st, _, out_mcp) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    "/api/admin/mcp-connectors",
                    &json!({
                        "name": "Mock MCP",
                        "endpoint_url": format!("{mock_base}/mcp"),
                        "scopes": ["read"],
                        "bearer_token": "mcp-bearer-token-1234"
                    })
                    .to_string(),
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let mcp_id = out_mcp["id"].as_str().unwrap().to_string();

        let (st, _, mcp_probe) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    &format!("/api/admin/mcp-connectors/{mcp_id}/test"),
                    "{}",
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(mcp_probe["tool_count"], 1);

        // 4. Verify copy 15/5m/IP rate limit trips cleanly on 16th copy
        for i in 0..13 {
            let (st, _, _) = read_json(
                app.clone()
                    .oneshot(json_req(
                        "POST",
                        &format!("/api/admin/mcp-connectors/{mcp_id}/copy"),
                        "{}",
                        Some(&cookie),
                        None,
                        true,
                    ))
                    .await
                    .unwrap(),
            )
            .await;
            assert_eq!(st, StatusCode::OK, "copy attempt #{i}");
        }
        // 1 + 1 + 13 = 15 copies recorded above; 16th must 429
        let (st, _, _) = read_json(
            app.clone()
                .oneshot(json_req(
                    "POST",
                    &format!("/api/admin/mcp-connectors/{mcp_id}/copy"),
                    "{}",
                    Some(&cookie),
                    None,
                    true,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);

        // Audit log contains recorded lifecycle events
        let (st, _, audit) = read_json(
            app.oneshot(json_req("GET", "/api/admin/audit-events", "", Some(&cookie), None, false))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert!(!audit["items"].as_array().unwrap().is_empty());
    }
}
