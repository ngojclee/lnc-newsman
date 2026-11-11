//! Documents module tests: insert, idempotent refresh, summary preservation, region
//! visibility. In-memory database; no network.

use super::*;
use crate::{AppState, config, fetch};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

fn state() -> SharedState {
    let dir = std::env::temp_dir().join(format!(
        "lnc-newsman-docs-{}-{}",
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
        db::Database::in_memory().unwrap(),
        fetch::Fetcher::public_only(),
    ))
}

fn doc(number: &str, region: &str) -> DocumentRow {
    DocumentRow {
        provider: "federal-register".into(),
        region: region.into(),
        doc_type: "rule".into(),
        agency: "Environmental Protection Agency".into(),
        number: number.into(),
        title: "Example rule".into(),
        effective_on: Some("2027-01-15".into()),
        published_on: Some("2026-12-01".into()),
        url: "https://www.federalregister.gov/d/2026-12345".into(),
        summary: None,
        topic: String::new(),
        disclaimer: String::new(),
    }
}

#[test]
fn upsert_is_idempotent_and_keeps_a_hand_written_summary() {
    let state = state();
    let db = state.db.lock().unwrap();
    assert!(upsert(&db, &doc("2026-12345", "global")).unwrap());
    assert!(
        !upsert(&db, &doc("2026-12345", "global")).unwrap(),
        "a repeated refresh must not look like a new document"
    );
    assert_eq!(count(&db, "federal-register").unwrap(), 1);
    db.conn
        .execute(
            "UPDATE documents SET summary=?1 WHERE number='2026-12345'",
            ["Bản tóm tắt do người viết."],
        )
        .unwrap();
    let mut refreshed = doc("2026-12345", "global");
    refreshed.title = "Example rule (updated)".into();
    upsert(&db, &refreshed).unwrap();
    let stored = get(&db, "federal-register", "2026-12345").unwrap().unwrap();
    assert_eq!(stored.title, "Example rule (updated)");
    assert_eq!(
        stored.summary.as_deref(),
        Some("Bản tóm tắt do người viết."),
        "a refresh must never overwrite a human summary"
    );
}

#[test]
fn invalid_documents_are_refused() {
    let state = state();
    let db = state.db.lock().unwrap();
    let mut missing_number = doc("", "global");
    assert!(upsert(&db, &missing_number).is_err());
    missing_number.number = "x".into();
    missing_number.url = "not a url".into();
    assert!(upsert(&db, &missing_number).is_err());
    let unknown_region = doc("ok", "nowhere");
    assert!(upsert(&db, &unknown_region).is_err());
}

#[tokio::test]
async fn the_reader_only_sees_documents_of_enabled_regions() {
    let state = state();
    {
        let db = state.db.lock().unwrap();
        db.conn
            .execute(
                "INSERT INTO regions(code,parent_code,label_vi,label_en,enabled) VALUES ('us',NULL,'US','US',0)",
                [],
            )
            .unwrap();
        upsert(&db, &doc("2026-1", "global")).unwrap();
        let mut us = doc("2026-2", "us");
        us.provider = "federal-register".into();
        upsert(&db, &us).unwrap();
    }
    let app = crate::router(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/documents")
                .header(header::HOST, "newsman.test")
                .header(header::AUTHORIZATION, "Bearer test-api-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let numbers: Vec<_> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["number"].as_str().unwrap())
        .collect();
    assert_eq!(
        numbers,
        ["2026-1"],
        "documents of a disabled region stay hidden"
    );
}

#[test]
fn folded_search_finds_vietnamese_with_or_without_diacritics() {
    let state = state();
    let db = state.db.lock().unwrap();
    let mut tax_doc = doc("2026-77777", "global");
    tax_doc.title = "Hướng dẫn khai thuế giá trị gia tăng".into();
    tax_doc.summary = Some("Bản tóm tắt tự viết về thuế GTGT.".into());
    tax_doc.topic = "tax".into();
    upsert(&db, &tax_doc).unwrap();
    // Unaccented query finds the accented title.
    let hits = search(&db, "thue", 10).unwrap();
    assert_eq!(hits.len(), 1, "folded search must match without diacritics");
    // Accented query and a prefix both work.
    assert_eq!(search(&db, "khai thuế", 10).unwrap().len(), 1);
    assert_eq!(search(&db, "gia", 10).unwrap().len(), 1);
    // An unrelated term matches nothing, and an empty query is not an error.
    assert!(search(&db, "unrelated", 10).unwrap().is_empty());
    assert!(search(&db, "   ", 10).unwrap().is_empty());
}

#[test]
fn a_summary_always_carries_the_disclaimer_and_a_client_cannot_drop_it() {
    let state = state();
    let db = state.db.lock().unwrap();
    let mut summary_doc = doc("2026-88888", "global");
    summary_doc.summary = Some("Bản tóm tắt tự viết.".into());
    // A caller tries to supply something else; the server writes its own constant.
    summary_doc.disclaimer = "Custom text".into();
    upsert(&db, &summary_doc).unwrap();
    let stored = get(&db, "federal-register", "2026-88888").unwrap().unwrap();
    assert_eq!(stored.disclaimer, SUMMARY_DISCLAIMER);
    assert!(stored.disclaimer.contains("Không phải tư vấn pháp lý/thuế"));
    // A document without a summary carries no disclaimer, so it cannot imply one.
    let bare = doc("2026-99999", "global");
    upsert(&db, &bare).unwrap();
    assert_eq!(
        get(&db, "federal-register", "2026-99999")
            .unwrap()
            .unwrap()
            .disclaimer,
        ""
    );
}

#[test]
fn folding_only_removes_diacritics() {
    assert_eq!(fold_diacritics("Thuế GTGT"), "Thue GTGT");
    // Upper case "Đ" folds to lower "d": the search index is case-insensitive anyway.
    assert_eq!(fold_diacritics("Đăng ký"), "dang ky");
    assert_eq!(fold_diacritics("already plain"), "already plain");
}
