use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::app;
use crate::auth::Keys;

use docket_migration::scratch::Scratch;

const SEED: &str = r#"
INSERT INTO projects (slug, skills, created_at, updated_at) VALUES ('o/p', '{}', 'c', 'u'),
  ('o/r', '{}', 'c', 'u'),
  ('o/s', '{}', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, resolution, body, opened_at, updated_at) VALUES
  (302, 'o/s', 'T', 1, 'Opened and done', 'done', 'ok', '', 'o02', 'u02'),
  (303, 'o/s', 'T', 2, 'Opened and dropped', 'dropped', 'dup', '', 'o03', 'u03'),
  (304, 'o/s', 'T', 3, 'Related and done', 'done', 'ok', '', 'o04', 'u04'),
  (305, 'o/s', 'T', 4, 'Related and dropped', 'dropped', 'dup', '', 'o05', 'u05');
INSERT INTO items (rid, project, key, num, title, state, body, opened_at, updated_at) VALUES
  (300, 'o/s', 'A', 1, 'Plan', 'open', '', 'o00', 'u00'),
  (301, 'o/s', 'CON', 1, 'Concept', 'open', '', 'o01', 'u01'),
  (306, 'o/s', 'T', 5, 'Opened and waiting', 'open', '', 'o06', 'u06'),
  (307, 'o/s', 'T', 6, 'Related and waiting', 'open', '', 'o07', 'u07'),
  (308, 'o/s', 'T', 7, 'later', 'open', '', 'o08', 'u08');
INSERT INTO dependencies (rid, on_rid, created_at) VALUES (306, 308, 'w06'), (307, 308, 'w07');
UPDATE items SET parent_rid=300 WHERE rid IN (302, 303, 306);
INSERT INTO links (rid, kind, to_rid) VALUES
  (300, 'related', 301), (304, 'related', 301), (305, 'related', 301), (307, 'related', 301);
INSERT INTO items (rid, project, key, num, title, state, body, opened_at, updated_at) VALUES
  (130, 'o/r', 'T', 1, 'Two releases out', 'open', '', 'o30', 'u30'),
  (131, 'o/r', 'T', 2, 'Next release', 'open', '', 'o31', 'u31'),
  (132, 'o/r', 'T', 3, 'This release, low', 'open', '', 'o32', 'u32'),
  (133, 'o/r', 'T', 4, 'Docs', 'open', '', 'o33', 'u33');
INSERT INTO items (rid, project, key, num, title, state, resolution, body, opened_at, updated_at)
  SELECT 200 + n, 'o/r', 'T', 100 + n, 'Closed ' || n, 'done', 'ok',
         '', 'o' || lpad(n::text, 3, '0'), 'u' || lpad((100 - n)::text, 3, '0')
  FROM generate_series(1, 30) AS n;
INSERT INTO items (rid, project, key, num, title, state, resolution, body, opened_at, updated_at) VALUES
  (240, 'o/r', 'T', 140, 'Dropped in next', 'dropped', 'dup', '', 'o40', 'u40'),
  (241, 'o/r', 'T', 141, 'Dropped in current', 'dropped', 'dup', '', 'o41', 'u41');
INSERT INTO items (rid, project, key, num, title, state, resolution, decision, decided_at, opened_at, updated_at) VALUES
  (242, 'o/r', 'Q', 1, 'Decided in next', 'done', 'ok', 'Derived from X: yes', 'd42', 'o42', 'u42'),
  (243, 'o/r', 'Q', 2, 'Decided in current', 'done', 'ok', 'Derived from X: no', 'd43', 'o43', 'u43');
INSERT INTO releases (id, project, name, position) VALUES
  (1, 'o/r', '1.0', 0), (2, 'o/r', '1.1', 1), (3, 'o/r', '1.2', 2);
UPDATE items SET release_id = r.id FROM releases r
  WHERE items.project = 'o/r' AND r.project = 'o/r' AND r.name = CASE
    WHEN items.rid = 130 OR items.rid BETWEEN 201 AND 205 THEN '1.2'
    WHEN items.rid IN (131, 240, 242) OR items.rid BETWEEN 208 AND 230 THEN '1.1'
    WHEN items.rid IN (133, 207) THEN 'docs' ELSE '1.0' END;
INSERT INTO items (rid, project, key, num, title, state, complexity, body, opened_at, updated_at) VALUES
  (1, 'o/p', 'T', 1, 'Plain fix', 'open', NULL, '', 'o01', 'u01'),
  (2, 'o/p', 'T', 2, 'Urgent fix', 'open', NULL, '', 'o02', 'u02'),
  (3, 'o/p', 'T', 3, 'Ranked fix', 'open', 'low', '', 'o03', 'u03'),
  (4, 'o/p', 'PK', 1, 'Package done', 'open', NULL, '', 'o04', 'u04'),
  (6, 'o/p', 'PK', 2, 'Package live', 'open', NULL, '', 'o06', 'u06'),
  (7, 'o/p', 'T', 5, 'Member open', 'open', NULL, '', 'o07', 'u07'),
  (8, 'o/p', 'Q', 1, 'Open lantern choice', 'open', NULL, '', 'o08', 'u08'),
  (12, 'o/p', 'T', 8, 'Inbox item', 'open', NULL, '', 'o12', 'u12'),
  (13, 'o/p', 'T', 9, 'Held theme', 'open', NULL, 'sync', 'o13', 'u13'),
  (15, 'o/p', 'CON', 1, 'Concept', 'open', NULL, '', 'o15', 'u15');
INSERT INTO items (rid, project, key, num, title, state, resolution, body, opened_at, updated_at) VALUES
  (5, 'o/p', 'T', 4, 'Member closed', 'done', 'abc', '', 'o05', 'u05'),
  (14, 'o/p', 'T', 10, 'Dropped one', 'dropped', 'dup', '', 'o14', 'u14'),
  (17, 'o/p', 'T', 11, 'Synced thing', 'done', 'ok', 'the sync queue body', 'o17', 'u17');
INSERT INTO labels (id, project, name) VALUES (1, 'o/p', 'sync'), (2, 'o/p', 'roadmap'), (3, 'o/p', 'group:g');
INSERT INTO item_labels (rid, label_id) VALUES (8, 1), (13, 2), (14, 3), (17, 3);
INSERT INTO items (rid, project, key, num, title, state, decision, decided_at, opened_at, updated_at) VALUES
  (9, 'o/p', 'Q', 2, 'Decided lantern choice', 'open', 'Derived from CID1: keep it', 'd09', 'o09', 'u09');
INSERT INTO items (rid, project, key, num, title, state, opened_at, updated_at) VALUES
  (10, 'o/p', 'T', 6, 'Waiting', 'open', 'o10', 'u10');
INSERT INTO dependencies (rid, on_rid, created_at) VALUES (10, 1, 'w10');
INSERT INTO items (rid, project, key, num, title, state, opened_at, updated_at) VALUES
  (16, 'o/p', 'A', 1, 'Plan', 'open', 'o16', 'u16');
INSERT INTO items (rid, project, key, num, title, state, opened_at, updated_at) VALUES
  (11, 'o/p', 'T', 7, 'Claimed', 'open', 'o11', 'u11');
INSERT INTO assignments (rid, assignee, kind, started_at, branch, host) VALUES
  (8, 'owner', 'ask', 'o08', NULL, ''), (11, 'agent', 'claim', 'c11', 'b', 'h');
UPDATE items SET parent_rid=4 WHERE rid=5;
UPDATE items SET parent_rid=6 WHERE rid=7;
UPDATE items SET parent_rid=16 WHERE rid IN (2, 3);
INSERT INTO links (rid, kind, to_rid) VALUES (1, 'related', 15), (3, 'origin', 2);
INSERT INTO links (rid, kind, to_path, to_line) VALUES (17, 'cites_file', 'src/sync.py', 3);
INSERT INTO events (uid, project, rid, at, host, kind, note, data) VALUES
  ('e1', 'o/p', 1, 'e01', 'devbox', 'decided', 'chose X', '{"derived": "CID2"}');
INSERT INTO search (rid, id, title, body, files) VALUES
  (1, 'T1', 'Plain fix thing', '', ''),
  (8, 'Q1', 'Open lantern choice', '', ''),
  (9, 'Q2', 'Decided lantern choice', '', ''),
  (13, 'T9', 'Held theme', 'sync', ''),
  (17, 'T11', 'Synced thing', 'the sync queue body', 'src/sync.py'),
  (302, 'T1', 'Opened and done', 'lamp', ''),
  (303, 'T2', 'Opened and dropped', 'lamp', ''),
  (304, 'T3', 'Related and done', 'lamp', '');
INSERT INTO projects (slug, skills, created_at, updated_at) VALUES ('o/a', '{}', 'c', 'u');
INSERT INTO areas (id, project, name, description, position, priority) VALUES
  (1, 'o/a', 'lanterns', 'Paper lights', 1, NULL), (2, 'o/a', 'kites', 'Things that fly', 2, 'high');
INSERT INTO items (rid, project, key, num, title, state, resolution, body, opened_at, updated_at, area_id) VALUES
  (500, 'o/a', 'T', 1, 'Fold the tail', 'open', NULL, '', 'o50', 'u50', 2),
  (501, 'o/a', 'T', 2, 'Tie the string', 'done', 'ok', '', 'o51', 'u51', 2),
  (502, 'o/a', 'T', 3, 'Trim the wick', 'open', NULL, '', 'o52', 'u52', 1);
UPDATE items SET priority='high' WHERE rid IN (2, 130);
UPDATE items SET priority='critical' WHERE rid=6;
UPDATE items SET priority='low' WHERE rid=132;
"#;

async fn seeded() -> (Router, Scratch) {
    let s = Scratch::new(2).await;
    s.seed(SEED).await;
    s.seed(crate::tests::TYPES_BY_KEY).await;
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
        ["T2", "PK1", "Q2", "T1", "T3", "T5", "T8", "T9", "CON1"]
    );
    assert_eq!(ids("/next?n=2").await, ["T2", "PK1"]);
    let (_, rows) = get("/next?n=1").await;
    assert_eq!(rows[0]["priority"], "high");
    assert_eq!(rows[0]["word"], "ready");
    assert!(rows[0].get("eff_tier").is_none());
    assert_eq!(get("/show/T8").await.1["word"], "ready");
}

#[tokio::test]
async fn test_next_orders_by_release_then_priority() {
    assert_eq!(ids("/next?project=o/r").await, ["T3", "T2", "T1", "T4"]);
}

#[tokio::test]
async fn test_next_by_role() {
    assert_eq!(
        ids("/next?role=work").await,
        ["T2", "T1", "T3", "T5", "T8", "T9", "CON1"]
    );
    assert_eq!(ids("/next?role=plan").await, ["Q2"]);
    assert_eq!(ids("/next?role=audit").await, ["PK1"]);
    assert_eq!(ids("/next?role=plan,work").await[0], "T2");
}

#[tokio::test]
async fn test_next_rows_carry_their_role_and_what_they_unblock() {
    let (_, rows) = get("/next").await;
    let role = |id: &str| {
        rows.as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()["role"]
            .clone()
    };
    assert_eq!(role("T2"), "build");
    assert_eq!(role("Q2"), "plan");
    assert_eq!(rows[0]["unblocks"], 0);
    let (_, one) = get("/next?id=Q2").await;
    assert_eq!(one.as_array().unwrap().len(), 1);
    assert_eq!(one[0]["id"], "Q2");
}

#[tokio::test]
async fn test_next_narrows_by_key_priority_under_label_and_complexity() {
    assert_eq!(ids("/next?label=roadmap").await, ["T9"]);
    assert_eq!(ids("/next?theme=Roadmap").await, ["T9"]);
    assert!(ids("/next?label=road").await.is_empty());
    assert_eq!(
        ids("/next?key=t").await,
        ["T2", "T1", "T3", "T5", "T8", "T9"]
    );
    assert_eq!(ids("/next?key=CON").await, ["CON1"]);
    assert_eq!(ids("/next?key=PK").await, ["PK1"]);
    assert_eq!(ids("/next?priority=high").await, ["T2"]);
    assert_eq!(ids("/next?under=A1").await, ["T2", "T3"]);
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
    assert_eq!(ids("/todo").await, ["Q2", "Q1"]);
    assert_eq!(ids("/questions").await, ["Q1"]);
    assert_eq!(ids("/questions?label=SYNC").await, ["Q1"]);
    assert_eq!(ids("/questions?theme=sync").await, ["Q1"]);
    assert!(ids("/questions?theme=road").await.is_empty());
    assert_eq!(ids("/research").await, ["Q2"]);
    let (_, todo) = get("/todo").await;
    assert_eq!(todo[0]["owner_group"], "derived");
    assert_eq!(todo[1]["word"], "waiting on owner");
    assert_eq!(todo[1]["owner_group"], "question");
    assert_eq!(todo[1]["group"], Value::Null);
}

#[tokio::test]
async fn test_questions_carry_the_nearest_decided_question() {
    let (_, rows) = get("/questions").await;
    assert_eq!(rows[0]["id"], "Q1");
    assert_eq!(rows[0]["close_to"]["id"], "Q2");
    assert_eq!(
        rows[0]["close_to"]["decision"],
        "Derived from CID1: keep it"
    );
}

#[tokio::test]
async fn test_list_rows_carry_no_body_and_show_does() {
    for path in ["/todo", "/next?n=100", "/questions"] {
        let (status, rows) = get(path).await;
        assert_eq!(status, StatusCode::OK, "{path}: {rows}");
        let rows = rows.as_array().unwrap();
        assert!(!rows.is_empty(), "{path} gave no rows");
        for row in rows {
            assert!(row.get("body").is_none(), "{path}: {row}");
        }
    }
    let (_, shown) = get("/show/T11").await;
    assert_eq!(shown["body"], "the sync queue body");
}

#[tokio::test]
async fn test_waiting_rows_carry_the_word_and_title_of_an_item_target() {
    let (_, rows) = get("/waiting").await;
    let held = rows.as_array().unwrap().iter().find(|r| r["id"] == "T6");
    let target = &held.unwrap()["wait_target"];
    assert_eq!(target["word"], "ready");
    assert_eq!(target["title"], "Plain fix");
    assert!(rows.as_array().unwrap().iter().all(|r| r["id"] != "A1"));
}

#[tokio::test]
async fn test_state_lists_waiting_wip_done_dropped_groups() {
    assert_eq!(ids("/waiting").await, ["T6"]);
    assert_eq!(ids("/waiting?on=item").await, ["T6"]);
    assert_eq!(get("/waiting?on=x").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(ids("/wip").await, ["T7"]);
    assert!(ids("/wip?host=other").await.is_empty());
    assert_eq!(ids("/done").await, ["T11", "T4"]);
    assert_eq!(ids("/done?n=1&key=t").await, ["T11"]);
    assert_eq!(ids("/dropped").await, ["T10"]);
    assert_eq!(ids("/groups").await, ["T10", "T11"]);
    assert_eq!(ids("/groups?name=g").await, ["T10", "T11"]);
    assert_eq!(ids("/groups?name=none").await, Vec::<String>::new());
    let (_, wip) = get("/wip").await;
    assert_eq!(wip[0]["word"], "in progress");
}

#[tokio::test]
async fn test_done_and_dropped_filter_by_release_before_the_page_is_cut() {
    let done = |release: &str, n: u32| format!("/done?project=o/r&release={release}&n={n}");
    assert_eq!(ids(&done("1.2", 20)).await.len(), 5);
    assert_eq!(ids(&done("1.2", 3)).await.len(), 3);
    assert_eq!(ids(&done("1.0", 40)).await, ["Q2", "T106"]);
    assert_eq!(ids("/done?project=o/r&n=20").await.len(), 20);
    assert_eq!(ids("/dropped?project=o/r&release=1.1").await, ["T140"]);
    assert_eq!(ids("/dropped?project=o/r&release=1.0").await, ["T141"]);
    assert_eq!(
        get("/done?project=o/r&release=9.9").await.0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn test_lists_under_a_plan_hold_what_it_opened() {
    let under = |route: &str, id: &str| format!("/{route}?project=o/s&under={id}");
    assert_eq!(ids(&under("done", "A1")).await, ["T1"]);
    assert_eq!(ids(&under("dropped", "A1")).await, ["T2"]);
    assert_eq!(ids(&under("waiting", "A1")).await, ["T5"]);
    assert_eq!(ids("/done?project=o/s").await, ["T3", "T1"]);
    assert_eq!(get(&under("done", "T99")).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_search_under_a_plan_holds_what_it_opened() {
    let under = |id: &str| format!("/search?project=o/s&q=lamp&state=any&under={id}");
    assert_eq!(ids("/search?project=o/s&q=lamp&state=any").await.len(), 3);
    let mut opened = ids(&under("A1")).await;
    opened.sort_unstable();
    assert_eq!(opened, ["T1", "T2"]);
    assert_eq!(get(&under("T99")).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_under_lists_every_item_a_plan_opened_whatever_its_word() {
    let under = |id: &str| format!("/under?project=o/s&under={id}");
    assert_eq!(ids(&under("A1")).await, ["T1", "T2", "T5"]);
    assert_eq!(get("/under?project=o/s").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(get(&under("T99")).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_derived_filters_by_release_before_the_page_is_cut() {
    assert_eq!(ids("/derived?project=o/r&release=1.1").await, ["Q1"]);
    assert_eq!(ids("/derived?project=o/r&release=1.0&n=1").await, ["Q2"]);
}

#[tokio::test]
async fn test_status_counts_tickets_in_total_and_every_key() {
    let (status, body) = get("/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["project"], "o/p");
    assert_eq!(body["host"], "devbox");
    assert_eq!(body["total"], 17);
    assert_eq!(
        body["by_word"],
        json!({"ready": 8, "done": 2, "waiting on owner": 1, "blocked": 1, "building": 2,
               "in progress": 1, "dropped": 1, "audit due": 1})
    );
    assert_eq!(body["by_key"]["PK"], json!({"audit due": 1, "building": 1}));
    assert_eq!(body["by_key"]["CON"], json!({"ready": 1}));
}

#[tokio::test]
async fn test_derived_merges_questions_and_ticket_events_newest_first() {
    let (status, body) = get("/derived").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!([
            {"id": "T1", "state": "open", "at": "e01", "title": "Plain fix", "chose": "chose X", "basis": "CID2"},
            {"id": "Q2", "state": "open", "at": "d09", "title": "Decided lantern choice", "chose": "keep it", "basis": "CID1"}
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
        "INSERT INTO items (rid, project, key, num, title, state, body, opened_at, updated_at) \
         VALUES (30, 'o/p', 'T', 30, 'Parts', 'open', 'x', 'o30', 'u30'); \
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

const METRICS_SEED: &str = r#"
INSERT INTO projects (slug, skills, created_at, updated_at) VALUES ('o/m', '{"prices":"opus=10:20"}', 'c', 'u');
INSERT INTO releases (id, project, name, position, target_date) VALUES
  (10, 'o/m', '1.0', 0, '2026-12-01'), (11, 'o/m', '1.1', 1, NULL);
INSERT INTO items (rid, project, key, num, title, state, release_id, resolution, body, opened_at, updated_at) VALUES
  (400, 'o/m', 'A', 1, 'Plan', 'open', 10, NULL, '', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z'),
  (401, 'o/m', 'T', 1, 'Shipped', 'done', 10, 'ok', '', '2020-01-01T00:00:00Z', '2020-01-03T00:00:00Z'),
  (402, 'o/m', 'T', 2, 'Waiting for work', 'open', 10, NULL, '', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z'),
  (403, 'o/m', 'T', 3, 'Later', 'open', 11, NULL, '', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z');
UPDATE items SET parent_rid=400 WHERE rid IN (401, 402);
INSERT INTO events (uid, project, rid, at, host, kind) VALUES
  ('m1', 'o/m', 401, '2020-01-03T00:00:00Z', 'devbox', 'closed');
INSERT INTO assignments (rid, assignee, kind, started_at, ended_at, outcome, host, runner, model, tokens_in, tokens_out, cost_reported) VALUES
  (401, 'agent', 'claim', '2020-01-02T00:00:00Z', '2020-01-02T01:00:00Z', 'landed', 'devbox', 'claude', 'opus', 100, 50, 1.5);
"#;

async fn metrics_of(query: &str) -> (StatusCode, Value) {
    let s = Scratch::new(2).await;
    s.seed(METRICS_SEED).await;
    s.seed(crate::tests::TYPES_BY_KEY).await;
    let app = app(&s.db, Keys::parse("devbox agent secret").unwrap());
    let req = Request::builder()
        .uri(format!("/metrics?project=o/m&{query}"))
        .header(AUTHORIZATION, "Bearer secret")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn test_metrics_of_a_release_counts_its_items_and_forecasts_from_the_burn() {
    let (status, body) = metrics_of("scope=release&release=1.0").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["progress"]["done"], 1);
    assert_eq!(body["progress"]["total"], 3);
    assert_eq!(body["progress"]["open"], json!({"building": 1, "ready": 1}));
    assert_eq!(body["lead_time"]["median"], 2 * 86_400);
    assert_eq!(body["cycle_time"]["median"], 86_400);
    assert_eq!(body["time_spent"]["agent"], 3600);
    assert_eq!(body["cost"]["tokens_in"], 100);
    assert_eq!(body["cost"]["cost_reported"], 1.5);
    assert!((body["cost"]["money"].as_f64().unwrap() - 0.002).abs() < 1e-9);
    assert_eq!(body["throughput"].as_array().unwrap().len(), 28);
    let forecast = &body["forecast"];
    assert_eq!(forecast["open"], 2);
    assert_eq!(forecast["converging"], false);
    assert_eq!(forecast["target"], "2026-12-01");
    assert_eq!(forecast["late"], true);
}

#[tokio::test]
async fn test_metrics_of_a_plan_reads_what_it_holds_and_refuses_what_is_not_offered() {
    let (status, body) = metrics_of("scope=plan&plan=A1").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["progress"]["total"], 2);
    assert!(body.get("forecast").is_none());
    assert_eq!(metrics_of("scope=plan").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(metrics_of("scope=sprint").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        metrics_of("scope=plan&plan=T99").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(metrics_of("release=9.9").await.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_metrics_of_the_releases_gives_one_row_each_with_what_a_later_release_holds() {
    let s = Scratch::new(2).await;
    s.seed(METRICS_SEED).await;
    s.seed(crate::tests::TYPES_BY_KEY).await;
    s.seed("INSERT INTO dependencies (rid, on_rid, created_at) VALUES (402, 403, 'c')")
        .await;
    let app = app(&s.db, Keys::parse("devbox agent secret").unwrap());
    let req = Request::builder()
        .uri("/metrics?project=o/m&scope=releases")
        .header(AUTHORIZATION, "Bearer secret")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    let rows = body["releases"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{body}");
    assert_eq!(rows[0]["name"], "1.0");
    assert_eq!(rows[0]["closed"], 1);
    assert_eq!(rows[0]["open"], 2);
    assert_eq!(rows[0]["held_later"], 1);
    assert_eq!(rows[0]["pace"], rows[0]["forecast"]["burn"]);
    assert_eq!(rows[1]["name"], "1.1");
    assert_eq!(rows[1]["held_later"], 0);
}

#[tokio::test]
async fn test_next_and_audit_narrow_to_one_area_ignoring_case() {
    assert_eq!(ids("/next?project=o/a&area=Kites").await, ["T1"]);
    let (status, body) = get("/audit?project=o/a&area=kites").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["area"]["name"], "kites");
    assert_eq!(body["area"]["description"], "Things that fly");
    assert_eq!(body["area"]["priority"], "high");
    let rows: Vec<&str> = body["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(rows, ["T2", "T1"]);
    let (_, item) = get("/audit?project=o/a&id=T3").await;
    assert_eq!(item["area"]["name"], "lanterns");
    assert_eq!(item["area"]["description"], "Paper lights");
    let (status, _) = get("/audit?project=o/a&area=boats").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
