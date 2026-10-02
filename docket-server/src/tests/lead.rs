use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde_json::{Value, json};
use tower::ServiceExt;

use docket_migration::scratch::Scratch;

use crate::app;
use crate::auth::Keys;
use crate::store::{column, sql};

const SLUG: &str = "o/p";

async fn scratch() -> (Scratch, Router) {
    let db = Scratch::new(4).await;
    db.seed(&format!(
        "INSERT INTO projects (slug, keys, skills, created_at, updated_at) \
         VALUES ('{SLUG}', '[]', '{{}}', 'c', 'u')"
    ))
    .await;
    let keys = Keys::parse("alpha owner alphakey\nbeta agent betakey").unwrap();
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

async fn act(app: &Router, key: &str, act: &str, session: &str) -> (StatusCode, Value) {
    let body = json!({ "project": SLUG, "branch": "main", "act": act, "session": session });
    send(app, Method::POST, "/do/lead", key, Some(body)).await
}

async fn read(app: &Router) -> Value {
    send(app, Method::GET, "/lead?project=o/p", "betakey", None)
        .await
        .1
}

async fn lead_events(db: &DatabaseConnection) -> Vec<String> {
    column(
        db,
        "SELECT COALESCE(data->>'act', '') FROM events WHERE kind='lead' ORDER BY seq",
        vec![],
    )
    .await
    .unwrap()
}

/// The stored claim's last renewal moved back, as if its holder went quiet that long ago.
async fn quiet_since(db: &DatabaseConnection, stamp: &str) {
    db.execute_raw(sql("UPDATE leads SET renewed_at=?", vec![stamp.into()]))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_no_lead_reads_as_none() {
    let (_db, app) = scratch().await;
    assert_eq!(
        read(&app).await,
        json!({ "project": SLUG, "lead": null, "lapsed": false, "lapses_at": null, "lapse_minutes": 10 })
    );
}

#[tokio::test]
async fn test_take_records_the_holder_and_an_event() {
    let (db, app) = scratch().await;
    let (status, out) = act(&app, "betakey", "take", "lead-1").await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["outcome"], "took");
    let lead = &read(&app).await["lead"];
    assert_eq!(
        (
            lead["host"].as_str(),
            lead["session"].as_str(),
            lead["branch"].as_str()
        ),
        (Some("beta"), Some("lead-1"), Some("main"))
    );
    assert_eq!(lead_events(&db.db).await, ["take"]);
}

#[tokio::test]
async fn test_two_takes_at_once_give_one_lead_and_refuse_the_other_naming_it() {
    let (db, app) = scratch().await;
    let (a, b) = tokio::join!(
        act(&app, "alphakey", "take", "lead-a"),
        act(&app, "betakey", "take", "lead-b")
    );
    let mut statuses = [a.0, b.0];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);
    let (won, lost) = if a.0 == StatusCode::OK {
        (a.1, b.1)
    } else {
        (b.1, a.1)
    };
    let holder = won["lead"]["session"].as_str().unwrap().to_string();
    assert!(
        lost["refused"]
            .as_str()
            .unwrap()
            .starts_with(&format!("o/p is led by {holder} on ")),
        "{lost}"
    );
    assert_eq!(lead_events(&db.db).await, ["take"]);
}

#[tokio::test]
async fn test_a_holder_silent_past_the_lapse_is_taken_over() {
    let (db, app) = scratch().await;
    act(&app, "alphakey", "take", "lead-a").await;
    quiet_since(&db.db, "2026-01-01T00:00:00Z").await;
    let state = read(&app).await;
    assert_eq!(state["lapsed"], true);
    assert_eq!(state["lapses_at"], "2026-01-01T00:10:00Z");
    let (status, out) = act(&app, "betakey", "take", "lead-b").await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["outcome"], "took over");
    assert_eq!(out["previous"]["session"], "lead-a");
    assert_eq!(out["lead"]["host"], "beta");
    assert_eq!(out["lapsed"], false);
    assert_eq!(lead_events(&db.db).await, ["take", "takeover"]);
}

#[tokio::test]
async fn test_the_lapse_is_the_projects_lead_lapse_fact() {
    let (db, app) = scratch().await;
    db.seed("UPDATE projects SET skills='{\"lead_lapse\": \"90\"}'")
        .await;
    act(&app, "alphakey", "take", "lead-a").await;
    quiet_since(&db.db, "2026-01-01T00:00:00Z").await;
    let state = read(&app).await;
    assert_eq!(
        (state["lapse_minutes"].as_i64(), state["lapses_at"].as_str()),
        (Some(90), Some("2026-01-01T01:30:00Z"))
    );
}

#[tokio::test]
async fn test_a_renewal_by_a_non_holder_is_refused() {
    let (db, app) = scratch().await;
    act(&app, "alphakey", "take", "lead-a").await;
    let (status, out) = act(&app, "betakey", "renew", "lead-b").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        out["refused"],
        "o/p is led by lead-a on alpha, not by lead-b on beta: only its holder renews it"
    );
    let (status, out) = act(&app, "alphakey", "renew", "lead-a").await;
    assert_eq!(
        (status, out["outcome"].as_str()),
        (StatusCode::OK, Some("renewed"))
    );
    assert_eq!(
        lead_events(&db.db).await,
        ["take"],
        "a renewal writes no event"
    );
}

#[tokio::test]
async fn test_give_frees_the_lead_for_the_next_take() {
    let (db, app) = scratch().await;
    act(&app, "alphakey", "take", "lead-a").await;
    let (status, out) = act(&app, "alphakey", "give", "lead-a").await;
    assert_eq!(
        (status, out["outcome"].as_str()),
        (StatusCode::OK, Some("gave"))
    );
    assert_eq!(out["lead"], Value::Null);
    assert_eq!(read(&app).await["lead"], Value::Null);
    assert_eq!(
        act(&app, "betakey", "take", "lead-b").await.0,
        StatusCode::OK
    );
    assert_eq!(lead_events(&db.db).await, ["take", "give", "take"]);
}

#[tokio::test]
async fn test_an_unknown_act_session_or_project_is_refused() {
    let (_db, app) = scratch().await;
    let (status, out) = act(&app, "alphakey", "steal", "lead-a").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(out["error"], "act is one of take, renew, give, not 'steal'");
    let (status, out) = act(&app, "alphakey", "take", " ").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(out["error"], "a lead names its session");
    let body = json!({ "project": "no/such", "act": "take", "session": "s" });
    assert_eq!(
        send(&app, Method::POST, "/do/lead", "alphakey", Some(body))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::GET, "/lead?project=no/such", "alphakey", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}
