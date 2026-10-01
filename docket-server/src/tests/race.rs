//! Scenario: several machines reach for the same item at the same moment, through separate connections
//! to one database file.
//! Expected behaviour: exactly one claim wins, and every filed item gets its own id.

use std::collections::HashSet;
use std::path::PathBuf;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use sea_orm::ConnectionTrait;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::*;

use docket_core::SCHEMA;
const RACERS: usize = 64;

fn scratch_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("docket-race-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("docket.db");
    std::fs::File::create(&path).unwrap();
    path
}

async fn file_app(name: &str) -> Router {
    let path = scratch_file(name);
    let db = connect(path.to_str().unwrap()).await.unwrap();
    db.execute_unprepared(SCHEMA).await.unwrap();
    db.execute_unprepared(
        r#"INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('o/p',
           '[{"key": "T", "kind": "work", "meaning": "tasks", "turn": "agent"}]', 'c', 'u')"#,
    )
    .await
    .unwrap();
    app(&db, Keys::parse("testbox owner ownerkey").unwrap())
}

async fn post(app: Router, verb: &'static str, body: Value) -> (StatusCode, Value) {
    let req = Request::post(format!("/do/{verb}"))
        .header(AUTHORIZATION, "Bearer ownerkey")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn race(
    app: &Router,
    verb: &'static str,
    body: impl Fn(usize) -> Value,
) -> Vec<(StatusCode, Value)> {
    let calls = (0..RACERS).map(|n| tokio::spawn(post(app.clone(), verb, body(n))));
    let mut out = Vec::new();
    for call in calls.collect::<Vec<_>>() {
        out.push(call.await.unwrap());
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_claims_one_wins() {
    let app = file_app("claims").await;
    let (status, _) = post(
        app.clone(),
        "new",
        json!({"project": "o/p", "key": "T", "title": "Contested"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let results = race(
        &app,
        "start",
        |n| json!({"project": "o/p", "id": "T1", "branch": format!("audit/t1-{n}")}),
    )
    .await;
    let won = results.iter().filter(|(s, _)| *s == StatusCode::OK).count();
    let refused = results
        .iter()
        .filter(|(s, _)| *s == StatusCode::CONFLICT)
        .count();
    assert_eq!((won, refused), (1, RACERS - 1), "{results:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_new_items_get_distinct_ids() {
    let app = file_app("ids").await;
    let results = race(
        &app,
        "new",
        |n| json!({"project": "o/p", "key": "T", "title": format!("Item {n}")}),
    )
    .await;
    let ids: HashSet<String> = results
        .iter()
        .map(|(status, out)| {
            assert_eq!(*status, StatusCode::OK, "{out}");
            out["item"]["id"].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(ids.len(), RACERS);
}
