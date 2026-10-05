use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Request, header::AUTHORIZATION};
use tower::ServiceExt;

use docket_core::dump::DumpPage;
use docket_migration::scratch::Scratch;

use super::*;
use crate::app;
use crate::auth::Keys;

async fn page(db: &DatabaseConnection, since: i64) -> DumpPage {
    let req = Request::get(format!("/dump?since={since}"))
        .header(AUTHORIZATION, "Bearer k")
        .body(Body::empty())
        .unwrap();
    let resp = app(db, Keys::parse("box owner k").unwrap())
        .oneshot(req)
        .await
        .unwrap();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn note(tx: &Tx, text: &str) {
    tx.event("o/p", None, "queue", Some(text), None, None)
        .await
        .unwrap();
}

/// Scenario: a write starts while another, which took a lower event seq, is still open.
/// Expected behaviour: the second waits for the first, so a dump page read between them skips nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tx_begin_interleaved_writes_reach_the_dump_in_seq_order() {
    let s = Scratch::new(4).await;
    s.seed("INSERT INTO projects (slug, created_at, updated_at) VALUES ('o/p', 'c', 'u')")
        .await;
    let first = Tx::begin(&s.db, "a").await.unwrap();
    note(&first, "first").await;
    let db = s.db.clone();
    let second = tokio::spawn(async move {
        let tx = Tx::begin(&db, "b").await.unwrap();
        note(&tx, "second").await;
        tx.commit().await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    let before = page(&s.db, 0).await;
    first.commit().await.unwrap();
    second.await.unwrap();
    let after = page(&s.db, before.cursor).await;
    let mut seen: Vec<String> = before
        .events
        .iter()
        .chain(&after.events)
        .filter_map(|e| e.note.clone())
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        ["first", "second"],
        "before {before:?}, after {after:?}"
    );
}

#[tokio::test]
async fn test_tx_item_locks_the_row_until_commit() {
    let s = Scratch::new(4).await;
    s.seed(
        "INSERT INTO projects (slug, created_at, updated_at) VALUES ('o/p', 'c', 'u'); \
         INSERT INTO items (project, key, num, title, state, turn, opened_at, updated_at) \
         VALUES ('o/p', 'T', 1, 'One', 'open', 'agent', 'o', 'u')",
    )
    .await;
    let tx = Tx::begin(&s.db, "a").await.unwrap();
    tx.item("o/p", "T1").await.unwrap();
    let blocked = s
        .db
        .execute_unprepared("SET lock_timeout = '200ms'; UPDATE items SET title='Two' WHERE rid=1")
        .await;
    assert!(blocked.is_err(), "the row was not locked");
    tx.commit().await.unwrap();
}

/// Scenario: a pool hands out a connection that was used a moment ago.
/// Expected behaviour: the connection is health-checked only once it has been idle for 30 s.
#[test]
fn test_options_ping_a_connection_only_after_it_has_idled() {
    let o = crate::options("postgres://u:p@h/d");
    assert_eq!(
        o.get_test_before_acquire_if_idle_for(),
        Some(Duration::from_secs(30))
    );
    assert!(!o.get_test_before_acquire());
}
