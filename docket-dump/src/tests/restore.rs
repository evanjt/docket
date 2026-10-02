use std::collections::BTreeMap;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use tower::ServiceExt;

use docket_core::dump::{DumpPage, files};
use docket_migration::scratch::Scratch;
use docket_server::app;
use docket_server::auth::Keys;

use super::*;

const KEYS: &str = r#"[{"key": "B", "kind": "work", "meaning": "bugs", "turn": "agent"}, {"key": "Q", "kind": "decision", "meaning": "questions", "turn": "user"}, {"key": "A", "kind": "audit", "meaning": "plans", "turn": "agent"}]"#;

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, "Bearer k");
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let body = body.map_or_else(Body::empty, |v| Body::from(v.to_string()));
    let resp = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let out = serde_json::from_slice(&bytes).unwrap_or_default();
    assert_eq!(status, StatusCode::OK, "{uri}: {out}");
    out
}

async fn full_page(app: &Router) -> DumpPage {
    serde_json::from_value(call(app, Method::GET, "/dump?since=0", None).await).unwrap()
}

/// A database with waits, links, a replacement, citations, event data and non-ASCII text.
async fn seeded() -> (Router, Scratch) {
    let db = Scratch::new(2).await;
    db.seed(&format!(
        "INSERT INTO projects (slug, keys, skills, remotes, created_at, updated_at) VALUES \
         ('o/p', '{KEYS}', '{{\"merge\": \"r\u{e9}base\"}}', '[\"git@example.com:o/p.git\"]', 'c', 'u')"
    ))
    .await;
    let app = app(&db.db, Keys::parse("box owner k").unwrap());
    let steps = [
        (
            "new",
            json!({"key": "B", "title": "Crust \u{e9}", "body": "- **Evidence.** `src/crust.ts:3` and `tests/crust_test.rs`.\n"}),
        ),
        ("new", json!({"key": "B", "title": "Old pace"})),
        ("new", json!({"key": "Q", "title": "One cache or two"})),
        (
            "new",
            json!({"key": "A", "title": "Shelves", "body": "- **Principles.**\n  1. One owner.\n- **Evidence.** `src/crust.ts:1`\n"}),
        ),
        ("new", json!({"key": "A", "title": "Recording"})),
        ("link", json!({"a": ["B1"], "kind": "opened", "b": "A1"})),
        ("link", json!({"a": ["A1"], "kind": "related", "b": "A2"})),
        ("wait", json!({"id": "B1", "on": "Q1"})),
        ("drop", json!({"id": "B2", "superseded_by": "B1"})),
        ("new", json!({"key": "B", "title": "After shelves"})),
        ("wait", json!({"id": "B3", "on": "A1"})),
        ("answer", json!({"id": "Q1", "decision": "Two"})),
        ("start", json!({"id": "Q1"})),
        (
            "close",
            json!({"id": "Q1", "resolution": "opened B1", "gates": "passed: all"}),
        ),
    ];
    for (verb, mut body) in steps {
        body["project"] = json!("o/p");
        call(&app, Method::POST, &format!("/do/{verb}"), Some(body)).await;
    }
    (app, db)
}

fn write_all(repo: &Path, page: &DumpPage) {
    for (path, text) in files(page, |_| None) {
        tree::write(repo, &path, &text).unwrap();
    }
}

fn as_map(page: &DumpPage) -> BTreeMap<String, String> {
    files(page, |_| None).into_iter().collect()
}

#[tokio::test]
async fn test_restore_round_trips_a_full_dump() {
    let before = full_page(&seeded().await.0).await;
    let dir = tempfile::tempdir().unwrap();
    write_all(dir.path(), &before);
    let db = Scratch::bare(2).await;
    let counts = restore(dir.path(), &db.url()).await.unwrap();
    assert_eq!(
        counts,
        Restored {
            projects: 1,
            items: 6,
            events: before.events.len()
        }
    );
    let after = full_page(&app(&db.db, Keys::parse("box owner k").unwrap())).await;
    assert_eq!(as_map(&before), as_map(&after));
    let cites: i64 = scalar(
        &db.db,
        "SELECT COUNT(*) FROM links WHERE kind LIKE 'cites_%'",
        vec![],
    )
    .await
    .unwrap();
    assert_eq!(cites, 3);
}

#[tokio::test]
async fn test_restore_refuses_a_database_that_holds_items() {
    let page = full_page(&seeded().await.0).await;
    let dir = tempfile::tempdir().unwrap();
    write_all(dir.path(), &page);
    let db = Scratch::bare(2).await;
    restore(dir.path(), &db.url()).await.unwrap();
    let refused = restore(dir.path(), &db.url()).await.unwrap_err();
    assert!(refused.contains("already holds 6 items"), "{refused}");
}

#[tokio::test]
async fn test_restore_refuses_a_reference_the_checkout_does_not_hold() {
    let mut page = full_page(&seeded().await.0).await;
    page.items.retain(|i| i.id != "A1");
    let dir = tempfile::tempdir().unwrap();
    write_all(dir.path(), &page);
    let db = Scratch::bare(2).await;
    let refused = restore(dir.path(), &db.url()).await.unwrap_err();
    assert!(
        refused.contains("A1, which is not in the tree"),
        "{refused}"
    );
}

#[test]
fn test_truthy_reads_values_as_python_does() {
    for v in [
        json!(null),
        json!(false),
        json!(0),
        json!(""),
        json!([]),
        json!({}),
    ] {
        assert!(!truthy(&v), "{v}");
    }
    for v in [
        json!(true),
        json!(1),
        json!("x"),
        json!([0]),
        json!({"a": null}),
    ] {
        assert!(truthy(&v), "{v}");
    }
}
