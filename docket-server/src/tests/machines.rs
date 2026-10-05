use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use docket_migration::scratch::Scratch;

use crate::app;
use crate::auth::Keys;

async fn scratch() -> (Scratch, Router) {
    let db = Scratch::new(2).await;
    let keys = Keys::parse("alpha owner ownerkey\nbeta agent agentkey").unwrap();
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

async fn set(app: &Router, key: &str, body: Value) -> (StatusCode, Value) {
    send(app, Method::POST, "/do/machine", key, Some(body)).await
}

fn beta() -> Value {
    json!({ "name": "beta", "ssh": "dev@beta.example", "slots": 3, "runners": ["claude", "codex"] })
}

#[tokio::test]
async fn test_no_machine_is_known_until_the_owner_sets_one() {
    let (_db, app) = scratch().await;
    let (status, out) = send(&app, Method::GET, "/machines", "agentkey", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out, json!({ "machines": [] }));
}

#[tokio::test]
async fn test_the_owner_sets_a_machine_and_every_key_reads_it() {
    let (_db, app) = scratch().await;
    let (status, out) = set(&app, "ownerkey", beta()).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["machines"][0]["name"], "beta");
    let (_, read) = send(&app, Method::GET, "/machines", "agentkey", None).await;
    let m = &read["machines"][0];
    assert_eq!(
        (
            m["ssh"].as_str(),
            m["slots"].as_i64(),
            m["runners"].clone(),
            m["note"].clone()
        ),
        (
            Some("dev@beta.example"),
            Some(3),
            json!(["claude", "codex"]),
            Value::Null
        )
    );
    assert!(m["updated_at"].as_str().is_some_and(|t| t.ends_with('Z')));
}

#[tokio::test]
async fn test_an_agent_key_setting_a_machine_is_forbidden() {
    let (_db, app) = scratch().await;
    let (status, out) = set(&app, "agentkey", beta()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        out["error"],
        "docket machine is refused on an agent's key: the machines are the owner's to set"
    );
    let remove = json!({ "name": "beta", "remove": true });
    assert_eq!(set(&app, "agentkey", remove).await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        send(&app, Method::GET, "/machines", "ownerkey", None)
            .await
            .1,
        json!({ "machines": [] })
    );
}

#[tokio::test]
async fn test_a_machine_with_no_ssh_address_is_refused() {
    let (_db, app) = scratch().await;
    let (status, out) = set(
        &app,
        "ownerkey",
        json!({ "name": "beta", "slots": 3, "runners": ["codex"] }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        out["refused"],
        "a new machine needs --ssh, --slots and --runners; beta has no ssh"
    );
    let blank = json!({ "name": "beta", "ssh": "", "slots": 3, "runners": ["codex"] });
    let (status, out) = set(&app, "ownerkey", blank).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        out["refused"],
        "beta needs an ssh address the other machines reach it at"
    );
}

#[tokio::test]
async fn test_a_set_changes_only_the_fields_it_names() {
    let (_db, app) = scratch().await;
    set(&app, "ownerkey", beta()).await;
    let (status, out) = set(
        &app,
        "ownerkey",
        json!({ "name": "beta", "slots": 6, "note": "spare" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let m = &out["machines"][0];
    assert_eq!(
        (m["slots"].as_i64(), m["ssh"].as_str()),
        (Some(6), Some("dev@beta.example"))
    );
    assert_eq!(m["note"], "spare");
}

#[tokio::test]
async fn test_machines_are_listed_by_name_and_removed_one_at_a_time() {
    let (_db, app) = scratch().await;
    set(&app, "ownerkey", beta()).await;
    let alpha =
        json!({ "name": "alpha", "ssh": "alpha.example", "slots": 1, "runners": ["claude"] });
    let (_, out) = set(&app, "ownerkey", alpha).await;
    let names: Vec<&str> = out["machines"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["name"].as_str())
        .collect();
    assert_eq!(names, ["alpha", "beta"]);
    let (status, out) = set(&app, "ownerkey", json!({ "name": "alpha", "remove": true })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out["machines"].as_array().unwrap().len(), 1);
    let (status, out) = set(&app, "ownerkey", json!({ "name": "alpha", "remove": true })).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(out["error"], "no machine alpha");
}

async fn report(app: &Router, key: &str, runner: &str, until: &str) -> (StatusCode, Value) {
    let body = json!({ "machine": "beta", "runner": runner, "until": until });
    send(app, Method::POST, "/do/limit", key, Some(body)).await
}

#[tokio::test]
async fn test_a_reported_limit_is_read_with_the_machine_by_every_key() {
    let (_db, app) = scratch().await;
    set(&app, "ownerkey", beta()).await;
    let (status, out) = report(&app, "agentkey", "codex", "2999-10-07T18:29:00Z").await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let (_, read) = send(&app, Method::GET, "/machines", "ownerkey", None).await;
    assert_eq!(
        read["machines"][0]["limits"],
        json!({ "codex": "2999-10-07T18:29:00Z" })
    );
}

#[tokio::test]
async fn test_a_later_report_keeps_the_later_reset_and_a_set_keeps_the_limits() {
    let (_db, app) = scratch().await;
    set(&app, "ownerkey", beta()).await;
    report(&app, "agentkey", "codex", "2999-10-09T00:00:00Z").await;
    let (_, out) = report(&app, "agentkey", "codex", "2999-10-07T18:29:00Z").await;
    assert_eq!(
        out["machines"][0]["limits"]["codex"],
        "2999-10-09T00:00:00Z"
    );
    let (_, out) = set(&app, "ownerkey", json!({ "name": "beta", "slots": 5 })).await;
    assert_eq!(
        out["machines"][0]["limits"]["codex"],
        "2999-10-09T00:00:00Z"
    );
}

#[tokio::test]
async fn test_a_limit_already_past_is_dropped_on_the_next_report() {
    let (_db, app) = scratch().await;
    set(&app, "ownerkey", beta()).await;
    report(&app, "agentkey", "codex", "2000-01-01T00:00:00Z").await;
    let (_, out) = report(&app, "agentkey", "claude", "2999-10-07T18:29:00Z").await;
    assert_eq!(
        out["machines"][0]["limits"],
        json!({ "claude": "2999-10-07T18:29:00Z" })
    );
}

#[tokio::test]
async fn test_a_limit_for_an_unknown_machine_runner_or_stamp_is_refused() {
    let (_db, app) = scratch().await;
    let body = json!({ "machine": "beta", "runner": "codex", "until": "2999-10-07T18:29:00Z" });
    let (status, _) = send(&app, Method::POST, "/do/limit", "agentkey", Some(body)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    set(
        &app,
        "ownerkey",
        json!({ "name": "beta", "ssh": "b", "slots": 1, "runners": ["claude"] }),
    )
    .await;
    let (status, out) = report(&app, "agentkey", "codex", "2999-10-07T18:29:00Z").await;
    assert_eq!(
        (status, out["refused"].as_str()),
        (StatusCode::CONFLICT, Some("beta has no codex"))
    );
    let (status, _) = report(&app, "agentkey", "claude", "tomorrow").await;
    assert_eq!(status, StatusCode::CONFLICT);
}
