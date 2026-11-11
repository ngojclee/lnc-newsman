//! Small helpers shared by provider tests: a loopback server with request counting and
//! an in-memory app state. No external network.

use crate::{AppState, config, connectors::Connector, fetch::Fetcher};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub type Counter = Arc<AtomicUsize>;

#[allow(dead_code)]
pub fn state_with(fetcher: Fetcher) -> Arc<AppState> {
    let dir = std::env::temp_dir().join(format!(
        "lnc-newsman-provider-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Arc::new(AppState::new(
        config::Config {
            data_dir: dir.clone(),
            regions_dir: dir.join("regions"),
            bind_address: "127.0.0.1".parse().unwrap(),
            port: 8000,
            management_key: Some("test-management-key".into()),
            api_keys: vec!["test-api-key".into()],
            allowed_hosts: vec!["newsman.test".into()],
            trusted_proxies: Vec::new(),
            secure_cookie: true,
            version: "test",
        },
        crate::db::Database::in_memory().unwrap(),
        fetcher,
    ))
}

/// A loopback server serving static bodies at exact paths, counting every request.
pub async fn counting_server(routes: Vec<(&'static str, &'static str)>) -> (String, Counter) {
    let counter: Counter = Arc::new(AtomicUsize::new(0));
    let mut app = axum::Router::new();
    for (path, body) in routes {
        let counter = counter.clone();
        app = app.route(
            path,
            axum::routing::get(move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    (axum::http::StatusCode::OK, body)
                }
            }),
        );
    }
    serve(app, counter).await
}

/// Same, but answers with a chosen status code (for 401/403/429 cases).
#[allow(dead_code)]
pub async fn status_server(path: &'static str, status: u16) -> (String, Counter) {
    let counter: Counter = Arc::new(AtomicUsize::new(0));
    let counter_inner = counter.clone();
    let app = axum::Router::new().route(
        path,
        axum::routing::get(move || {
            let counter = counter_inner.clone();
            async move {
                counter.fetch_add(1, Ordering::Relaxed);
                axum::http::StatusCode::from_u16(status).unwrap()
            }
        }),
    );
    serve(app, counter).await
}

async fn serve(app: axum::Router, counter: Counter) -> (String, Counter) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), counter)
}

/// Registers a connector enabled in `region`, with the cache window off so a second run
/// in a test is controlled by the test itself.
pub fn enable(db: &crate::db::Database, connector: &dyn Connector, region: &str) {
    crate::connectors::upsert(db, connector, region).unwrap();
    db.conn
        .execute(
            "UPDATE connectors SET enabled=1, cache_seconds=0 WHERE id=?1",
            [connector.id()],
        )
        .unwrap();
}
