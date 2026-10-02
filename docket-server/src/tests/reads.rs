use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::app;
use crate::auth::Keys;

use docket_migration::scratch::Scratch;

const SEED: &str = r#"
INSERT INTO projects (slug, keys, themes, skills, created_at, updated_at) VALUES ('o/p',
  '[{"key":"T","kind":"work"},{"key":"Q","kind":"decision"},{"key":"A","kind":"audit"},
    {"key":"PK","kind":"package"},{"key":"CON","kind":"concept"}]',
  '[{"name":"sync"}]', '{}', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, tags, rank, complexity, scope, theme,
                   group_name, body, opened_at, updated_at) VALUES
  (1, 'o/p', 'T', 1, 'Plain fix', 'open', 'agent', '[]', NULL, NULL, NULL, NULL, NULL, '', 'o01', 'u01'),
  (2, 'o/p', 'T', 2, 'Urgent fix', 'open', 'agent', '["high"]', NULL, NULL, NULL, NULL, NULL, '', 'o02', 'u02'),
  (3, 'o/p', 'T', 3, 'Ranked fix', 'open', 'agent', '[]', 1, 'low', NULL, NULL, NULL, '', 'o03', 'u03'),
  (4, 'o/p', 'PK', 1, 'Package done', 'open', 'agent', '[]', NULL, NULL, NULL, NULL, NULL, '', 'o04', 'u04'),
  (6, 'o/p', 'PK', 2, 'Package live', 'open', 'agent', '["critical"]', NULL, NULL, NULL, NULL, NULL, '', 'o06', 'u06'),
  (7, 'o/p', 'T', 5, 'Member open', 'open', 'agent', '[]', NULL, NULL, NULL, NULL, NULL, '', 'o07', 'u07'),
  (8, 'o/p', 'Q', 1, 'Open question', 'open', 'user', '[]', NULL, NULL, NULL, 'sync', NULL, '', 'o08', 'u08'),
  (12, 'o/p', 'T', 8, 'Inbox item', 'open', 'agent', '[]', NULL, NULL, 'inbox', NULL, NULL, '', 'o12', 'u12'),
  (13, 'o/p', 'T', 9, 'Held theme', 'open', 'agent', '[]', NULL, NULL, NULL, 'roadmap', NULL, 'sync', 'o13', 'u13'),
  (15, 'o/p', 'CON', 1, 'Concept', 'open', 'agent', '[]', NULL, NULL, NULL, NULL, NULL, '', 'o15', 'u15');
INSERT INTO items (rid, project, key, num, title, state, resolution, tags, rank, group_name, body, opened_at, updated_at) VALUES
  (5, 'o/p', 'T', 4, 'Member closed', 'done', 'abc', '[]', NULL, NULL, '', 'o05', 'u05'),
  (14, 'o/p', 'T', 10, 'Dropped one', 'dropped', 'dup', '[]', NULL, 'g', '', 'o14', 'u14'),
  (17, 'o/p', 'T', 11, 'Synced thing', 'done', 'ok', '[]', 2, 'g', 'the sync queue body', 'o17', 'u17');
INSERT INTO items (rid, project, key, num, title, state, turn, decision, decided_at, tags, opened_at, updated_at) VALUES
  (9, 'o/p', 'Q', 2, 'Decided question', 'open', 'agent', 'Derived from CID1: keep it', 'd09', '[]', 'o09', 'u09');
INSERT INTO items (rid, project, key, num, title, state, turn, wait_on, wait_item, wait_ref, wait_since, tags, opened_at, updated_at) VALUES
  (10, 'o/p', 'T', 6, 'Waiting', 'open', 'agent', 'item', 1, 'T1', 'w10', '[]', 'o10', 'u10'),
  (16, 'o/p', 'A', 1, 'Plan', 'open', 'agent', 'condition', NULL, 'all closed', 'w16', '[]', 'o16', 'u16');
INSERT INTO items (rid, project, key, num, title, state, turn, claim_branch, claim_host, claim_since, tags, opened_at, updated_at) VALUES
  (11, 'o/p', 'T', 7, 'Claimed', 'open', 'agent', 'b', 'h', 'c11', '[]', 'o11', 'u11');
INSERT INTO links (rid, kind, to_rid) VALUES
  (5, 'opened', 4), (7, 'opened', 6), (1, 'related', 15), (2, 'opened', 16), (3, 'opened', 2);
INSERT INTO links (rid, kind, to_path, to_line) VALUES (17, 'cites_file', 'src/sync.py', 3);
INSERT INTO events (uid, project, rid, at, host, kind, note, data) VALUES
  ('e1', 'o/p', 1, 'e01', 'devbox', 'decided', 'chose X', '{"derived": "CID2"}');
INSERT INTO search (rid, id, title, body, files) VALUES
  (1, 'T1', 'Plain fix thing', '', ''),
  (13, 'T9', 'Held theme', 'sync', ''),
  (17, 'T11', 'Synced thing', 'the sync queue body', 'src/sync.py');
"#;

async fn seeded() -> (Router, Scratch) {
    let s = Scratch::new(2).await;
    s.seed(SEED).await;
    (app(&s.db, Keys::parse("devbox agent secret").unwrap()), s)
}

async fn get(path: &str) -> (StatusCode, Value) {
    let uri = match (path.contains("project="), path.contains('?')) {
        (true, _) => path.to_string(),
        (false, true) => format!("{path}&project=o/p"),
        (false, false) => format!("{path}?project=o/p"),
    };
    let req = Request::builder()
        .uri(uri)
        .header(AUTHORIZATION, "Bearer secret")
        .body(Body::empty())
        .unwrap();
    let (app, _db) = seeded().await;
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn ids(path: &str) -> Vec<String> {
    let (status, body) = get(path).await;
    assert_eq!(status, StatusCode::OK, "{path}: {body}");
    body.as_array()
        .unwrap_or_else(|| panic!("{path} gave {body}"))
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn test_next_orders_by_priority_then_age_alone() {
    assert_eq!(
        ids("/next").await,
        ["T2", "T1", "T3", "T5", "Q2", "T8", "T9"]
    );
    assert_eq!(ids("/next?n=2").await, ["T2", "T1"]);
    let (_, rows) = get("/next?n=1").await;
    assert_eq!(rows[0]["priority"], "high");
    assert_eq!(rows[0]["word"], "ready");
    assert_eq!(rows[0]["tags"], json!(["high"]));
    assert!(rows[0].get("eff_tier").is_none());
    assert_eq!(get("/show/T8").await.1["word"], "ready");
}

#[tokio::test]
async fn test_next_by_role() {
    assert_eq!(
        ids("/next?role=work").await,
        ["T2", "T1", "T3", "T5", "T8", "T9"]
    );
    assert_eq!(ids("/next?role=plan").await, ["Q2"]);
    assert!(ids("/next?role=audit").await.is_empty());
}

#[tokio::test]
async fn test_next_narrows_by_key_priority_under_theme_and_complexity() {
    assert_eq!(ids("/next?theme=road").await, ["T9"]);
    assert_eq!(
        ids("/next?key=t").await,
        ["T2", "T1", "T3", "T5", "T8", "T9"]
    );
    assert!(ids("/next?key=CON").await.is_empty());
    assert!(ids("/next?key=PK").await.is_empty());
    assert_eq!(ids("/next?priority=high").await, ["T2"]);
    assert_eq!(ids("/next?under=A1").await, ["T2", "T3"]);
    assert_eq!(ids("/next?under=con1").await, ["T1"]);
    assert_eq!(ids("/next?complexity=low").await, ["T3"]);
}

#[tokio::test]
async fn test_next_refuses_bad_choices_and_unknown_targets() {
    assert_eq!(get("/next?role=x").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        get("/next?priority=urgent").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(get("/next?under=zz").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(get("/next?under=T99").await.0, StatusCode::NOT_FOUND);
    assert_eq!(get("/next?project=o/none").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_owner_lists_todo_questions_research() {
    assert_eq!(ids("/todo").await, ["Q1"]);
    assert_eq!(ids("/questions").await, ["Q1"]);
    assert_eq!(ids("/questions?theme=SYN").await, ["Q1"]);
    assert!(ids("/questions?theme=road").await.is_empty());
    assert_eq!(ids("/research").await, ["Q2"]);
    let (_, todo) = get("/todo").await;
    assert_eq!(todo[0]["word"], "parked");
    assert_eq!(todo[0]["group"], Value::Null);
}

#[tokio::test]
async fn test_state_lists_waiting_wip_done_dropped_groups() {
    assert_eq!(ids("/waiting").await, ["A1", "T6"]);
    assert_eq!(ids("/waiting?on=item").await, ["T6"]);
    assert_eq!(get("/waiting?on=x").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(ids("/wip").await, ["T7"]);
    assert!(ids("/wip?host=other").await.is_empty());
    assert_eq!(ids("/done").await, ["T11", "T4"]);
    assert_eq!(ids("/done?n=1&key=t").await, ["T11"]);
    assert_eq!(ids("/dropped").await, ["T10"]);
    assert_eq!(ids("/groups").await, ["T11", "T10"]);
    assert_eq!(ids("/groups?name=none").await, Vec::<String>::new());
    let (_, wip) = get("/wip").await;
    assert_eq!(wip[0]["word"], "building");
}

#[tokio::test]
async fn test_status_counts_tickets_in_total_and_every_key() {
    let (status, body) = get("/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["project"], "o/p");
    assert_eq!(body["host"], "devbox");
    assert_eq!(body["total"], 15);
    assert_eq!(
        body["by_word"],
        json!({"ready": 7, "done": 2, "parked": 1, "blocked": 2, "building": 1,
               "dropped": 1, "standing": 1})
    );
    assert_eq!(body["by_key"]["PK"], json!({"ready": 1, "building": 1}));
    assert_eq!(body["by_key"]["CON"], json!({"standing": 1}));
}

#[tokio::test]
async fn test_derived_merges_questions_and_ticket_events_newest_first() {
    let (status, body) = get("/derived").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!([
            {"id": "T1", "state": "open", "at": "e01", "title": "Plain fix", "chose": "chose X", "basis": "CID2"},
            {"id": "Q2", "state": "open", "at": "d09", "title": "Decided question", "chose": "keep it", "basis": "CID1"}
        ])
    );
    assert_eq!(ids("/derived?n=1").await, ["T1"]);
}

#[tokio::test]
async fn test_search_ranks_with_snippets_and_refuses_bad_queries() {
    let (status, rows) = get("/search?q=sync").await;
    assert_eq!(status, StatusCode::OK);
    let mut found: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    found.sort_unstable();
    assert_eq!(found, ["T11", "T9"]);
    for r in rows.as_array().unwrap() {
        assert!(r["snippet"].as_str().unwrap().contains("[sync]"), "{r}");
        assert_eq!(r["snip"], r["snippet"]);
        assert!(r["score"].is_number());
    }
    assert_eq!(ids("/search?q=sync&state=done").await, ["T11"]);
    assert_eq!(ids("/search?q=sync%20queue").await, ["T11"]);
    assert_eq!(
        ids("/search?q=sync%20OR%20plain&raw=true&key=t&n=1")
            .await
            .len(),
        1
    );
    assert_eq!(get("/search?q=%20").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        get("/search?q=(((&raw=true").await,
        (StatusCode::OK, json!([]))
    );
    assert_eq!(
        get("/search?q=a&state=closed").await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn test_similar_matches_title_words_and_cited_files_and_skips_itself() {
    assert_eq!(ids("/similar/T11").await, ["T1"]);
    assert_eq!(ids("/similar/t11?state=open").await, ["T1"]);
    assert!(ids("/similar/T11?state=done").await.is_empty());
    assert!(ids("/similar/T4").await.is_empty());
    assert_eq!(get("/similar/T99").await.0, StatusCode::NOT_FOUND);
    let (_, rows) = get("/similar/T11").await;
    assert!(rows[0].get("snippet").is_none());
    assert!(rows[0]["snip"].is_string());
}

#[tokio::test]
async fn test_search_finds_each_part_of_a_path_a_hyphenated_word_and_text_in_brackets() {
    let (app, db) = seeded().await;
    db.seed(
        "INSERT INTO items (rid, project, key, num, title, state, turn, tags, body, opened_at, updated_at) \
         VALUES (30, 'o/p', 'T', 30, 'Parts', 'open', 'agent', '[]', 'x', 'o30', 'u30'); \
         INSERT INTO search (rid, id, title, body, files) VALUES (30, 'T30', 'Parts', \
         'Reads `web/src/a.rs:4`, the Content-Range header and <b>bold words</b> at user@host.', 'web/src/a.rs')",
    )
    .await;
    for q in [
        "a.rs",
        "web/src",
        "src/a.rs:4",
        "range",
        "bold",
        "words",
        "host",
    ] {
        let req = Request::builder()
            .uri(format!("/search?project=o/p&q={q}"))
            .header(AUTHORIZATION, "Bearer secret")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let found: Value = serde_json::from_slice(&bytes).unwrap();
        let ids: Vec<&str> = found
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["id"].as_str())
            .collect();
        assert!(ids.contains(&"T30"), "{q}: {found}");
    }
}
