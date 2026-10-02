use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde_json::{Value, json};
use tower::ServiceExt;

use docket_migration::scratch::Scratch;

use crate::app;
use crate::auth::Keys;

const SLUG: &str = "test/proj";

async fn scratch() -> (Scratch, Router) {
    let db = Scratch::new(2).await;
    db.seed(&format!(
        "INSERT INTO projects (slug, keys, skills, created_at, updated_at) \
         VALUES ('{SLUG}', '[]', '{{\"owner\": \"Ana\", \"pool\": \"local=2\"}}', 'c', 'u')"
    ))
    .await;
    let keys = Keys::parse("hosta owner ownerkey\nhostb agent agentkey").unwrap();
    let router = app(&db.db, keys);
    (db, router)
}

async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    key: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {key}"));
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn set_as(app: &Router, key: &str, fact: &str, value: &str) -> (StatusCode, Value) {
    let body = json!({ "project": SLUG, "key": fact, "value": value });
    send(app, Method::POST, "/do/fact", key, Some(body)).await
}

async fn stored(db: &DatabaseConnection) -> Value {
    let row = db
        .query_one_raw(crate::store::sql(
            "SELECT skills FROM projects WHERE slug=?",
            vec![SLUG.into()],
        ))
        .await
        .unwrap()
        .unwrap();
    row.try_get_by_index::<Value>(0).unwrap()
}

#[tokio::test]
async fn test_read_returns_the_stored_facts_without_the_retired_ones() {
    let (_db, app) = scratch().await;
    let (status, out) = send(
        &app,
        Method::GET,
        "/facts?project=test/proj",
        "agentkey",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        out,
        json!({ "project": SLUG, "skills": { "owner": "Ana" } })
    );
}

#[tokio::test]
async fn test_read_an_unknown_project_is_not_found() {
    let (_db, app) = scratch().await;
    let (status, out) = send(
        &app,
        Method::GET,
        "/facts?project=no/such",
        "ownerkey",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(out["error"], "no project no/such");
}

#[tokio::test]
async fn test_set_stores_the_fact_and_marks_the_project_for_the_dump() {
    let (db, app) = scratch().await;
    let (status, out) = set_as(&app, "agentkey", "gates", "make test").await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["skills"], json!({"gates": "make test", "owner": "Ana"}));
    assert_eq!(
        stored(&db.db).await,
        json!({"gates": "make test", "owner": "Ana", "pool": "local=2"}),
        "a retired fact stays stored"
    );
    let row = db
        .db
        .query_one_raw(crate::store::sql(
            "SELECT v FROM meta WHERE k='pending_dump_projects'",
            vec![],
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get_by_index::<String>(0).unwrap(),
        r#"["test/proj"]"#
    );
}

#[tokio::test]
async fn test_set_an_empty_value_unsets_the_fact() {
    let (db, app) = scratch().await;
    let (status, out) = set_as(&app, "ownerkey", "owner", "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out["skills"], json!({}));
    assert_eq!(stored(&db.db).await, json!({"pool": "local=2"}));
}

#[tokio::test]
async fn test_set_stores_models_that_read() {
    let (db, app) = scratch().await;
    let models = "high=claude:opus-x:high medium=codex:gpt-x";
    let (status, out) = set_as(&app, "agentkey", "models", models).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(stored(&db.db).await["models"], models);
}

#[tokio::test]
async fn test_set_refuses_a_value_the_fact_does_not_hold_and_changes_nothing() {
    let (db, app) = scratch().await;
    for (fact, value, why) in [
        ("mode", "go", "mode is one of run, drain, pause, not 'go'"),
        (
            "job_timeout",
            "0",
            "job_timeout is a whole number above 0, not '0'",
        ),
        (
            "flow",
            "ticket",
            "flow is written by docket, not set by hand",
        ),
        (
            "models",
            "high=gemini:m",
            "models: 'high=gemini:m' runs on gemini, not one of claude, codex",
        ),
        (
            "pool",
            "a=4",
            "pool was a fact of the old loop and is no longer read",
        ),
    ] {
        let (status, out) = set_as(&app, "ownerkey", fact, value).await;
        assert_eq!(status, StatusCode::CONFLICT, "{fact}");
        assert_eq!(out["refused"], why);
    }
    let (status, out) = set_as(&app, "ownerkey", "colour", "red").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        out["refused"]
            .as_str()
            .unwrap()
            .starts_with("colour is not a skill fact")
    );
    assert_eq!(
        stored(&db.db).await,
        json!({"owner": "Ana", "pool": "local=2"})
    );
}
