//! Scenario: the server reads a database another program writes.
//! Expected behaviour: reads answer, every write is refused before it reaches a handler, and the
//! read-only connection cannot write even when asked directly.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use sea_orm::ConnectionTrait;
use serde_json::Value;
use tower::ServiceExt;

use docket_core::SCHEMA;

use super::*;

async fn seeded_file(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("docket-read-only-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("docket.db");
    std::fs::File::create(&path).unwrap();
    let path = path.to_str().unwrap().to_string();
    let db = connect(&path).await.unwrap();
    db.execute_unprepared(SCHEMA).await.unwrap();
    db.execute_unprepared(
        r#"INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('o/p',
           '[{"key": "T", "kind": "work", "meaning": "tasks", "turn": "agent"}]', 'c', 'u')"#,
    )
    .await
    .unwrap();
    path
}

async fn send(app: Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn test_read_only_answers_reads_and_refuses_writes() {
    let path = seeded_file("routes").await;
    let db = connect_read_only(&path).await.unwrap();
    let app = read_only(app(&db, Keys::parse("laptop owner k").unwrap()));

    let get = Request::get("/projects").header(AUTHORIZATION, "Bearer k");
    let (status, projects) = send(app.clone(), get.body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(projects[0]["slug"], "o/p");

    let post = Request::post("/do/new")
        .header(AUTHORIZATION, "Bearer k")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"project": "o/p", "branch": "main", "key": "T", "title": "x"}"#,
        ))
        .unwrap();
    let (status, out) = send(app, post).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(out["refused"], "this server is read-only");
}

#[tokio::test]
async fn test_connect_read_only_cannot_write() {
    let path = seeded_file("connection").await;
    let db = connect_read_only(&path).await.unwrap();
    let write = db.execute_unprepared("DELETE FROM projects").await;
    assert!(write.is_err());
    let rw = connect(&path).await.unwrap();
    let left = rw.query_one_raw(sea_orm::Statement::from_string(
        rw.get_database_backend(),
        "SELECT COUNT(*) AS n FROM projects",
    ));
    assert!(left.await.unwrap().is_some());
}
