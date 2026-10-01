use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use futures_util::StreamExt;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use tower::ServiceExt;

use crate::app;
use crate::auth::Keys;

use docket_core::SCHEMA;
const SEED: &str = "INSERT INTO projects (slug, keys, created_at, updated_at) \
                    VALUES ('o/p', '[{\"key\":\"T\",\"kind\":\"work\"}]', 'c', 'u');";

/// A database in a file of its own, since a change is only seen across connections to one file.
async fn file_db(name: &str) -> (std::path::PathBuf, DatabaseConnection) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("docket-{name}-{}-{nanos}.db", std::process::id()));
    let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .unwrap();
    db.execute_unprepared(SCHEMA).await.unwrap();
    db.execute_unprepared(SEED).await.unwrap();
    (path, db)
}

fn remove(path: &std::path::Path) {
    for tail in ["", "-wal", "-shm"] {
        std::fs::remove_file(format!("{}{tail}", path.display())).ok();
    }
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

#[tokio::test]
async fn test_changes_without_a_key_is_401() {
    let (path, db) = file_db("changes-401").await;
    for key in [None, Some("wrong")] {
        assert_eq!(subscribe(&db, key).await.status(), StatusCode::UNAUTHORIZED);
    }
    remove(&path);
}

#[tokio::test]
async fn test_changes_says_hello_then_change_after_another_connection_commits() {
    let (path, db) = file_db("changes-commit").await;
    let resp = subscribe(&db, Some("secret")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["content-type"], "text/event-stream");
    let mut stream = resp.into_body().into_data_stream();
    let hello = frame(&mut stream, Duration::from_secs(2)).await.unwrap();
    assert!(hello.starts_with("event: hello"), "{hello}");
    // Two ticks of the watcher with nothing written: no change.
    assert_eq!(frame(&mut stream, super::EVERY * 3).await, None);

    let writer = Database::connect(format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    writer
        .execute_unprepared("UPDATE projects SET updated_at='later' WHERE slug='o/p'")
        .await
        .unwrap();
    let change = frame(&mut stream, Duration::from_secs(3)).await.unwrap();
    assert!(change.starts_with("event: change\ndata: 1"), "{change}");

    writer
        .execute_unprepared("UPDATE projects SET updated_at='later still' WHERE slug='o/p'")
        .await
        .unwrap();
    let again = frame(&mut stream, Duration::from_secs(3)).await.unwrap();
    assert!(again.starts_with("event: change\ndata: 2"), "{again}");
    remove(&path);
}
