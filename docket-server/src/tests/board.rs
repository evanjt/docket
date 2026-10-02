use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::app;
use crate::auth::Keys;

use docket_migration::scratch::Scratch;

const SEED: &str = r#"
INSERT INTO projects (slug, keys, themes, skills, remotes, created_at, updated_at) VALUES ('o/p',
  '[{"key":"T","kind":"work"},{"key":"Q","kind":"decision"},{"key":"A","kind":"audit"},
    {"key":"PK","kind":"package"},{"key":"CON","kind":"concept"},{"key":"CID","kind":"idea"}]',
  '[{"name":"sync"}]', '{"stale_claim":"1"}', '["git@h:o/p.git"]', 'c', 'u'),
  ('o/q', '[{"key":"T","kind":"work"}]', '[]', '{}', '[]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, tags, theme, group_name, body, opened_at, updated_at) VALUES
  (1, 'o/p', 'T', 1, 'Fix the sync', 'open', 'agent', '["high"]', NULL, 'g',
     '- **Touches.** src/a.rs, src/b.rs' || chr(10) || '`src/a.rs:3` serves A1#1', '2026-01-01T00:00:00Z', 'u1'),
  (2, 'o/p', 'PK', 1, 'Package', 'open', 'agent', '["critical"]', NULL, NULL, '', '2026-01-01T00:00:00Z', 'u2'),
  (3, 'o/p', 'CON', 1, 'Concept', 'open', 'agent', '[]', NULL, NULL, '', '2026-01-01T00:00:00Z', 'u3'),
  (4, 'o/p', 'A', 1, 'Plan', 'open', 'agent', '[]', NULL, NULL,
     '- **Principles.**' || chr(10) || '  1. One owner.' || chr(10) || '  2. No gaps.', '2026-01-01T00:00:00Z', 'u4'),
  (6, 'o/p', 'T', 3, 'Held theme', 'open', 'agent', '[]', 'roadmap', 'g', 'x', '2026-01-01T00:00:00Z', 'u6'),
  (7, 'o/p', 'Q', 1, 'Two of them', 'open', 'user', '[]', NULL, NULL, '', '2026-01-01T00:00:00Z', 'u7');
INSERT INTO items (rid, project, key, num, title, state, resolution, tags, body, opened_at, updated_at) VALUES
  (5, 'o/p', 'T', 2, 'Done one', 'done', 'abc1234', '[]', '`src/a.rs`', '2026-01-01T00:00:00Z', 'u5');
INSERT INTO items (rid, project, key, num, title, state, turn, claim_branch, claim_host, claim_since, claim_job, tags, body, opened_at, updated_at) VALUES
  (8, 'o/p', 'T', 4, 'Claimed', 'open', 'agent', 'audit/t4', 'devbox', '2026-01-01T00:00:00Z', 'j1', '[]',
     '- **Touches.** src/a.rs', '2026-01-01T00:00:00Z', 'u8'),
  (9, 'o/p', 'T', 5, 'Also claimed', 'open', 'agent', 'audit/t5', 'h', '2026-01-02T00:00:00Z', NULL, '[]',
     '- **Touches.** src/a.rs', '2026-01-01T00:00:00Z', 'u9');
INSERT INTO items (rid, project, key, num, title, state, turn, wait_on, wait_item, wait_ref, wait_since, tags, opened_at, updated_at) VALUES
  (10, 'o/p', 'T', 6, 'Waits', 'open', 'agent', 'item', 1, 'T1', '2026-01-01T00:00:00Z', '[]', '2026-01-01T00:00:00Z', 'u10');
INSERT INTO items (rid, project, key, num, title, state, turn, resolution, tags, body, opened_at, updated_at) VALUES
  (11, 'o/p', 'A', 2, 'Due plan', 'open', 'agent', NULL, '[]', 'x', '2026-01-01T00:00:00Z', 'u11'),
  (12, 'o/p', 'T', 7, 'Due work', 'done', NULL, 'def5678', '[]', 'x', '2026-01-01T00:00:00Z', 'u12');
INSERT INTO links (rid, kind, to_rid) VALUES
  (1, 'opened', 2), (5, 'opened', 2), (1, 'related', 3), (3, 'related', 1), (1, 'opened', 4), (5, 'opened', 4),
  (8, 'opened', 2), (8, 'opened', 4), (12, 'opened', 11);
INSERT INTO links (rid, kind, to_path, to_line) VALUES
  (1, 'cites_file', 'src/a.rs', 3), (5, 'cites_file', 'src/a.rs', NULL), (6, 'cites_test', 'tests/t.rs', NULL);
INSERT INTO events (uid, project, rid, at, host, kind, note, data) VALUES
  ('e1', 'o/p', 1, '2026-01-01T00:00:00Z', 'devbox', 'opened', 'Fix the sync', NULL),
  ('e2', 'o/p', 1, '2026-01-01T00:00:01Z', 'devbox', 'claimed', NULL, '{"role": "build"}'),
  ('e3', 'o/p', 8, '2026-01-01T00:00:02Z', 'devbox', 'claimed', NULL, '{"model": "m1", "role": "review"}'),
  ('e4', 'o/p', 5, '2026-01-01T00:00:03Z', 'devbox', 'closed', 'abc1234', NULL);
INSERT INTO search (rid, id, title, body, files) VALUES
  (1, 'T1', 'Fix the sync', 'serves A1#1', 'src/a.rs'),
  (5, 'T2', 'Done one', 'mentions T1 here', 'src/a.rs');
"#;

struct Seeded {
    app: Router,
    db: Scratch,
}

impl Seeded {
    async fn new() -> Self {
        let scratch = Scratch::new(2).await;
        scratch.seed(SEED).await;
        let keys = Keys::parse("devbox owner ownerkey\nother agent agentkey").unwrap();
        Self {
            app: app(&scratch.db, keys),
            db: scratch,
        }
    }

    async fn send(
        &self,
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
        let resp = self
            .app
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        self.send(Method::GET, uri, "ownerkey", None).await
    }

    async fn ok(&self, uri: &str) -> Value {
        let (status, body) = self.get(uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        body
    }
}

fn ids(v: &Value) -> Vec<&str> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn test_whoami_names_the_key_host() {
    let s = Seeded::new().await;
    assert_eq!(
        s.ok("/whoami").await,
        json!({"host": "devbox", "owner": true})
    );
    let (_, who) = s.send(Method::GET, "/whoami", "agentkey", None).await;
    assert_eq!(who, json!({"host": "other", "owner": false}));
}

#[tokio::test]
async fn test_counts_every_project_by_state_and_last_event() {
    let s = Seeded::new().await;
    let c = s.ok("/counts").await;
    assert_eq!(
        c[0],
        json!({"slug": "o/p", "open": 10, "done": 2, "dropped": 0, "last_event": "2026-01-01T00:00:03Z"})
    );
    assert_eq!(c[1]["slug"], "o/q");
    assert_eq!(c[1]["last_event"], Value::Null);
}

#[tokio::test]
async fn test_flow_counts_tickets_in_the_order_first_met() {
    let s = Seeded::new().await;
    let f = s.ok("/flow?project=o/p").await;
    assert_eq!(f["host"], "devbox");
    assert_eq!(f["total"], 11);
    let words: Vec<&str> = f["by_word"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p[0].as_str().unwrap())
        .collect();
    assert!(
        words.contains(&"building") && words.contains(&"blocked") && words.contains(&"standing")
    );
    let pk = f["by_key"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p[0] == "PK")
        .unwrap();
    assert_eq!(pk[1], json!([["building", 1]]));
}

#[tokio::test]
async fn test_log_reads_one_items_events_with_their_data() {
    let s = Seeded::new().await;
    let log = s.ok("/log/t1?project=o/p").await;
    assert_eq!(log.as_array().unwrap().len(), 2);
    assert_eq!(log[0]["data"], Value::Null);
    assert_eq!(log[1]["data"], json!({"role": "build"}));
    assert_eq!(s.get("/log/T99?project=o/p").await.0, StatusCode::NOT_FOUND);
    assert_eq!(s.get("/log/zz?project=o/p").await.0, StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_deps_lists_each_tie_and_the_mentions() {
    let s = Seeded::new().await;
    let d = s.ok("/deps/T1?project=o/p").await;
    assert_eq!(ids(&d["holds"]), ["T6"]);
    assert_eq!(ids(&d["related"]), ["CON1"]);
    assert_eq!(ids(&d["opened_by"]), ["PK1", "A1"]);
    assert_eq!(ids(&d["group"]), ["T3"]);
    assert_eq!(ids(&d["same_files"]), ["T2"]);
    assert_eq!(d["same_files"][0]["shared"], 1);
    assert_eq!(ids(&d["mentions"]), ["T2"]);
    assert!(d["mentions"][0]["score"].is_number());
    assert!(d.get("waits_on").is_none());
    let w = s.ok("/deps/T6?project=o/p").await;
    assert_eq!(ids(&w["waits_on"]), ["T1"]);
}

#[tokio::test]
async fn test_deps_words_another_projects_row_by_its_own_keys() {
    let s = Seeded::new().await;
    let seed = "INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('o/r', \
                '[{\"key\":\"P\",\"kind\":\"package\"},{\"key\":\"T\",\"kind\":\"work\"}]', 'c', 'u'); \
                INSERT INTO items (rid, project, key, num, title, state, turn, tags, body, opened_at, updated_at) VALUES \
                (20, 'o/r', 'P', 1, 'Other package', 'open', 'agent', '[]', '', 'o', 'u'), \
                (21, 'o/r', 'T', 1, 'Other member', 'open', 'agent', '[]', '', 'o', 'u'); \
                INSERT INTO links (rid, kind, to_rid) VALUES (21, 'opened', 20); \
                INSERT INTO links (rid, kind, to_path) VALUES (20, 'cites_file', 'src/a.rs');";
    s.db.seed(seed).await;
    let d = s.ok("/deps/T1?project=o/p").await;
    let other = d["same_files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "P1")
        .unwrap();
    assert_eq!(other["word"], "building");
}

#[tokio::test]
async fn test_context_reads_what_show_prints_beside_the_row() {
    let s = Seeded::new().await;
    let c = s.ok("/context/T1?project=o/p").await;
    assert_eq!(c["priority"], "high");
    assert_eq!(c["package"]["id"], "PK1");
    assert_eq!(
        c["package"]["progress"],
        json!({"done": 1, "total": 3, "live": 1})
    );
    assert_eq!(c["holds"], json!(["T6"]));
    assert_eq!(c["concepts"], json!(["CON1"]));
    let p = s.ok("/context/PK1?project=o/p").await;
    assert_eq!(ids(&p["members"]), ["T1", "T2", "T4"]);
    let con = s.ok("/context/CON1?project=o/p").await;
    assert_eq!(con["standing"]["total"], 1);
    assert_eq!(con["standing"]["words"], json!({"ready": 1}));
}

#[tokio::test]
async fn test_files_and_citations_read_the_cited_paths() {
    let s = Seeded::new().await;
    let f = s.ok("/files?project=o/p&prefix=src").await;
    // A citation with no line sorts before one with a line, as SQLite orders nulls.
    assert_eq!(ids(&f), ["T2", "T1"]);
    assert_eq!(f[1]["path"], "src/a.rs");
    assert_eq!(f[1]["line"], 3);
    assert_eq!(
        ids(&s.ok("/files?project=o/p&prefix=src&state=done").await),
        ["T2"]
    );
    let c = s.ok("/citations?project=o/p").await;
    assert_eq!(c.as_array().unwrap().len(), 3);
    assert_eq!(
        c[0],
        json!({"id": "T1", "state": "open", "path": "src/a.rs"})
    );
    assert_eq!(
        s.ok("/citations?project=o/p&open_only=true")
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn test_graph_nodes_carry_the_rid_events_name() {
    let s = Seeded::new().await;
    let g = s.ok("/graph?project=o/p").await;
    let items = s
        .ok("/items?filter=%7B%22project%22%3A%22o%2Fp%22%7D&range=%5B0%2C999%5D")
        .await;
    for n in g["nodes"].as_array().unwrap() {
        let stored = items
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == n["id"])
            .unwrap();
        assert_eq!(n["rid"], stored["rid"], "{n}");
    }
}

#[tokio::test]
async fn test_graph_holds_nodes_and_edges_once_each() {
    let s = Seeded::new().await;
    let g = s.ok("/graph?project=o/p").await;
    assert_eq!(g["nodes"].as_array().unwrap().len(), 12);
    let edges = g["edges"].as_array().unwrap();
    let related = edges.iter().filter(|e| e["kind"] == "related").count();
    assert_eq!(related, 1);
    assert!(
        edges
            .iter()
            .any(|e| e["kind"] == "cites" && e["to"] == "tests/t.rs")
    );
}

fn node<'a>(g: &'a Value, id: &str) -> &'a Value {
    g["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id)
        .unwrap()
}

#[tokio::test]
async fn test_graph_nodes_carry_progress_and_due_as_the_core_counts_them() {
    let s = Seeded::new().await;
    let g = s.ok("/graph?project=o/p").await;
    assert_eq!(
        node(&g, "A1")["progress"],
        json!({ "done": 1, "total": 3, "live": 1 })
    );
    assert_eq!(node(&g, "A1")["due"], false);
    assert_eq!(
        node(&g, "A2")["progress"],
        json!({ "done": 1, "total": 1, "live": 0 })
    );
    assert_eq!(node(&g, "A2")["due"], true);
    assert_eq!(
        node(&g, "PK1")["progress"],
        json!({ "done": 1, "total": 3, "live": 1 })
    );
    assert!(node(&g, "T1").get("progress").is_none(), "{g}");
    assert!(node(&g, "T1").get("due").is_none(), "{g}");
}

#[tokio::test]
async fn test_graph_counts_what_a_plans_tickets_opened_in_turn() {
    let s = Seeded::new().await;
    s.db.seed(
        "INSERT INTO items (rid, project, key, num, title, state, turn, tags, opened_at, updated_at) VALUES \
         (13, 'o/p', 'T', 8, 'Opened by the due work', 'open', 'agent', '[]', '2026-01-01T00:00:00Z', 'u13'); \
         INSERT INTO links (rid, kind, to_rid) VALUES (13, 'opened', 12);",
    )
    .await;
    let g = s.ok("/graph?project=o/p").await;
    assert_eq!(
        node(&g, "A2")["progress"],
        json!({ "done": 1, "total": 2, "live": 0 })
    );
    assert_eq!(node(&g, "A2")["due"], false);
    let m = s.ok("/summary?project=o/p").await;
    assert_eq!(m["due"], json!([]));
}

#[tokio::test]
async fn test_check_finds_an_open_audit_and_bare_bodies() {
    let s = Seeded::new().await;
    let c = s.ok("/check?project=o/p").await;
    let kinds: Vec<&str> = c
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"open_audit"), "{c}");
    assert!(kinds.contains(&"no_body"), "{c}");
    assert_eq!(s.ok("/check?project=o/q").await, json!([]));
}

#[tokio::test]
async fn test_shares_flags_idle_claims_and_files_they_share() {
    let s = Seeded::new().await;
    let sh = s.ok("/shares?project=o/p").await;
    assert_eq!(ids(&sh), ["T4", "T5"]);
    assert_eq!(sh[0]["shares"][0]["holder"], "T5");
    assert_eq!(sh[0]["shares"][0]["paths"], json!(["src/a.rs"]));
    assert!(sh[0]["flag"].as_str().unwrap().starts_with("no event for"));
}

#[tokio::test]
async fn test_summary_reads_claims_plans_and_due_audits() {
    let s = Seeded::new().await;
    let m = s.ok("/summary?project=o/p").await;
    for gone in [
        "release",
        "held_themes",
        "hour",
        "busy",
        "running",
        "jobs",
        "packages",
    ] {
        assert!(m.get(gone).is_none(), "{gone}: {m}");
    }
    assert_eq!(ids(&m["claims"]), ["T4", "T5"]);
    assert_eq!(m["claims"][0]["branch"], "audit/t4");
    assert_eq!(m["claims"][0]["host"], "devbox");
    assert_eq!(m["claims"][0]["title"], "Claimed");
    assert!(
        m["claims"][0]["flag"]
            .as_str()
            .unwrap()
            .starts_with("no event for")
    );
    assert_eq!(ids(&m["plans"]), ["A1", "A2"]);
    assert_eq!(
        (
            &m["plans"][0]["done"],
            &m["plans"][0]["total"],
            &m["plans"][0]["live"]
        ),
        (&json!(1), &json!(3), &json!(1))
    );
    assert_eq!(ids(&m["due"]), ["A2"]);
}

#[tokio::test]
async fn test_audit_sections_principles_and_bound_items() {
    let s = Seeded::new().await;
    let a = s.ok("/audit?project=o/p&id=A1").await;
    assert_eq!(a["target"]["id"], "A1");
    assert_eq!(ids(&a["rows"]), ["T1", "T2", "T4"]);
    assert_eq!(a["principles"], json!([[1, "One owner."], [2, "No gaps."]]));
    assert_eq!(a["served"]["1"], json!([{"id": "T1", "state": "open"}]));
    let pk = s.ok("/audit?project=o/p&id=PK1").await;
    assert_eq!(pk["breakdown"]["progress"]["total"], 3);
    let con = s.ok("/audit?project=o/p&id=CON1").await;
    assert_eq!(ids(&con["rows"]), ["T1"]);
    assert_eq!(
        ids(&s.ok("/audit?project=o/p&group=g").await["rows"]).len(),
        2
    );
    let orphans = s.ok("/audit?project=o/p&orphans=true").await;
    let mut found = ids(&orphans["rows"]);
    found.sort_unstable();
    assert_eq!(found, ["Q1", "T3", "T4", "T5", "T6"]);
    assert_eq!(s.get("/audit?project=o/p").await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_project_matches_by_remote_or_name_and_creates_the_rest() {
    let s = Seeded::new().await;
    let post = |body: Value| s.send(Method::POST, "/do/project", "agentkey", Some(body));
    let (_, m) = post(json!({"remotes": ["git@h:o/p.git"], "basename": "x", "create": true})).await;
    assert_eq!(m, json!({"slug": "o/p", "how": "matched", "matches": []}));
    let (_, m) = post(json!({"basename": "q", "create": false})).await;
    assert_eq!(m["slug"], "o/q");
    let (_, m) = post(json!({"candidate": "n/new", "remotes": ["git@h:n/new.git"], "basename": "new", "create": true})).await;
    assert_eq!(m["how"], "created");
    let (_, row) = s
        .get("/projects?filter=%7B%22slug%22%3A%22n%2Fnew%22%7D")
        .await;
    assert_eq!(row[0]["remotes"], json!(["git@h:n/new.git"]));
    assert_eq!(row[0]["keys"].as_array().unwrap().len(), 9);
    let (_, m) = post(json!({"basename": "zzz", "create": false})).await;
    assert_eq!(m["how"], "none");
}

#[tokio::test]
async fn test_project_reports_a_checkout_that_could_be_several() {
    let s = Seeded::new().await;
    let created = json!({"candidate": "x/p", "basename": "elsewhere", "create": true});
    s.send(Method::POST, "/do/project", "ownerkey", Some(created))
        .await;
    let (_, m) = s
        .send(
            Method::POST,
            "/do/project",
            "ownerkey",
            Some(json!({"basename": "p", "create": true})),
        )
        .await;
    assert_eq!(
        m,
        json!({"slug": null, "how": "ambiguous", "matches": ["o/p", "x/p"]})
    );
}

#[tokio::test]
async fn test_reindex_rebuilds_every_items_search_row() {
    let s = Seeded::new().await;
    let (status, out) = s
        .send(
            Method::POST,
            "/do/reindex",
            "ownerkey",
            Some(json!({"project": "o/p"})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out, json!({"project": "o/p", "items": 12}));
    let hits = s.ok("/search?project=o/p&q=Claimed").await;
    let mut found = ids(&hits);
    found.sort_unstable();
    assert_eq!(found, ["T4", "T5"]);
}
