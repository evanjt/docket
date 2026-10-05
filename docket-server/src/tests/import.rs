use std::sync::atomic::{AtomicU32, Ordering};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use docket_migration::scratch::Scratch;

use super::*;
use crate::app;
use crate::auth::Keys;

const SCHEMA: &str = include_str!("fixtures/sqlite.sql");

/// Rows with gaps in every key, an item waiting on a later one, links out of rid order, event data,
/// and the local tables.
const SEED: &str = r#"
INSERT INTO projects (slug, keys, themes, skills, remotes, created_at, updated_at) VALUES
  ('o/p', '[{"key": "T", "kind": "work", "meaning": "tasks", "turn": "agent"}, {"key": "PK", "kind": "package"}]',
   '[{"name": "sync", "note": ""}]', '{"merge": "rébase"}', '["git@example.com:o/p.git"]', 'c', 'u'),
  ('o/q', '[]', '[]', '{}', '[]', 'c', 'u');
INSERT INTO roots VALUES ('box', '/w/p', 'o/p', 'b', 'bind');
INSERT INTO items (rid, project, key, num, title, state, turn, tags, body, opened_at, updated_at) VALUES
  (3, 'o/p', 'T', 2, 'Later', 'open', 'agent', '[]', '', 'o3', 'u3'),
  (7, 'o/p', 'PK', 1, 'Package', 'open', 'agent', '[]', '', 'o7', 'u7');
INSERT INTO items (rid, project, key, num, title, state, turn, wait_on, wait_item, wait_ref, wait_since, tags,
                   body, opened_at, updated_at) VALUES
  (1, 'o/p', 'T', 1, 'Waits on a later one', 'open', 'agent', 'item', 3, 'T2', 'w', '["high"]',
   'Cites `src/a.rs:4` and `tests/t.rs`.', 'o1', 'u1');
INSERT INTO items (rid, project, key, num, title, state, resolution, superseded_by, tags, opened_at, updated_at)
  VALUES (2, 'o/p', 'T', 3, 'Replaced', 'dropped', 'superseded by T2', 3, '[]', 'o2', 'u2');
INSERT INTO links (rowid, rid, kind, to_rid, to_path, to_line) VALUES
  (5, 1, 'opened', 7, NULL, NULL), (9, 1, 'cites_file', NULL, 'src/a.rs', 4),
  (2, 1, 'cites_test', NULL, 'tests/t.rs', NULL);
INSERT INTO events (seq, uid, project, rid, at, host, branch, kind, note, data) VALUES
  (4, 'e1', 'o/p', 1, 'a1', 'box', NULL, 'claimed', NULL, '{"model": "m", "role": "build"}'),
  (10, 'e2', 'o/p', NULL, 'a2', 'box', 'main', 'queue', 'n', NULL);
INSERT INTO pending_dump VALUES (1, 'o/p');
INSERT INTO chores VALUES ('sync', '', 1.5, 0.25, 'ok', 0, 1);
INSERT INTO meta VALUES ('k', 'v');
"#;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A SQLite file holding the schema and the rows given, removed when the guard drops.
struct SqliteFile(std::path::PathBuf);

impl SqliteFile {
    async fn new(rows: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("docket-import-{}-{n}.db", std::process::id()));
        std::fs::remove_file(&path).ok();
        let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
            .await
            .unwrap();
        db.execute_unprepared(SCHEMA).await.unwrap();
        db.execute_unprepared(rows).await.unwrap();
        db.close().await.unwrap();
        Self(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for SqliteFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}

async fn call(app: &Router, method: Method, uri: &str, body: Option<Value>) -> Value {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, "Bearer k");
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let body = body.map_or_else(Body::empty, |v| Body::from(v.to_string()));
    let resp = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let out = serde_json::from_slice(&bytes).unwrap_or_default();
    assert_eq!(status, StatusCode::OK, "{uri}: {out}");
    out
}

fn count(counts: &Counts, table: &str, project: &str) -> i64 {
    counts
        .get(&(table.to_string(), project.to_string()))
        .copied()
        .unwrap_or(0)
}

#[tokio::test]
async fn test_import_copies_every_row_with_its_keys() {
    let file = SqliteFile::new(SEED).await;
    let s = Scratch::new(2).await;
    let counts = import(file.path(), &s.db).await.unwrap();
    assert_eq!(count(&counts, "projects", "o/q"), 1);
    assert_eq!(count(&counts, "items", "o/p"), 4);
    assert_eq!(count(&counts, "links", "o/p"), 3);
    assert_eq!(count(&counts, "events", "o/p"), 2);
    for table in ["roots", "pending_dump"] {
        assert_eq!(count(&counts, table, "o/p"), 1, "{table}");
    }
    assert_eq!(count(&counts, "chores", ""), 1);
    assert_eq!(count(&counts, "meta", ""), 1);

    let app = app(&s.db, Keys::parse("box owner k").unwrap());
    let shown = call(&app, Method::GET, "/show/T1?project=o/p", None).await;
    assert_eq!(shown["wait_ref"], "T2");
    assert_eq!(shown["tags"], json!(["high"]));
    assert_eq!(shown["opened"], json!(["PK1"]));
    let cites = json!([
        {"path": "src/a.rs", "line": 4, "kind": "cites_file"},
        {"path": "tests/t.rs", "line": null, "kind": "cites_test"},
    ]);
    assert_eq!(shown["cites"], cites);
    let dropped = call(&app, Method::GET, "/show/T3?project=o/p", None).await;
    assert_eq!(dropped["superseded_by"], "T2");
    let log = call(&app, Method::GET, "/log/T1?project=o/p", None).await;
    assert_eq!(log[0]["data"], json!({"model": "m", "role": "build"}));
}

#[tokio::test]
async fn test_import_builds_search_and_moves_each_key_past_the_copy() {
    let file = SqliteFile::new(SEED).await;
    let s = Scratch::new(2).await;
    import(file.path(), &s.db).await.unwrap();
    let app = app(&s.db, Keys::parse("box owner k").unwrap());
    let found = call(&app, Method::GET, "/search?project=o/p&q=tests/t.rs", None).await;
    assert_eq!(found[0]["id"], "T1");
    let deps = call(&app, Method::GET, "/deps/T1?project=o/p", None).await;
    assert_eq!(deps["waits_on"][0]["id"], "T2");

    let body =
        json!({"project": "o/p", "key": "T", "title": "After the copy", "body": "`src/b.rs:1`"});
    let opened = call(&app, Method::POST, "/do/new", Some(body)).await;
    assert_eq!(opened["item"]["id"], "T4");
    let page = call(&app, Method::GET, "/dump?since=10", None).await;
    assert_eq!(page["cursor"], 11);
    let rows = call(
        &app,
        Method::GET,
        "/items?filter=%7B%22id%22%3A%22T4%22%7D",
        None,
    )
    .await;
    assert_eq!(rows[0]["rid"], 8);
    let links = call(
        &app,
        Method::GET,
        "/links?sort=%5B%22id%22%2C%22DESC%22%5D",
        None,
    )
    .await;
    assert_eq!(links[0]["id"], 10);
}

#[tokio::test]
async fn test_import_refuses_a_database_that_holds_rows() {
    let file = SqliteFile::new(SEED).await;
    let s = Scratch::new(2).await;
    import(file.path(), &s.db).await.unwrap();
    let refused = import(file.path(), &s.db).await.unwrap_err();
    assert!(refused.contains("already holds"), "{refused}");
}

#[tokio::test]
async fn test_import_refuses_json_that_does_not_parse_and_keeps_nothing() {
    let file = SqliteFile::new(
        "INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('o/p', 'not json', 'c', 'u');",
    )
    .await;
    let s = Scratch::new(2).await;
    let refused = import(file.path(), &s.db).await.unwrap_err();
    assert!(refused.contains("projects.keys"), "{refused}");
    let left: i64 = crate::store::scalar(&s.db, "SELECT COUNT(*) FROM projects", vec![])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn test_compare_names_each_count_that_differs() {
    let key = |t: &str, p: &str| (t.to_string(), p.to_string());
    let had = Counts::from([(key("items", "o/p"), 3), (key("links", "o/p"), 2)]);
    let has = Counts::from([(key("items", "o/p"), 3), (key("events", "o/p"), 1)]);
    let refused = compare(&had, &has).unwrap_err();
    assert_eq!(
        refused,
        "the counts differ: events of \"o/p\": 0 in SQLite, 1 in Postgres; \
         links of \"o/p\": 2 in SQLite, 0 in Postgres"
    );
    assert!(compare(&had, &had).is_ok());
}
