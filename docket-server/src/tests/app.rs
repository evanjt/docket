use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use serde_json::Value;
use tower::ServiceExt;

use super::*;

use docket_core::SCHEMA;
const SEED: &str = r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('o/p',
  '[{"key":"T","kind":"work"},{"key":"PK","kind":"package"},{"key":"CON","kind":"concept"}]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, tags, body, group_name, opened_at, updated_at)
  VALUES (1, 'o/p', 'T', 1, 'First', 'open', 'agent', '["high","single"]', 'Body text', 'g', 'o1', 'u1'),
         (2, 'o/p', 'PK', 1, 'Package', 'open', 'agent', '[]', '', NULL, 'o2', 'u2'),
         (3, 'o/p', 'CON', 1, 'Concept', 'open', 'agent', '[]', '', NULL, 'o3', 'u3');
INSERT INTO items (rid, project, key, num, title, state, resolution, superseded_by, tags, opened_at, updated_at)
  VALUES (4, 'o/p', 'T', 2, 'Old', 'dropped', 'replaced', 1, '[]', 'o4', 'u4');
INSERT INTO links (rid, kind, to_rid) VALUES (1, 'opened', 2), (1, 'related', 3);
INSERT INTO links (rid, kind, to_path, to_line) VALUES (1, 'cites_file', 'src/a.rs', 7);
INSERT INTO events (uid, project, rid, at, host, kind) VALUES ('e1', 'o/p', 1, 'o1', 'devbox', 'opened');
"#;

async fn seeded() -> Router {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1);
    let db = Database::connect(options).await.unwrap();
    db.execute_unprepared(SCHEMA).await.unwrap();
    db.execute_unprepared(SEED).await.unwrap();
    app(&db, Keys::parse("devbox agent secret").unwrap())
}

async fn send(app: Router, method: Method, uri: &str, key: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(key) = key {
        req = req.header(AUTHORIZATION, format!("Bearer {key}"));
    }
    let resp = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn get(uri: &str) -> (StatusCode, Value) {
    send(seeded().await, Method::GET, uri, Some("secret")).await
}

#[tokio::test]
async fn test_request_without_known_key_is_401() {
    for key in [None, Some("wrong"), Some("")] {
        for uri in ["/items", "/show/T1?project=o/p", "/projects"] {
            let (status, _) = send(seeded().await, Method::GET, uri, key).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} with {key:?}");
        }
    }
}

#[tokio::test]
async fn test_health_answers_without_a_key() {
    for key in [None, Some("wrong"), Some("secret")] {
        let (status, _) = send(seeded().await, Method::GET, "/health", key).await;
        assert_eq!(status, StatusCode::OK, "with {key:?}");
    }
    let (status, _) = send(seeded().await, Method::POST, "/health", None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_show_matches_the_json_of_docket_show() {
    let (status, body) = get("/show/T1?project=o/p").await;
    assert_eq!(status, StatusCode::OK);
    let expected: Value = serde_json::from_str(
        r#"{
        "project": "o/p", "key": "T", "num": 1, "id": "T1", "title": "First", "state": "open",
        "turn": "agent", "turn_note": null, "asked_at": null,
        "claim_branch": null, "claim_host": null, "claim_since": null,
        "claim_runner": null, "claim_job": null, "claim_on": null,
        "wait_on": null, "wait_ref": null, "wait_since": null,
        "decision": null, "decided_at": null, "resolution": null,
        "scope": null, "complexity": null, "theme": null, "rank": null,
        "tags": ["high", "single"], "body": "Body text", "conflict": 0,
        "opened_at": "o1", "updated_at": "u1",
        "group": "g", "word": "ready", "priority": "high", "superseded_by": null,
        "related": ["CON1"], "opened": ["PK1"],
        "cites": [{"path": "src/a.rs", "line": 7, "kind": "cites_file"}]
        }"#,
    )
    .unwrap();
    assert_eq!(body, expected);
}

#[tokio::test]
async fn test_show_derives_word_from_kind_and_open_members() {
    let (_, package) = get("/show/PK1?project=o/p").await;
    assert_eq!(package["word"], "building");
    assert_eq!(
        package["progress"].to_string(),
        r#"{"done":0,"live":0,"total":1}"#
    );
    assert_eq!(get("/show/CON1?project=o/p").await.1["word"], "standing");
    let (_, dropped) = get("/show/T2?project=o/p").await;
    assert_eq!(dropped["word"], "dropped");
    assert_eq!(dropped["superseded_by"], "T1");
}

#[tokio::test]
async fn test_show_unknown_item_or_project_is_404() {
    assert_eq!(get("/show/T9?project=o/p").await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        get("/show/T1?project=o/none").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn test_resources_list_filter_and_fetch_by_key() {
    let (status, items) = get("/items").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(items.as_array().unwrap().len(), 4);
    assert!(items[0].get("body").is_none());

    let (_, dropped) = get("/items?filter=%7B%22state%22%3A%22dropped%22%7D").await;
    assert_eq!(dropped.as_array().unwrap().len(), 1);
    assert_eq!(dropped[0]["id"], "T2");

    assert_eq!(get("/items/1").await.1["body"], "Body text");
    assert_eq!(get("/links").await.1.as_array().unwrap().len(), 3);
    assert_eq!(get("/events").await.1[0]["host"], "devbox");
    assert_eq!(get("/projects").await.1[0]["slug"], "o/p");
}

#[tokio::test]
async fn test_no_route_accepts_a_write() {
    for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
        for uri in [
            "/items",
            "/items/1",
            "/events",
            "/links/1",
            "/projects",
            "/show/T1?project=o/p",
        ] {
            let (status, _) = send(seeded().await, method.clone(), uri, Some("secret")).await;
            assert!(
                status == StatusCode::METHOD_NOT_ALLOWED || status == StatusCode::NOT_FOUND,
                "{method} {uri} gave {status}"
            );
        }
    }
    assert_eq!(get("/items").await.1.as_array().unwrap().len(), 4);
}
