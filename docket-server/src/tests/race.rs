//! Scenario: several machines reach for the same item at the same moment, through separate connections
//! to one database.
//! Expected behaviour: exactly one claim wins, and every filed item gets its own id.

use std::collections::HashSet;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DbBackend, Statement};

use docket_migration::scratch::Scratch;

use super::*;

const RACERS: usize = 64;

/// A server over a database of its own, with as many connections as the server opens.
async fn served() -> (Router, Scratch) {
    let s = Scratch::new(CONNECTIONS).await;
    s.seed(
        r#"INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('o/p',
           '[{"key": "T", "kind": "work", "meaning": "tasks", "turn": "agent"}]', 'c', 'u')"#,
    )
    .await;
    s.seed("INSERT INTO areas (project, name, description, position, priority) VALUES ('o/p', 'general', '', 1, NULL)")
        .await;
    (
        app(&s.db, Keys::parse("testbox owner ownerkey").unwrap()),
        s,
    )
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
    let (app, _db) = served().await;
    let (status, _) = post(
        app.clone(),
        "new",
        json!({"project": "o/p", "key": "T", "title": "Contested", "area": "general"}),
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
    let (app, _db) = served().await;
    let results = race(
        &app,
        "new",
        |n| json!({"project": "o/p", "key": "T", "title": format!("Item {n}"), "area": "general"}),
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

async fn waiting_for_an_advisory_lock(holder: &sea_orm::DatabaseConnection) -> i64 {
    holder
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*) AS n FROM pg_locks WHERE locktype = 'advisory' AND NOT granted",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "n")
        .unwrap()
}

/// Scenario: another process holds the write lock while more writers queue than the pool has connections.
/// Expected behaviour: the queued writers wait without a connection, so a read still answers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_queued_writers_leave_connections_for_reads() {
    let (app, s) = served().await;
    let mut options = ConnectOptions::new(s.url());
    options.max_connections(1).sqlx_logging(false);
    let holder = Database::connect(options).await.unwrap();
    holder
        .execute_unprepared(&format!(
            "SELECT pg_advisory_lock({})",
            crate::store::WRITES
        ))
        .await
        .unwrap();

    let writers: Vec<_> = (0..10)
        .map(|_| {
            tokio::spawn(post(
                app.clone(),
                "priority",
                json!({"project": "o/p", "ids": ["T1"], "tier": "high"}),
            ))
        })
        .collect();
    for _ in 0..100 {
        if waiting_for_an_advisory_lock(&holder).await > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    let read = Request::get("/check?project=o/p")
        .header(AUTHORIZATION, "Bearer ownerkey")
        .body(Body::empty())
        .unwrap();
    let answered = tokio::time::timeout(Duration::from_secs(2), app.clone().oneshot(read)).await;

    holder
        .execute_unprepared(&format!(
            "SELECT pg_advisory_unlock({})",
            crate::store::WRITES
        ))
        .await
        .unwrap();
    for writer in writers {
        writer.await.unwrap();
    }
    let status = answered
        .expect("a read answers while writers queue on the lock")
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::OK);
}
