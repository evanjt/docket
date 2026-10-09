use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use docket_migration::scratch::Scratch;

use crate::app;
use crate::auth::Keys;

const SLUG: &str = "o/p";

async fn scratch() -> (Scratch, Router) {
    let db = Scratch::new(4).await;
    db.seed(&format!(
        "INSERT INTO projects (slug, created_at, updated_at) VALUES ('{SLUG}', 'c', 'u');
         INSERT INTO items (rid, project, key, num, title, state, resolution, type, opened_at, updated_at) VALUES
           (1, '{SLUG}', 'A', 1, 'lay the kiln', 'done', 'fired', 'plan', 'o', 'u'),
           (2, '{SLUG}', 'A', 2, 'glaze the bowls', 'done', 'fired', 'plan', 'o', 'u'),
           (3, '{SLUG}', 'T', 1, 'stack the wood', 'done', 'stacked', 'task', 'o', 'u');"
    ))
    .await;
    let keys = Keys::parse("alpha owner alphakey\nbeta agent betakey").unwrap();
    let router = app(&db.db, keys);
    (db, router)
}

async fn send(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, "Bearer betakey");
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

async fn record(app: &Router, published: &str, plans: &[&str]) -> (StatusCode, Value) {
    let body = json!({
        "project": SLUG,
        "published": published,
        "work": "c".repeat(40),
        "plans": plans,
    });
    send(app, Method::POST, "/do/publication", Some(body)).await
}

fn shas(out: &Value) -> Vec<(String, Value)> {
    out["publications"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["published"].as_str().unwrap().to_string(),
                p["plans"].clone(),
            )
        })
        .collect()
}

#[tokio::test]
async fn test_publications_are_recorded_and_listed_newest_first() {
    let (_db, app) = scratch().await;
    let (status, out) = send(
        &app,
        Method::GET,
        &format!("/publications?project={SLUG}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out, json!({ "project": SLUG, "publications": [] }));

    let (old, new) = ("a".repeat(40), "b".repeat(40));
    let (status, out) = record(&app, &old, &["A1"]).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    let (status, out) = record(&app, &new, &["A2", "A1", "A2"]).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["publications"][0]["work"], "c".repeat(40));

    let (_, listed) = send(
        &app,
        Method::GET,
        &format!("/publications?project={SLUG}"),
        None,
    )
    .await;
    assert_eq!(
        shas(&listed),
        [(new, json!(["A1", "A2"])), (old, json!(["A1"]))]
    );
}

#[tokio::test]
async fn test_a_published_sha_is_recorded_once_and_the_first_row_stands() {
    let (_db, app) = scratch().await;
    let sha = "a".repeat(40);
    record(&app, &sha, &["A1"]).await;
    let (status, out) = record(&app, &sha, &["A2"]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{out}");
    assert_eq!(
        out["refused"],
        format!("{sha} is recorded as published in {SLUG} already")
    );
    let (_, listed) = send(
        &app,
        Method::GET,
        &format!("/publications?project={SLUG}"),
        None,
    )
    .await;
    assert_eq!(shas(&listed), [(sha, json!(["A1"]))]);
}

#[tokio::test]
async fn test_a_publication_names_full_shas_and_only_plans_of_its_project() {
    let (_db, app) = scratch().await;
    let (status, out) = record(&app, "abc1234", &[]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{out}");
    assert_eq!(
        out["refused"],
        "the published commit is a full sha of 40 or 64 lowercase hex digits, not 'abc1234'"
    );
    let (status, out) = record(&app, &"a".repeat(40), &["T1"]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{out}");
    assert_eq!(
        out["refused"],
        "T1 is a task, and a publication covers plans"
    );
    let (status, _) = record(&app, &"a".repeat(40), &["A9"]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::GET, "/publications?project=no/such", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, listed) = send(
        &app,
        Method::GET,
        &format!("/publications?project={SLUG}"),
        None,
    )
    .await;
    assert_eq!(shas(&listed).len(), 0);
}
