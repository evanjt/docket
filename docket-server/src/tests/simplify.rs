use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use sea_orm::ConnectionTrait;
use serde_json::Value;
use tower::ServiceExt;

use super::*;
use crate::app;
use crate::auth::Keys;

use docket_migration::scratch::Scratch;

const GATE: &str = "everything it opened is closed";

/// A project mid-way through the package flow: open, done and reviewed packages, a plan that held its
/// audit round, items in the inbox and later, and work held outside a release.
const SEED: &str = r#"
INSERT INTO projects (slug, keys, themes, skills, created_at, updated_at) VALUES ('o/p',
  '[{"key":"T","kind":"work"},{"key":"B","kind":"work"},{"key":"A","kind":"audit"},
    {"key":"PK","kind":"package","meaning":"packages"},{"key":"STY","kind":"story"},
    {"key":"CON","kind":"concept"}]',
  '[{"name":"Theme 1"}]', '{"release":"1.0 2026-10-01","owner":"Ada"}', 'c', 'u'),
  ('o/q', '[{"key":"T","kind":"work"}]', '[]', '{}', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, tags, scope, theme, body, opened_at, updated_at) VALUES
  (1, 'o/p', 'PK', 1, 'Shelves have one owner', 'open', 'agent', '[]', NULL, NULL, '', 'o', 'u'),
  (2, 'o/p', 'T', 1, 'Oven runs hot twice', 'open', 'agent', '[]', NULL, NULL, '', 'o', 'u'),
  (3, 'o/p', 'PK', 2, 'Oven owner', 'open', 'agent', '["high"]', NULL, NULL, '', 'o', 'u'),
  (6, 'o/p', 'A', 1, 'The pantry plan', 'open', 'agent', '[]', NULL, NULL, '', 'o', 'u'),
  (8, 'o/p', 'A', 2, 'Plan still building', 'open', 'agent', '[]', NULL, NULL, '', 'o', 'u'),
  (9, 'o/p', 'T', 4, 'Port mixer', 'open', 'agent', '[]', NULL, NULL, '', 'o', 'u'),
  (10, 'o/p', 'T', 5, 'Seen on the way', 'open', 'agent', '[]', 'inbox', NULL, '', 'o', 'u'),
  (11, 'o/p', 'T', 6, 'After the release', 'open', 'agent', '["high"]', 'later', NULL, '', 'o', 'u'),
  (12, 'o/p', 'T', 7, 'Next version work', 'open', 'agent', '[]', NULL, '1.1', '', 'o', 'u'),
  (13, 'o/p', 'T', 8, 'Release work', 'open', 'agent', '[]', NULL, 'Theme 1', '', 'o', 'u'),
  (14, 'o/p', 'T', 9, 'Urgent next version', 'open', 'agent', '["critical"]', NULL, '1.1', '', 'o', 'u'),
  (15, 'o/p', 'STY', 1, 'A baker corrects a step', 'open', 'user', '[]', NULL, NULL, '', 'o', 'u'),
  (16, 'o/p', 'CON', 1, 'Crusts', 'open', 'agent', '[]', NULL, NULL, '', 'o', 'u'),
  (17, 'o/p', 'B', 1, 'Gap the audit found', 'open', 'agent', '[]', NULL, NULL, '', 'o', 'u'),
  (21, 'o/p', 'T', 12, 'Docs work', 'open', 'agent', '[]', NULL, 'docs', '', 'o', 'u'),
  (22, 'o/p', 'T', 13, 'Much later work', 'open', 'agent', '[]', NULL, '1.10', '', 'o', 'u'),
  (20, 'o/q', 'T', 1, 'Elsewhere', 'open', 'agent', '[]', 'later', NULL, '', 'o', 'u');
INSERT INTO items (rid, project, key, num, title, state, resolution, tags, body, opened_at, updated_at) VALUES
  (4, 'o/p', 'T', 2, 'Oven runs hot', 'done', 'abc1234', '[]', '', 'o', 'u'),
  (7, 'o/p', 'PK', 4, 'Closed package', 'done', 'def5678', '[]', '', 'o', 'u'),
  (18, 'o/p', 'T', 10, 'Pantry work', 'done', 'fed4321', '[]', '', 'o', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, claim_branch, claim_host, claim_since, tags, body, opened_at, updated_at) VALUES
  (5, 'o/p', 'PK', 3, 'Reviewed package', 'open', 'agent', 'audit/pk3-r1', 'devbox', 'c', '[]', '', 'o', 'u');
INSERT INTO items (rid, project, key, num, title, state, resolution, tags, body, opened_at, updated_at) VALUES
  (19, 'o/p', 'T', 11, 'Reviewed work', 'done', 'aaa1111', '[]', '', 'o', 'u');
UPDATE items SET wait_on='condition', wait_ref='everything it opened is closed', wait_since='w' WHERE rid IN (6, 8);
INSERT INTO links (rid, kind, to_rid) VALUES
  (2, 'opened', 1), (4, 'opened', 3), (19, 'opened', 5), (18, 'opened', 6), (17, 'opened', 6),
  (9, 'opened', 8), (2, 'opened', 15), (2, 'related', 16);
INSERT INTO events (uid, project, rid, at, host, kind, note, data) VALUES
  ('e1', 'o/p', 6, 'a1', 'devbox', 'released', 'audit: gaps: B1', '{"audit": "gaps", "gaps": ["B1"]}'),
  ('e2', 'o/p', 1, 'a2', 'devbox', 'released', 'audit: gaps: T1', '{"audit": "gaps", "gaps": ["T1"]}');
"#;

async fn seeded() -> Scratch {
    let s = Scratch::new(2).await;
    s.seed(SEED).await;
    s
}

async fn rows(s: &Scratch, text: &str) -> Vec<Value> {
    let found =
        s.db.query_all_raw(crate::store::sql(text, vec![]))
            .await
            .unwrap();
    found
        .iter()
        .map(|r| r.try_get_by_index::<Value>(0).unwrap())
        .collect()
}

async fn items(s: &Scratch) -> Value {
    rows(
        s,
        "SELECT jsonb_agg(to_jsonb(i) - 'updated_at' ORDER BY rid) FROM items i",
    )
    .await
    .remove(0)
}

async fn item(s: &Scratch, id: &str) -> Value {
    rows(
        s,
        &format!("SELECT to_jsonb(i) FROM items i WHERE project='o/p' AND id='{id}'"),
    )
    .await
    .remove(0)
}

async fn next(s: &Scratch, role: &str) -> Vec<String> {
    let app = app(&s.db, Keys::parse("devbox agent k").unwrap());
    let req = Request::builder()
        .uri(format!("/next?project=o/p&role={role}"))
        .header(AUTHORIZATION, "Bearer k")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let rows: Value = serde_json::from_slice(&bytes).unwrap();
    rows.as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

fn ids(list: &[String]) -> Vec<&str> {
    list.iter().map(String::as_str).collect()
}

#[tokio::test]
async fn test_a_dry_run_reports_the_write_and_writes_nothing() {
    let s = seeded().await;
    let before = items(&s).await;
    let dry = simplify(&s.db, "testbox", false).await.unwrap();
    assert_eq!(items(&s).await, before);
    let wrote = simplify(&s.db, "testbox", true).await.unwrap();
    assert_eq!(dry, wrote);
    let p = wrote.iter().find(|c| c.project == "o/p").unwrap();
    assert_eq!(p.plan_keys, ["PK"]);
    assert_eq!(p.open_plans, 3);
    assert_eq!(ids(&p.released), ["PK3"]);
    assert_eq!(ids(&p.closed), ["A1"]);
    assert_eq!(ids(&p.gated), ["PK1"]);
    assert_eq!(ids(&p.unscoped), ["T5", "T6"]);
    assert_eq!(ids(&p.lowered), ["T5"]);
    assert_eq!(p.release.as_deref(), Some("1.0 2026-10-01"));
    assert_eq!(p.releases.as_deref(), Some("1.0 1.1 1.10"));
    let q = wrote.iter().find(|c| c.project == "o/q").unwrap();
    assert_eq!(ids(&q.unscoped), ["T1"]);
}

#[tokio::test]
async fn test_packages_become_plans_and_the_ceremony_closes_out() {
    let s = seeded().await;
    let counts = "SELECT jsonb_object_agg(project, n) FROM (SELECT project, count(*) AS n FROM items GROUP BY project) c";
    let before = rows(&s, counts).await;
    simplify(&s.db, "testbox", true).await.unwrap();
    assert_eq!(rows(&s, counts).await, before);

    let keys = rows(&s, "SELECT keys FROM projects WHERE slug='o/p'").await;
    let pk = keys[0]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["key"] == "PK")
        .unwrap();
    assert_eq!(pk["kind"], "audit");
    assert_eq!(pk["meaning"], "packages");

    assert_eq!(item(&s, "PK1").await["wait_ref"], GATE);
    assert_eq!(item(&s, "PK3").await["claim_branch"], Value::Null);
    assert_eq!(item(&s, "PK4").await["state"], "done");
    assert_eq!(item(&s, "A1").await["state"], "done");
    assert!(
        item(&s, "A1").await["resolution"]
            .as_str()
            .unwrap()
            .contains("A7")
    );
    assert_eq!(item(&s, "A2").await["wait_ref"], GATE);
    assert_eq!(next(&s, "audit").await, ["PK2", "PK3"]);
    assert!(next(&s, "work").await.contains(&"B1".to_string()));

    let scoped = rows(
        &s,
        "SELECT count(*)::text::jsonb FROM items WHERE scope IS NOT NULL",
    )
    .await;
    assert_eq!(scoped[0], 0);
    assert_eq!(item(&s, "T5").await["tags"], serde_json::json!(["low"]));
    assert_eq!(item(&s, "T6").await["tags"], serde_json::json!(["high"]));
    assert_eq!(item(&s, "T7").await["tags"], serde_json::json!([]));
    assert_eq!(item(&s, "T12").await["tags"], serde_json::json!([]));
    assert_eq!(item(&s, "T8").await["tags"], serde_json::json!([]));
    assert_eq!(
        item(&s, "T9").await["tags"],
        serde_json::json!(["critical"])
    );
    let skills = rows(&s, "SELECT skills FROM projects WHERE slug='o/p'").await;
    assert_eq!(
        skills[0],
        serde_json::json!({"owner": "Ada", "releases": "1.0 1.1 1.10"})
    );

    let notes = rows(
        &s,
        "SELECT jsonb_agg(note) FROM events WHERE note LIKE '%A7%'",
    )
    .await;
    assert!(notes[0].as_array().unwrap().len() >= 6, "{}", notes[0]);
}

#[tokio::test]
async fn test_a_second_run_changes_nothing() {
    let s = seeded().await;
    simplify(&s.db, "testbox", true).await.unwrap();
    let after = items(&s).await;
    let again = simplify(&s.db, "testbox", true).await.unwrap();
    assert!(again.iter().all(Change::is_empty), "{again:?}");
    assert_eq!(items(&s).await, after);
}

#[test]
fn test_releases_are_the_current_then_the_version_themes_in_order() {
    assert_eq!(
        releases_of(
            "1.0",
            &[
                "1.2", "docs", "1.10", "1.1", "1.2", "v2.0", "Theme 1", "1.0"
            ]
        ),
        ["1.0", "1.1", "1.2", "1.10", "v2.0"]
    );
    assert_eq!(releases_of("1.0", &[]), ["1.0"]);
}
