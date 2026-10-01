use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use serde_json::{Value, json};
use tower::ServiceExt;

use docket_core::SCHEMA;

use crate::app;
use crate::auth::Keys;

const KEYS: &str = r#"[{"key": "B", "kind": "work", "meaning": "bugs", "turn": "agent"}]"#;

async fn scratch() -> (DatabaseConnection, Router) {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1);
    let db = Database::connect(options).await.unwrap();
    db.execute_unprepared(SCHEMA).await.unwrap();
    db.execute_unprepared(&format!(
        "INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('t/p', '{KEYS}', 'c', 'u')"
    ))
    .await
    .unwrap();
    let router = app(&db, Keys::parse("testbox owner ownerkey").unwrap());
    (db, router)
}

async fn post(app: &Router, verb: &str, mut body: Value) -> (StatusCode, Value) {
    body["project"] = json!("t/p");
    let req = Request::post(format!("/do/{verb}"))
        .header(AUTHORIZATION, "Bearer ownerkey")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// The kind, note and data of B1's newest event.
async fn last_event(db: &DatabaseConnection) -> (String, String, String) {
    let row = db
        .query_one_raw(crate::store::sql(
            "SELECT kind, note, data FROM events WHERE rid=1 ORDER BY seq DESC LIMIT 1",
            vec![],
        ))
        .await
        .unwrap()
        .unwrap();
    (
        row.try_get_by_index(0).unwrap(),
        row.try_get_by_index(1).unwrap(),
        row.try_get_by_index(2).unwrap(),
    )
}

#[tokio::test]
async fn test_retry_hands_a_parked_item_back_with_the_retry_mark() {
    let (db, app) = scratch().await;
    post(&app, "new", json!({ "key": "B", "title": "Flaky" })).await;
    post(
        &app,
        "ask",
        json!({ "id": "B1", "note": "parked by the loop" }),
    )
    .await;
    let (status, out) = post(&app, "retry", json!({ "id": "B1" })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(out["refused"], "reply needs a note saying what happened.");
    let (status, out) = post(&app, "retry", json!({ "id": "B1", "note": "try again" })).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["item"]["turn"], "agent");
    assert_eq!(out["item"]["turn_note"], "try again");
    assert_eq!(
        last_event(&db).await,
        (
            "replied".into(),
            "try again".into(),
            r#"{"retry": true}"#.into()
        )
    );
}

#[tokio::test]
async fn test_retry_on_the_agents_turn_records_an_edit() {
    let (db, app) = scratch().await;
    post(&app, "new", json!({ "key": "B", "title": "Flaky" })).await;
    let (status, _) = post(&app, "retry", json!({ "id": "B1" })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        last_event(&db).await,
        ("edited".into(), "retry".into(), r#"{"retry": true}"#.into())
    );
}

#[tokio::test]
async fn test_retry_refuses_a_held_item() {
    let (_, app) = scratch().await;
    post(&app, "new", json!({ "key": "B", "title": "Flaky" })).await;
    post(&app, "start", json!({ "id": "B1", "branch": "audit/b1-1" })).await;
    let (status, out) = post(&app, "retry", json!({ "id": "B1", "note": "x" })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        out["refused"],
        "B1 is held by audit/b1-1: docket kill B1 first if its job is stuck."
    );
}
