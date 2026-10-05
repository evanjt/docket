use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use futures_util::StreamExt;
use sea_orm::{ConnectionTrait, DatabaseConnection};
use tower::ServiceExt;

use docket_migration::scratch::Scratch;

use crate::app;
use crate::auth::Keys;
use crate::store::Tx;

const SEED: &str = "INSERT INTO projects (slug, keys, created_at, updated_at) \
                    VALUES ('o/p', '[{\"key\":\"T\",\"kind\":\"work\"}]', 'c', 'u');";

async fn seeded() -> Scratch {
    let s = Scratch::new(2).await;
    s.seed(SEED).await;
    s
}

async fn subscribe(db: &DatabaseConnection, key: Option<&str>) -> axum::response::Response {
    let mut req = Request::builder().uri("/changes");
    if let Some(key) = key {
        req = req.header(AUTHORIZATION, format!("Bearer {key}"));
    }
    app(db, Keys::parse("devbox agent secret").unwrap())
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

/// The next frame of the stream as text, `None` when none comes within the wait.
async fn frame(
    stream: &mut (impl futures_util::Stream<Item = Result<axum::body::Bytes, axum::Error>> + Unpin),
    wait: Duration,
) -> Option<String> {
    let bytes = tokio::time::timeout(wait, stream.next())
        .await
        .ok()??
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// One write transaction, committed with nothing in it but its notice.
async fn write(db: &DatabaseConnection) {
    Tx::begin(db, "devbox")
        .await
        .unwrap()
        .commit()
        .await
        .unwrap();
}

/// One write transaction that wrote to the named projects.
async fn write_to(db: &DatabaseConnection, slugs: &[&str]) {
    let mut tx = Tx::begin(db, "devbox").await.unwrap();
    for slug in slugs {
        tx.touch_project(slug);
    }
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn test_changes_without_a_key_is_401() {
    let s = seeded().await;
    for key in [None, Some("wrong")] {
        assert_eq!(
            subscribe(&s.db, key).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn test_changes_says_hello_then_change_after_each_write_commits() {
    let s = seeded().await;
    let resp = subscribe(&s.db, Some("secret")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["content-type"], "text/event-stream");
    let mut stream = resp.into_body().into_data_stream();
    let hello = frame(&mut stream, Duration::from_secs(2)).await.unwrap();
    assert!(hello.starts_with("event: hello"), "{hello}");
    assert_eq!(frame(&mut stream, Duration::from_millis(500)).await, None);

    write(&s.db).await;
    let change = frame(&mut stream, Duration::from_secs(3)).await.unwrap();
    assert!(
        change.starts_with("event: change\ndata: {\"n\":1,\"projects\":[]}"),
        "{change}"
    );

    let other = crate::connect(&s.url()).await.unwrap();
    write(&other).await;
    let again = frame(&mut stream, Duration::from_secs(3)).await.unwrap();
    assert!(
        again.starts_with("event: change\ndata: {\"n\":2,\"projects\":[]}"),
        "{again}"
    );
}

#[tokio::test]
async fn test_changes_says_nothing_for_a_write_rolled_back() {
    let s = seeded().await;
    let resp = subscribe(&s.db, Some("secret")).await;
    let mut stream = resp.into_body().into_data_stream();
    frame(&mut stream, Duration::from_secs(2)).await.unwrap();
    let tx = Tx::begin(&s.db, "devbox").await.unwrap();
    tx.conn
        .execute_unprepared(&format!("NOTIFY {}", super::CHANNEL))
        .await
        .unwrap();
    drop(tx);
    assert_eq!(frame(&mut stream, Duration::from_millis(500)).await, None);
}

#[tokio::test]
async fn test_changes_name_the_projects_a_write_touched() {
    let s = seeded().await;
    let resp = subscribe(&s.db, Some("secret")).await;
    let mut stream = resp.into_body().into_data_stream();
    frame(&mut stream, Duration::from_secs(2)).await.unwrap();

    write_to(&s.db, &["o/p", "o/q"]).await;
    let change = frame(&mut stream, Duration::from_secs(3)).await.unwrap();
    assert!(
        change.starts_with("event: change\ndata: {\"n\":1,\"projects\":[\"o/p\",\"o/q\"]}"),
        "{change}"
    );
}

#[test]
fn test_payload_is_left_empty_when_the_slugs_would_not_fit() {
    let few: std::collections::BTreeSet<String> = ["o/p".to_string()].into();
    assert_eq!(super::payload(&few), "[\"o/p\"]");
    let many = (0..2000).map(|i| format!("owner/project-{i}")).collect();
    assert_eq!(super::payload(&many), "");
}
