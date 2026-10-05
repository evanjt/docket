use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::app;
use crate::auth::Keys;

use docket_migration::scratch::Scratch as Database;
const KEYS: &str = r#"[{"key": "T", "kind": "work", "meaning": "tasks", "turn": "agent"}, {"key": "B", "kind": "work", "meaning": "bugs", "turn": "agent"}, {"key": "Q", "kind": "decision", "meaning": "questions", "turn": "user"}, {"key": "I", "kind": "research", "meaning": "investigations", "turn": "agent"}, {"key": "A", "kind": "audit", "meaning": "audits", "turn": "agent"}, {"key": "STY", "kind": "story", "meaning": "stories", "turn": "agent"}, {"key": "CON", "kind": "concept", "meaning": "concepts", "turn": "agent"}, {"key": "CID", "kind": "idea", "meaning": "central ideas", "turn": "agent"}, {"key": "PK", "kind": "package", "meaning": "packages", "turn": "agent"}]"#;
const SLUG: &str = "test/proj";
const GATE: &str = "everything it opened is closed";

/// A scratch database and project, every call made as the owner on one branch unless told otherwise.
struct Scratch {
    app: Router,
    db: Database,
}

impl Scratch {
    async fn new() -> Self {
        let db = Database::new(2).await;
        db.seed(&format!(
            "INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('{SLUG}', '{KEYS}', 'c', 'u')"
        ))
        .await;
        let keys = Keys::parse("testbox owner ownerkey\ndevbox agent agentkey").unwrap();
        Self {
            app: app(&db.db, keys),
            db,
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

    async fn post_as(&self, key: &str, verb: &str, mut body: Value) -> (StatusCode, Value) {
        let fields = body.as_object_mut().unwrap();
        fields.entry("project").or_insert(json!(SLUG));
        fields.entry("branch").or_insert(json!("audit/t-1"));
        self.send(Method::POST, &format!("/do/{verb}"), key, Some(body))
            .await
    }

    async fn post(&self, verb: &str, body: Value) -> (StatusCode, Value) {
        self.post_as("ownerkey", verb, body).await
    }

    async fn ok(&self, verb: &str, body: Value) -> Value {
        let (status, out) = self.post(verb, body.clone()).await;
        assert_eq!(status, StatusCode::OK, "{verb} {body}: {out}");
        out
    }

    async fn refused(&self, verb: &str, body: Value) -> String {
        let (status, out) = self.post(verb, body.clone()).await;
        assert_eq!(status, StatusCode::CONFLICT, "{verb} {body}: {out}");
        out["refused"].as_str().unwrap().to_string()
    }

    async fn status(&self, verb: &str, body: Value) -> StatusCode {
        self.post(verb, body).await.0
    }

    async fn item(&self, id: &str) -> Value {
        let (status, out) = self
            .send(
                Method::GET,
                &format!("/show/{id}?project={SLUG}"),
                "ownerkey",
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "show {id}: {out}");
        out
    }

    async fn word(&self, id: &str) -> String {
        self.item(id).await["word"].as_str().unwrap().to_string()
    }

    async fn events(&self, id: &str) -> Vec<Value> {
        let filter = format!("{{\"project\":\"{SLUG}\",\"id\":\"{id}\"}}");
        let (_, rows) = self
            .send(
                Method::GET,
                &format!("/items?filter={}", urlencode(&filter)),
                "ownerkey",
                None,
            )
            .await;
        let rid = rows[0]["rid"].as_i64().unwrap();
        let filter = format!("{{\"rid\":{rid}}}");
        let (_, events) = self
            .send(
                Method::GET,
                &format!("/events?filter={}&sort=seq", urlencode(&filter)),
                "ownerkey",
                None,
            )
            .await;
        events.as_array().unwrap().clone()
    }

    /// The item's assignment rows, oldest first.
    async fn assignments(&self, id: &str) -> Vec<Value> {
        let filter = format!("{{\"project\":\"{SLUG}\",\"id\":\"{id}\"}}");
        let (_, items) = self
            .send(
                Method::GET,
                &format!("/items?filter={}", urlencode(&filter)),
                "ownerkey",
                None,
            )
            .await;
        let rid = items[0]["rid"].as_i64().unwrap();
        let filter = format!("{{\"rid\":{rid}}}");
        let (status, rows) = self
            .send(
                Method::GET,
                &format!("/assignments?filter={}&sort=id", urlencode(&filter)),
                "ownerkey",
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "assignments of {id}: {rows}");
        rows.as_array().unwrap().clone()
    }

    async fn open(&self, key: &str, title: &str) -> Value {
        self.ok("new", json!({ "key": key, "title": title })).await
    }

    /// An item of a kind kept to read, written as the import leaves it.
    async fn kept(&self, key: &str, num: i64, title: &str) {
        self.db
            .seed(&format!(
                "INSERT INTO items (project, key, num, title, state, turn, tags, body, opened_at, updated_at) \
                 VALUES ('{SLUG}', '{key}', {num}, '{title}', 'open', 'agent', '[]', '', 'o', 'u')"
            ))
            .await;
    }

    async fn open_with_body(&self, key: &str, title: &str, body: &str) -> Value {
        self.ok("new", json!({ "key": key, "title": title, "body": body }))
            .await
    }
}

fn urlencode(text: &str) -> String {
    text.replace('%', "%25")
        .replace('{', "%7B")
        .replace('}', "%7D")
        .replace('"', "%22")
        .replace(':', "%3A")
        .replace(',', "%2C")
        .replace('/', "%2F")
}

fn ids(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap().to_string())
        .collect()
}

// ---- the pathways of test_core.py ----

#[tokio::test]
async fn test_question_pathway() {
    let s = Scratch::new().await;
    s.open("B", "Till freezes at opening").await;
    s.open("Q", "One cache or two").await;
    assert_eq!(s.word("B1").await, "ready");
    assert_eq!(s.word("Q1").await, "parked");
    s.ok("start", json!({ "id": "B1" })).await;
    s.ok("wait", json!({ "id": "B1", "on": "Q1" })).await;
    assert_eq!(s.word("B1").await, "blocked");
    s.refused("start", json!({ "id": "Q1" })).await;
    let out = s
        .ok(
            "answer",
            json!({ "id": "Q1", "decision": "Two, the read path is hot" }),
        )
        .await;
    assert_eq!(ids(&out["released"]), vec!["B1"]);
    assert_eq!(s.word("B1").await, "ready");
    assert_eq!(s.word("Q1").await, "ready");
    assert!(
        s.item("Q1").await["body"]
            .as_str()
            .unwrap()
            .contains("Decision, ")
    );
    s.ok("start", json!({ "id": "Q1" })).await;
    s.refused("start", json!({ "id": "Q1", "branch": "audit/other-2" }))
        .await;
    s.open("B", "Split the cache").await;
    s.ok("link", json!({ "a": ["B2"], "kind": "origin", "b": "Q1" }))
        .await;
    s.ok("close", json!({ "id": "Q1", "resolution": "opened B2" }))
        .await;
    assert_eq!(s.word("Q1").await, "done");
    assert_eq!(s.item("B2").await["origin"], json!(["Q1"]));
    s.refused("start", json!({ "id": "Q1" })).await;
}

#[tokio::test]
async fn test_a_derived_answer_carries_its_basis() {
    let s = Scratch::new().await;
    s.open("Q", "One cache or two").await;
    s.open("Q", "Which colour for the alert").await;
    s.ok("answer", json!({ "id": "Q2", "decision": "Red" }))
        .await;
    s.ok(
        "answer",
        json!({ "id": "Q1", "decision": "Two, the read path is hot", "derived": "CID3, one owner per cache" }),
    )
    .await;
    let q1 = s.item("Q1").await;
    assert_eq!(
        q1["decision"],
        "Derived from CID3, one owner per cache: Two, the read path is hot"
    );
    assert!(
        q1["body"]
            .as_str()
            .unwrap()
            .contains("derived.** Two, the read path is hot. Basis: CID3, one owner per cache")
    );
    s.ok("start", json!({ "id": "Q1" })).await;
    s.ok(
        "close",
        json!({ "id": "Q1", "resolution": "opened nothing" }),
    )
    .await;
    assert_eq!(
        s.refused(
            "answer",
            json!({ "id": "Q2", "decision": "Blue", "derived": "" })
        )
        .await,
        "a derived answer needs its basis: the decision, central idea or practice it follows."
    );
    assert_eq!(
        s.refused(
            "answer",
            json!({ "id": "Q2", "decision": "Blue", "derived": "Q1" })
        )
        .await,
        "Q2 was decided by the owner on ".to_string()
            + s.item("Q2").await["decided_at"].as_str().unwrap()
            + ": Red. A derived answer never replaces that."
    );
}

#[tokio::test]
async fn test_a_derived_answer_with_its_carrier_closes_and_never_lists_for_planning() {
    let s = Scratch::new().await;
    s.open("Q", "Retry or fail on a timeout").await;
    s.open("T", "Retry a timed out order three times").await;
    s.open("Q", "Which retry count").await;
    let out = s
        .ok(
            "answer",
            json!({ "id": "Q1", "decision": "Retry", "derived": "CID2, orders are idempotent", "carried_by": ["T1"] }),
        )
        .await;
    assert_eq!(out["item"]["state"], "done");
    let q1 = s.item("Q1").await;
    assert_eq!(q1["resolution"], "carried by T1");
    assert_eq!(s.item("T1").await["origin"], json!(["Q1"]));
    let (_, research) = s
        .send(
            Method::GET,
            &format!("/research?project={SLUG}"),
            "ownerkey",
            None,
        )
        .await;
    assert!(ids(&research).is_empty(), "{research}");
    assert!(!next(&s, "plan").await.contains(&"Q1".to_string()));
    let (status, out) = s
        .post(
            "answer",
            json!({ "id": "Q2", "decision": "Three", "derived": "CID2", "carried_by": ["T9"] }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{out}");
    assert_eq!(s.item("Q2").await["state"], "open");
}

#[tokio::test]
async fn test_a_release_files_an_item_in_a_release_that_exists_and_none_files_it_in_the_backlog() {
    let s = two_releases().await;
    s.ok(
        "new",
        json!({ "key": "B", "title": "Crash on resume", "release": "current" }),
    )
    .await;
    s.ok(
        "new",
        json!({ "key": "T", "title": "Wider settings page", "release": "1.1", "theme": "upkeep" }),
    )
    .await;
    s.ok("add", json!({ "title": "Hook runs twice" })).await;
    assert_eq!(s.item("B1").await["release"], "1.0");
    assert_eq!(s.item("T1").await["release"], "1.1");
    assert_eq!(s.item("T1").await["theme"], "upkeep");
    assert_eq!(s.item("B2").await["release"], Value::Null);
    assert_eq!(
        s.refused(
            "add",
            json!({ "title": "Lint the hooks", "release": "upkeep" })
        )
        .await,
        "upkeep is not a release here: give current or one of 1.0 1.1"
    );
}

#[tokio::test]
async fn test_edit_to_a_release_the_project_does_not_have_is_refused() {
    let s = two_releases().await;
    filed(&s, "T", "1.0").await;
    assert_eq!(
        s.refused("edit", json!({ "id": "T1", "release": "0.42" }))
            .await,
        "0.42 is not a release here: give current or one of 1.0 1.1"
    );
    s.ok("edit", json!({ "id": "T1", "release": "1.1" })).await;
    assert_eq!(s.item("T1").await["release"], "1.1");
    s.ok("edit", json!({ "id": "T1", "release": "" })).await;
    assert_eq!(s.item("T1").await["release"], Value::Null);
}

#[tokio::test]
async fn test_a_release_ships_only_once_its_open_items_are_closed_or_moved_on() {
    let s = two_releases().await;
    filed(&s, "T", "1.0").await;
    filed(&s, "T", "1.0").await;
    s.ok("close", json!({ "id": "T2", "resolution": "fixed" }))
        .await;
    assert_eq!(
        s.refused("releases", json!({ "action": "ship", "name": "1.0" }))
            .await,
        "1.0 still holds 1 open: T1. Close them, or pass --move-open-to a later release."
    );
    let out = s
        .ok(
            "releases",
            json!({ "action": "ship", "name": "1.0", "to": "1.1" }),
        )
        .await;
    assert_eq!(out["moved"], json!(["T1"]));
    assert_eq!(s.item("T1").await["release"], "1.1");
    assert!(out["releases"][0]["shipped_at"].is_string());
    assert_eq!(
        s.refused("edit", json!({ "id": "T1", "release": "1.0" }))
            .await,
        "1.0 has shipped: give current or one of 1.1"
    );
    filed(&s, "T", "current").await;
    assert_eq!(s.item("T3").await["release"], "1.1");
}

#[tokio::test]
async fn test_releases_are_added_in_version_order_and_kept_by_the_owner() {
    let s = two_releases().await;
    let out = s
        .ok("releases", json!({ "action": "add", "name": "1.0.5" }))
        .await;
    let names: Vec<&str> = out["releases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["1.0", "1.0.5", "1.1"]);
    assert_eq!(
        s.refused("releases", json!({ "action": "add", "name": "1.1" }))
            .await,
        "1.1 is already a release"
    );
    let (status, _) = s
        .post_as(
            "agentkey",
            "releases",
            json!({ "action": "add", "name": "2.0" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    filed(&s, "T", "1.0").await;
    let out = s
        .ok(
            "releases",
            json!({ "action": "move", "name": "1.0", "to": "1.1" }),
        )
        .await;
    assert_eq!(out["moved"], json!(["T1"]));
}

#[tokio::test]
async fn test_a_new_question_lists_the_decided_questions_close_to_it() {
    let s = Scratch::new().await;
    s.open("Q", "Retry policy for the order queue").await;
    s.ok(
        "answer",
        json!({ "id": "Q1", "decision": "Exponential backoff, five tries" }),
    )
    .await;
    s.open("Q", "Colour of the alert banner").await;
    let out = s.open("Q", "Retry policy for the sync queue").await;
    assert_eq!(
        out["decided_like"],
        json!([{ "id": "Q1", "title": "Retry policy for the order queue", "decision": "Exponential backoff, five tries" }])
    );
    s.ok(
        "answer",
        json!({ "id": "Q3", "decision": "The same as Q1", "derived": "Q1" }),
    )
    .await;
    let out = s.open("B", "Retry policy breaks on 500").await;
    assert_eq!(out["decided_like"], json!([]));
    assert_eq!(out["kind"], "work");
}

#[tokio::test]
async fn test_setting_tags_keeps_the_priority_unless_the_list_names_one() {
    let s = Scratch::new().await;
    s.open("B", "Till freezes at opening").await;
    s.ok("priority", json!({ "ids": ["B1"], "tier": "high" }))
        .await;
    s.ok(
        "edit",
        json!({ "id": "B1", "set": [{ "field": "tags", "value": "single" }] }),
    )
    .await;
    let b1 = s.item("B1").await;
    assert_eq!(b1["priority"], "high");
    assert_eq!(b1["tags"], json!(["high", "single"]));
}

#[tokio::test]
async fn test_question_not_yet_decidable() {
    let s = Scratch::new().await;
    s.open("Q", "Which tolerance").await;
    s.ok("reply", json!({ "id": "Q1", "note": "measure first" }))
        .await;
    assert_eq!(s.word("Q1").await, "ready");
    s.open("I", "Measure the oven's drift").await;
    s.ok("wait", json!({ "id": "Q1", "on": "I1" })).await;
    s.ok("start", json!({ "id": "I1" })).await;
    let out = s
        .ok(
            "close",
            json!({ "id": "I1", "resolution": "measured: 12 m" }),
        )
        .await;
    assert_eq!(ids(&out["released"]), vec!["Q1"]);
    assert_eq!(s.word("Q1").await, "ready");
    s.ok("ask", json!({ "id": "Q1", "note": "measured, ready" }))
        .await;
    assert_eq!(s.word("Q1").await, "parked");
}

#[tokio::test]
async fn test_device_pathway() {
    let s = Scratch::new().await;
    s.open("B", "Crash on open").await;
    s.ok("start", json!({ "id": "B1" })).await;
    let out = s
        .ok(
            "ask",
            json!({ "id": "B1", "note": "needs a run on the S22" }),
        )
        .await;
    assert_eq!(out["item"]["word"], "parked");
    assert_eq!(out["item"]["claim_branch"], Value::Null);
    assert_eq!(out["twins"], json!([]));
    assert_eq!(
        s.refused("start", json!({ "id": "B1" })).await,
        "B1 is the owner's turn: needs a run on the S22. reply to it first if you are taking it back."
    );
    s.ok(
        "reply",
        json!({ "id": "B1", "note": "crashes on open, log attached" }),
    )
    .await;
    assert_eq!(s.word("B1").await, "ready");
    s.ok("start", json!({ "id": "B1" })).await;
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    let events = s.events("B1").await;
    let kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        vec!["opened", "claimed", "asked", "replied", "claimed", "closed"]
    );
}

#[tokio::test]
async fn test_close_needs_decision_on_a_question() {
    let s = Scratch::new().await;
    s.open("Q", "Undecided").await;
    assert_eq!(
        s.refused(
            "close",
            json!({ "id": "Q1", "resolution": "opened nothing" })
        )
        .await,
        "Q1 is a decision and has none yet. answer it first, or drop it if it no longer needs one."
    );
}

#[tokio::test]
async fn test_close_without_a_resolution_names_the_branch_it_looked_for() {
    let s = Scratch::new().await;
    s.open("B", "x").await;
    assert_eq!(
        s.refused("close", json!({ "id": "B1" })).await,
        "close needs a resolution: the sha the work landed as, or what closed it. No branch of this item was found to read one from."
    );
    s.ok("start", json!({ "id": "B1" })).await;
    assert_eq!(
        s.refused("close", json!({ "id": "B1" })).await,
        "close needs a resolution: the sha the work landed as, or what closed it. No branch of audit/t-1 was found to read one from."
    );
}

#[tokio::test]
async fn test_drop_and_reopen() {
    let s = Scratch::new().await;
    s.open("B", "First").await;
    s.open("B", "Duplicate of the first").await;
    let out = s
        .ok("drop", json!({ "id": "B2", "superseded_by": "B1" }))
        .await;
    assert_eq!(out["item"]["state"], "dropped");
    assert_eq!(out["item"]["resolution"], "superseded by B1");
    assert_eq!(out["item"]["superseded_by"], "B1");
    s.ok(
        "reopen",
        json!({ "id": "B2", "why": "not a duplicate after all" }),
    )
    .await;
    assert_eq!(s.word("B2").await, "ready");
    assert_eq!(s.item("B2").await["superseded_by"], Value::Null);
    assert_eq!(
        s.refused("drop", json!({ "id": "B2" })).await,
        "drop needs a reason."
    );
}

#[tokio::test]
async fn test_a_wait_until_a_condition_depends_on_a_task_for_the_owner() {
    let s = Scratch::new().await;
    s.open("B", "Wide rename").await;
    s.ok("wait", json!({ "id": "B1", "until": "the fleet is quiet" }))
        .await;
    let task = s.item("T1").await;
    assert_eq!(task["title"], "the fleet is quiet");
    assert_eq!(task["turn"], "user");
    assert_eq!(task["related"], json!(["B1"]));
    assert_eq!(s.item("B1").await["wait_ref"], "T1");
    assert_eq!(
        s.refused("start", json!({ "id": "B1" })).await,
        format!(
            "B1 is waiting on item T1 since {}. resume it first.",
            s.item("B1").await["wait_since"].as_str().unwrap()
        )
    );
    s.ok("close", json!({ "id": "T1", "resolution": "it is quiet" }))
        .await;
    assert_eq!(s.word("B1").await, "ready");
    assert_eq!(
        s.refused("wait", json!({ "id": "B1" })).await,
        "wait takes exactly one of --on ID or --until \"condition\"."
    );
}

#[tokio::test]
async fn test_resume_clears_a_wait_and_the_dependencies_still_holding_it() {
    let s = Scratch::new().await;
    s.open("B", "Wide rename").await;
    s.open("B", "Narrow rename").await;
    s.ok("wait", json!({ "id": "B1", "on": "B2" })).await;
    s.ok("resume", json!({ "id": "B1" })).await;
    assert_eq!(s.word("B1").await, "ready");
    s.ok("close", json!({ "id": "B2", "resolution": "abc1234" }))
        .await;
    assert_eq!(s.word("B1").await, "ready");
    assert_eq!(
        s.events("B1").await.last().unwrap()["note"],
        "no longer depends on B2"
    );
}

#[tokio::test]
async fn test_an_item_on_two_dependencies_waits_until_both_are_satisfied() {
    let s = Scratch::new().await;
    s.open("T", "Glaze the bowls").await;
    s.open("T", "Fire the kiln").await;
    s.open("Q", "Which glaze").await;
    s.ok("wait", json!({ "id": "T1", "on": "T2" })).await;
    s.ok("dep", json!({ "id": "T1", "on": ["Q1"] })).await;
    s.ok("close", json!({ "id": "T2", "resolution": "abc1234" }))
        .await;
    assert_eq!(s.word("T1").await, "blocked");
    assert_eq!(s.item("T1").await["wait_ref"], "Q1");
    let out = s
        .ok("answer", json!({ "id": "Q1", "decision": "celadon" }))
        .await;
    assert_eq!(ids(&out["released"]), vec!["T1"]);
    assert_eq!(s.word("T1").await, "ready");
    assert_eq!(s.item("T1").await["turn"], "agent");
}

#[tokio::test]
async fn test_a_dependency_that_closes_a_cycle_is_refused_with_its_path() {
    let s = Scratch::new().await;
    for title in ["Throw", "Trim", "Fire"] {
        s.open("T", title).await;
    }
    s.ok("dep", json!({ "id": "T1", "on": ["T2"] })).await;
    s.ok("dep", json!({ "id": "T2", "on": ["T3"] })).await;
    assert_eq!(
        s.refused("dep", json!({ "id": "T3", "on": ["T1"] })).await,
        "T3 cannot depend on T1, which would close the cycle T1 -> T2 -> T3 -> T1."
    );
    assert_eq!(s.word("T3").await, "ready");
    assert_eq!(
        s.refused("dep", json!({ "id": "T3", "on": ["T3"] })).await,
        "T3 cannot depend on itself."
    );
}

#[tokio::test]
async fn test_dropping_a_dependency_asks_the_owner_and_closing_one_resumes_the_agent() {
    let s = Scratch::new().await;
    s.open("T", "Order clay").await;
    s.open("T", "Throw the mugs").await;
    s.open("T", "Order porcelain").await;
    s.open("T", "Throw the cups").await;
    s.ok("wait", json!({ "id": "T2", "on": "T1" })).await;
    s.ok("wait", json!({ "id": "T4", "on": "T3" })).await;
    let out = s
        .ok("drop", json!({ "id": "T1", "why": "the supplier closed" }))
        .await;
    assert_eq!(ids(&out["released"]), vec!["T2"]);
    let waiter = s.item("T2").await;
    assert_eq!(waiter["turn"], "user");
    assert_eq!(
        waiter["turn_note"],
        "T1, which this waited on, was dropped. Still wanted?"
    );
    assert_eq!(waiter["wait_on"], Value::Null);
    s.ok("close", json!({ "id": "T3", "resolution": "abc1234" }))
        .await;
    s.open("Q", "Which kiln").await;
    s.open("I", "Measure the kiln").await;
    s.ok("wait", json!({ "id": "Q1", "on": "I1" })).await;
    s.ok(
        "close",
        json!({ "id": "I1", "resolution": "it reaches cone 10" }),
    )
    .await;
    assert_eq!(s.item("Q1").await["turn"], "agent");
    let waiter = s.item("T4").await;
    assert_eq!(
        (waiter["turn"].as_str(), s.word("T4").await.as_str()),
        (Some("agent"), "ready")
    );
}

#[tokio::test]
async fn test_a_dependency_dropped_for_a_successor_holds_until_the_successor_closes() {
    let s = Scratch::new().await;
    s.open("T", "Old kiln wiring").await;
    s.open("T", "Fire the bisque").await;
    s.open("T", "New kiln wiring").await;
    s.ok("wait", json!({ "id": "T2", "on": "T1" })).await;
    s.ok("drop", json!({ "id": "T1", "superseded_by": "T3" }))
        .await;
    assert_eq!(s.word("T2").await, "blocked");
    assert_eq!(s.item("T2").await["wait_ref"], "T3");
    assert_eq!(s.item("T2").await["turn"], "agent");
    s.ok("close", json!({ "id": "T3", "resolution": "abc1234" }))
        .await;
    assert_eq!(s.word("T2").await, "ready");
}

#[tokio::test]
async fn test_removing_the_last_dependency_resumes_and_a_reopened_one_holds_again() {
    let s = Scratch::new().await;
    s.open("T", "Mix the slip").await;
    s.open("T", "Cast the jugs").await;
    s.ok("dep", json!({ "id": "T2", "on": ["T1"] })).await;
    assert_eq!(
        s.refused("dep", json!({ "id": "T1", "on": ["T2"], "remove": true }))
            .await,
        "T1 does not depend on T2."
    );
    s.ok("dep", json!({ "id": "T2", "on": ["T1"], "remove": true }))
        .await;
    assert_eq!(s.word("T2").await, "ready");
    s.ok("dep", json!({ "id": "T2", "on": ["T1"] })).await;
    s.ok("close", json!({ "id": "T1", "resolution": "abc1234" }))
        .await;
    assert_eq!(s.word("T2").await, "ready");
    s.ok("reopen", json!({ "id": "T1", "why": "the slip split" }))
        .await;
    assert_eq!(s.word("T2").await, "blocked");
    assert_eq!(
        s.refused("dep", json!({ "id": "T2", "on": ["T1"] })).await,
        "T2 already depends on T1."
    );
    s.open("T", "Glaze the jugs").await;
    assert_eq!(
        s.refused("dep", json!({ "id": "T3", "on": ["T1", "T1"] }))
            .await,
        "T3 already depends on T1."
    );
}

#[tokio::test]
async fn test_resuming_a_gated_plan_is_refused_while_a_member_is_open() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("B", "Port the proofer").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    assert_eq!(
        s.refused("resume", json!({ "id": "A1" })).await,
        format!("A1 waits until {GATE}, and B1 is open. Close it first, or pass --force.")
    );
    s.ok("resume", json!({ "id": "A1", "force": true })).await;
    assert_eq!(s.item("A1").await["wait_on"], Value::Null);
}

#[tokio::test]
async fn test_edit_fields_and_body() {
    let s = Scratch::new().await;
    s.open("B", "Thing").await;
    let out = s
        .ok(
            "edit",
            json!({ "id": "B1", "set": [
                { "field": "complexity", "value": "high" },
                { "field": "group", "value": "engine" }
            ] }),
        )
        .await;
    assert_eq!(out["item"]["complexity"], "high");
    assert_eq!(out["item"]["group"], "engine");
    let out = s
        .ok(
            "edit",
            json!({ "id": "B1", "body": "- **Evidence.** `src/x.rs:4`\n" }),
        )
        .await;
    assert_eq!(
        out["item"]["cites"],
        json!([{ "path": "src/x.rs", "line": 4, "kind": "cites_file" }])
    );
    assert_eq!(
        s.refused(
            "edit",
            json!({ "id": "B1", "set": [{ "field": "state", "value": "done" }] })
        )
        .await,
        "state is not editable; fields are title, complexity, theme, group, tags, turn_note. State and turn move with their own verbs. Also priority: docket priority, release: --release NAME."
    );
    let refused = s
        .refused(
            "edit",
            json!({ "id": "B1", "set": [{ "field": "priority", "value": "high" }] }),
        )
        .await;
    assert!(refused.contains("priority: docket priority"), "{refused}");
    assert!(refused.contains("release: --release NAME"), "{refused}");
    s.ok("edit", json!({ "id": "B1", "append": "a dated note" }))
        .await;
    assert!(
        s.item("B1").await["body"]
            .as_str()
            .unwrap()
            .contains("**Note, ")
    );
    assert_eq!(
        s.refused("edit", json!({ "id": "B1" })).await,
        "edit needs --set field=value, --release NAME, --append \"text\" or --body FILE|-."
    );
    assert_eq!(
        s.refused(
            "edit",
            json!({ "id": "B1", "set": [{ "field": "title", "value": " " }] })
        )
        .await,
        "a title cannot be empty"
    );
}

#[tokio::test]
async fn test_a_body_replace_against_a_newer_row_is_refused() {
    let s = Scratch::new().await;
    s.open("B", "Thing").await;
    let seen = "2000-01-01T00:00:00Z";
    s.ok("edit", json!({ "id": "B1", "append": "written meanwhile" }))
        .await;
    assert_eq!(
        s.refused(
            "edit",
            json!({ "id": "B1", "body": "stale copy", "expect_updated_at": seen })
        )
        .await,
        "B1 changed since it was read; reload it before replacing its body."
    );
    assert!(
        s.item("B1").await["body"]
            .as_str()
            .unwrap()
            .contains("written meanwhile")
    );
    let now = s.item("B1").await["updated_at"]
        .as_str()
        .unwrap()
        .to_string();
    s.ok(
        "edit",
        json!({ "id": "B1", "body": "fresh copy", "expect_updated_at": now }),
    )
    .await;
}

#[tokio::test]
async fn test_a_second_claim_on_another_branch_is_refused() {
    let s = Scratch::new().await;
    s.open("B", "contested").await;
    s.ok("start", json!({ "id": "B1", "branch": "audit/b1-0" }))
        .await;
    let since = s.item("B1").await["claim_since"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        s.refused("start", json!({ "id": "B1", "branch": "audit/b1-1" }))
            .await,
        format!(
            "B1 is held by audit/b1-0 on testbox since {since}. Pick another, or --force if that claim is abandoned."
        )
    );
    assert_eq!(
        s.refused("start", json!({ "id": "B1", "branch": "audit/b1-0" }))
            .await,
        format!("B1 is already yours, claimed {since}.")
    );
    s.ok(
        "start",
        json!({ "id": "B1", "branch": "audit/b1-2", "force": true }),
    )
    .await;
    assert_eq!(s.item("B1").await["claim_branch"], "audit/b1-2");
}

#[tokio::test]
async fn test_b539_wait_target_must_exist() {
    let s = Scratch::new().await;
    s.open("B", "x").await;
    assert_eq!(
        s.status("wait", json!({ "id": "B1", "on": "Q99" })).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        s.status("start", json!({ "id": "B9" })).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        s.status("start", json!({ "id": "B1", "project": "o/none" }))
            .await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn test_b878_illegal_transition() {
    let s = Scratch::new().await;
    s.open("B", "x").await;
    assert_eq!(
        s.refused(
            "reply",
            json!({ "id": "B1", "note": "nothing to reply to" })
        )
        .await,
        "B1 is already the agent's turn."
    );
    assert_eq!(
        s.refused("release", json!({ "id": "B1" })).await,
        "B1 is not claimed."
    );
    assert_eq!(
        s.refused("resume", json!({ "id": "B1" })).await,
        "B1 is not waiting."
    );
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    assert_eq!(
        s.refused("start", json!({ "id": "B1" })).await,
        "B1 is done (abc1234); start needs an open item. reopen it first if that is what you mean."
    );
    s.refused("ask", json!({ "id": "B1", "note": "too late" }))
        .await;
    assert_eq!(
        s.refused("reopen", json!({ "id": "B1", "why": "" })).await,
        "reopen needs a reason, it is recorded."
    );
}

#[tokio::test]
async fn test_start_records_the_runner_and_job() {
    let s = Scratch::new().await;
    s.open_with_body("B", "a bug", "- x\n").await;
    assert_eq!(
        s.refused("start", json!({ "id": "B1", "job": "pk2-s1" }))
            .await,
        "a job is named in its runner: docket start B1 --runner codex --job pk2-s1"
    );
    assert_eq!(
        s.status("start", json!({ "id": "B1", "runner": "cron" }))
            .await,
        StatusCode::BAD_REQUEST
    );
    let out = s
        .ok("start", json!({ "id": "B1", "runner": "codex", "job": "pk2-s1", "model": "gpt-5.4", "on": "devbox", "role": "build" }))
        .await;
    assert_eq!(out["item"]["claim_runner"], "codex");
    assert_eq!(out["item"]["claim_job"], "pk2-s1");
    assert_eq!(out["item"]["claim_on"], "devbox");
    let claimed = s
        .events("B1")
        .await
        .into_iter()
        .find(|e| e["kind"] == "claimed")
        .unwrap();
    assert_eq!(
        claimed["data"],
        json!({"job": "pk2-s1", "machine": "devbox", "model": "gpt-5.4", "role": "build", "runner": "codex"})
    );
    s.ok("release", json!({ "id": "B1" })).await;
    assert_eq!(s.item("B1").await["claim_on"], Value::Null);
}

#[tokio::test]
async fn test_every_verb_that_ends_a_claim_clears_its_runner_and_job() {
    let s = Scratch::new().await;
    s.open("Q", "which way").await;
    let verbs = [
        ("release", json!({})),
        ("wait", json!({ "on": "Q1" })),
        ("ask", json!({ "note": "run it on a device" })),
        ("close", json!({ "resolution": "abc1234" })),
        ("drop", json!({ "why": "no longer needed" })),
    ];
    for (n, (verb, extra)) in verbs.iter().enumerate() {
        let id = format!("B{}", n + 1);
        s.open_with_body("B", &format!("bug {}", n + 1), "- x\n")
            .await;
        s.ok(
            "start",
            json!({ "id": id, "runner": "claude", "job": format!("j{}", n + 1) }),
        )
        .await;
        let mut body = extra.clone();
        body["id"] = json!(id);
        let out = s.ok(verb, body).await;
        assert_eq!(out["item"]["claim_runner"], Value::Null, "{verb}");
        assert_eq!(out["item"]["claim_job"], Value::Null, "{verb}");
    }
}

// ---- test_audit.py ----

async fn plan(s: &Scratch) {
    s.open_with_body("A", "The pantry plan", "Every oven has one schedule.\n")
        .await;
}

#[tokio::test]
async fn test_audit_waits_until_everything_under_it_is_closed() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("B", "Port the proofer").await;
    s.open("Q", "Which scale does the mixer read").await;
    s.ok("parent", json!({ "a": ["B1", "Q1"], "plan": "A1" }))
        .await;
    assert_eq!(s.word("A1").await, "blocked");
    assert_eq!(s.item("A1").await["wait_ref"], GATE);
    s.refused("start", json!({ "id": "A1" })).await;
    s.ok(
        "answer",
        json!({ "id": "Q1", "decision": "the bench scale" }),
    )
    .await;
    s.open("B", "The mixer reads the bench scale").await;
    s.ok("parent", json!({ "a": ["B2"], "plan": "A1" })).await;
    s.ok("link", json!({ "a": ["B2"], "kind": "origin", "b": "Q1" }))
        .await;
    s.ok("close", json!({ "id": "Q1", "resolution": "opened B2" }))
        .await;
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    assert_eq!(s.word("A1").await, "blocked");
    let out = s
        .ok("drop", json!({ "id": "B2", "why": "not needed" }))
        .await;
    assert_eq!(ids(&out["released"]), vec!["A1"]);
    assert_eq!(s.word("A1").await, "ready");
    let waited = s
        .events("A1")
        .await
        .into_iter()
        .find(|e| e["kind"] == "waited")
        .unwrap();
    assert_eq!(waited["note"], format!("until: {GATE} (2 open)"));
    assert_eq!(waited["branch"], Value::Null);
}

#[tokio::test]
async fn test_a_reopened_item_holds_its_audit_again() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("B", "Port the proofer").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    assert_eq!(s.word("A1").await, "ready");
    s.ok(
        "reopen",
        json!({ "id": "B1", "why": "the port reads the wrong constant" }),
    )
    .await;
    assert_eq!(s.word("A1").await, "blocked");
}

#[tokio::test]
async fn test_a_plans_ticket_is_claimed_and_closed_on_its_own_branch() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("B", "oven runs hot twice").await;
    s.open("B", "bread oven runs hot twice").await;
    s.ok("parent", json!({ "a": ["B1", "B2"], "plan": "A1" }))
        .await;
    s.ok("start", json!({ "id": "B1", "branch": "audit/b1-1" }))
        .await;
    s.ok("start", json!({ "id": "B2", "branch": "audit/b2-1" }))
        .await;
    assert_eq!(s.word("A1").await, "blocked");
    assert_eq!(
        s.refused(
            "close",
            json!({ "id": "B1", "resolution": "abc1234", "branch": "audit/b2-1" })
        )
        .await,
        format!(
            "B1 is held by audit/b1-1 on testbox since {}. close would take it out from under that agent. Merge or unclaim the branch first, or pass --force.",
            s.item("B1").await["claim_since"].as_str().unwrap()
        )
    );
    s.ok(
        "close",
        json!({ "id": "B1", "resolution": "abc1234", "branch": "audit/b1-1" }),
    )
    .await;
    assert_eq!(s.item("B1").await["state"], "done");
}

#[tokio::test]
async fn test_an_audit_that_finds_work_goes_back_to_waiting() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.ok("start", json!({ "id": "A1" })).await;
    s.open("B", "The rye ratio is in no table").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    assert_eq!(s.word("A1").await, "building");
    assert_eq!(
        s.refused("close", json!({ "id": "A1", "resolution": "clean" }))
            .await,
        "A1 has work under it that is still open: B1. It closes only when nothing under it is open; unclaim it and it comes back when they close."
    );
    s.ok("release", json!({ "id": "A1" })).await;
    assert_eq!(s.word("A1").await, "blocked");
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    s.ok("start", json!({ "id": "A1" })).await;
    s.ok(
        "close",
        json!({ "id": "A1", "resolution": "clean: every oven has a stored schedule" }),
    )
    .await;
    assert_eq!(s.item("A1").await["state"], "done");
}

async fn next(s: &Scratch, role: &str) -> Vec<String> {
    let (status, out) = s
        .send(
            Method::GET,
            &format!("/next?project={SLUG}&role={role}"),
            "ownerkey",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "next {role}: {out}");
    ids(&out)
}

#[tokio::test]
async fn test_a_plans_audit_comes_due_once_and_closes_in_one_round() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("B", "Port the proofer").await;
    s.open("T", "Port the mixer").await;
    s.ok("parent", json!({ "a": ["B1", "T1"], "plan": "A1" }))
        .await;
    assert!(next(&s, "audit").await.is_empty());
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    assert!(next(&s, "audit").await.is_empty());
    s.ok("close", json!({ "id": "T1", "resolution": "def5678" }))
        .await;
    assert_eq!(next(&s, "audit").await, vec!["A1"]);
    assert!(!next(&s, "work").await.contains(&"A1".to_string()));
    assert!(!next(&s, "plan").await.contains(&"A1".to_string()));
    s.ok("start", json!({ "id": "A1" })).await;
    s.open("B", "The proofer reads the wrong constant").await;
    s.ok("parent", json!({ "a": ["B2"], "plan": "A1" })).await;
    s.ok(
        "close",
        json!({ "id": "A1", "resolution": "one round: gap B2" }),
    )
    .await;
    assert_eq!(s.item("A1").await["state"], "done");
    assert_eq!(next(&s, "work").await, vec!["B2"]);
    s.ok("close", json!({ "id": "B2", "resolution": "0a1b2c3" }))
        .await;
    assert_eq!(s.item("A1").await["state"], "done");
    assert!(next(&s, "audit").await.is_empty());
}

#[tokio::test]
async fn test_a_new_plan_is_the_plan_roles_until_it_opens_work() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("I", "How many ovens the bakery has").await;
    assert_eq!(next(&s, "plan").await, vec!["A1", "I1"]);
    assert!(next(&s, "audit").await.is_empty());
    s.open("B", "Port the proofer").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    assert_eq!(next(&s, "plan").await, vec!["I1"]);
    assert_eq!(next(&s, "work").await, vec!["B1"]);
}

#[tokio::test]
async fn test_packages_concepts_ideas_and_stories_are_read_only() {
    let s = Scratch::new().await;
    for key in ["PK", "CON", "CID", "STY"] {
        assert_eq!(
            s.refused("new", json!({ "key": key, "title": "one more" }))
                .await,
            format!(
                "{key} is kept to read: plans group the work now. File a plan with docket new A \"goal\"."
            )
        );
    }
    s.kept("PK", 1, "Shelves have one owner").await;
    assert_eq!(
        s.refused("start", json!({ "id": "PK1" })).await,
        "PK1 is a package and kept to read: plans group the work now, and its tickets are claimed one by one."
    );
    s.ok("drop", json!({ "id": "PK1", "why": "now a plan" }))
        .await;
}

#[tokio::test]
async fn test_an_empty_plan_is_ready_and_unlinking_the_last_open_item_releases_the_audit() {
    let s = Scratch::new().await;
    plan(&s).await;
    assert_eq!(s.word("A1").await, "ready");
    s.open("B", "Unrelated after all").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    assert_eq!(s.word("A1").await, "blocked");
    s.ok("parent", json!({ "a": ["B1"] })).await;
    assert_eq!(s.word("A1").await, "ready");
}

#[tokio::test]
async fn test_parent_takes_several_items_at_once() {
    let s = Scratch::new().await;
    plan(&s).await;
    for t in ["One", "Two", "Three"] {
        s.open("B", t).await;
    }
    let out = s
        .ok("parent", json!({ "a": ["B1", "B2", "B3"], "plan": "A1" }))
        .await;
    assert_eq!(out["items"], json!(["B1", "B2", "B3"]));
    assert_eq!(s.item("B3").await["parent"], "A1");
    assert_eq!(s.item("A1").await["children"], json!(["B1", "B2", "B3"]));
    assert_eq!(
        s.refused("parent", json!({ "a": ["A1"], "plan": "A1" }))
            .await,
        "A1 cannot be its own parent."
    );
}

#[tokio::test]
async fn test_key_adds_a_key_and_keeps_its_kind_once_it_holds_items() {
    let s = Scratch::new().await;
    let out = s
        .ok(
            "key",
            json!({ "key": "X", "kind": "research", "meaning": "explorations, measured by hand" }),
        )
        .await;
    assert_eq!(
        out,
        json!({ "key": "X", "kind": "research", "meaning": "explorations, measured by hand", "turn": "agent" })
    );
    s.open("X", "Measure it").await;
    assert_eq!(
        s.refused(
            "key",
            json!({ "key": "X", "kind": "work", "meaning": "now work" })
        )
        .await,
        "X is research and holds 1 items; changing its kind changes the rules they were filed under."
    );
    s.ok(
        "key",
        json!({ "key": "X", "kind": "research", "meaning": "explorations, renamed" }),
    )
    .await;
    assert_eq!(
        s.refused(
            "key",
            json!({ "key": "x1", "kind": "work", "meaning": "not a key" })
        )
        .await,
        "'x1' is not a key: one to three capitals."
    );
    let (_, projects) = s.send(Method::GET, "/projects", "ownerkey", None).await;
    let keys = projects[0]["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 10);
    assert_eq!(keys[9]["meaning"], "explorations, renamed");
}

// ---- keys, and the kinds kept to read ----

#[tokio::test]
async fn test_three_capital_keys_allocate_and_link() {
    let s = Scratch::new().await;
    s.ok(
        "key",
        json!({ "key": "FIX", "kind": "work", "meaning": "fixes" }),
    )
    .await;
    s.open("Q", "Which fixes").await;
    s.ok("answer", json!({ "id": "Q1", "decision": "these" }))
        .await;
    let out = s.open("FIX", "A baker corrects a step once").await;
    assert_eq!(out["item"]["num"], 1);
    s.ok(
        "link",
        json!({ "a": ["FIX1"], "kind": "origin", "b": "Q1" }),
    )
    .await;
    assert_eq!(s.item("FIX1").await["origin"], json!(["Q1"]));
    s.refused(
        "key",
        json!({ "key": "ABCD", "kind": "work", "meaning": "too long" }),
    )
    .await;
}

#[tokio::test]
async fn test_a_story_no_longer_waits_on_its_work() {
    let s = Scratch::new().await;
    s.kept("STY", 1, "A baker corrects a step once").await;
    s.open("B", "The shared step has no editor").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "STY1" })).await;
    assert_eq!(s.word("STY1").await, "ready");
    let out = s
        .ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    assert_eq!(out["released"], json!([]));
    assert_eq!(s.word("STY1").await, "ready");
}

#[tokio::test]
async fn test_standing_items_never_close_never_wait_and_are_nobodys_move() {
    let s = Scratch::new().await;
    s.kept("CON", 1, "The pantry").await;
    s.kept("CID", 1, "Nothing deletes").await;
    assert_eq!(s.word("CON1").await, "standing");
    assert_eq!(
        s.refused("close", json!({ "id": "CON1", "resolution": "done" }))
            .await,
        "CON1 is a concept and stays open for good. Retire it with docket drop CON1 \"why\" if it no longer holds."
    );
    assert_eq!(
        s.refused("wait", json!({ "id": "CID1", "until": "later" }))
            .await,
        "CID1 is a standing item and never waits."
    );
    s.ok("drop", json!({ "id": "CID1", "why": "no longer holds" }))
        .await;
}

// ---- test_flow.py ----

#[tokio::test]
async fn test_add_files_a_low_priority_ticket() {
    let s = Scratch::new().await;
    s.open("B", "the fix at hand").await;
    let out = s.ok("add", json!({ "title": "a thing someone saw" })).await;
    assert_eq!(out["item"]["id"], "B2");
    assert_eq!(out["item"]["word"], "ready");
    assert_eq!(out["item"]["priority"], "low");
    assert_eq!(next(&s, "work").await, vec!["B1", "B2"]);
    let out = s
        .ok(
            "add",
            json!({ "title": "seen by a job", "key": "T", "from": "b1-x" }),
        )
        .await;
    assert_eq!(out["item"]["id"], "T1");
    let opened = s
        .events("T1")
        .await
        .into_iter()
        .find(|e| e["kind"] == "opened")
        .unwrap();
    assert_eq!(opened["data"], json!({"observed_by": "b1-x"}));
}

#[tokio::test]
async fn test_two_tickets_on_one_file_both_start_and_name_each_other() {
    let s = Scratch::new().await;
    s.open_with_body("B", "one", "`src/store.rs:1`\n").await;
    s.open_with_body("B", "two", "`src/store.rs:9`\n").await;
    s.ok("start", json!({ "id": "B1", "branch": "audit/b1-1" }))
        .await;
    let out = s
        .ok("start", json!({ "id": "B2", "branch": "audit/b2-1" }))
        .await;
    assert_eq!(
        out["shares"],
        json!([{ "holder": "B1", "branch": "audit/b1-1", "host": "testbox", "paths": ["src/store.rs"] }])
    );
}

#[tokio::test]
async fn test_a_claim_records_the_host_a_job_runs_on_and_the_group_it_is_in() {
    let s = Scratch::new().await;
    s.ok(
        "new",
        json!({ "key": "B", "title": "one", "group": "engine" }),
    )
    .await;
    s.ok(
        "new",
        json!({ "key": "B", "title": "two", "group": "engine" }),
    )
    .await;
    let out = s
        .ok(
            "start",
            json!({ "id": "B1", "runner": "remote", "job": "b1-x", "on": "devbox" }),
        )
        .await;
    assert_eq!(out["item"]["claim_on"], "devbox");
    assert_eq!(out["group_others"], json!(["B2"]));
    s.ok("release", json!({ "id": "B1" })).await;
    assert_eq!(s.item("B1").await["claim_on"], Value::Null);
}

#[tokio::test]
async fn test_a_fleet_job_files_nothing() {
    let s = Scratch::new().await;
    for verb in ["new", "add"] {
        let (status, out) = s
            .post_as(
                "agentkey",
                verb,
                json!({ "key": "B", "title": "something I noticed" }),
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{verb}");
        assert!(
            out["error"].as_str().unwrap().contains("Observations"),
            "{verb}"
        );
    }
    s.open("B", "filed by the owner").await;
    let (status, _) = s.post_as("agentkey", "start", json!({ "id": "B1" })).await;
    assert_eq!(status, StatusCode::OK);
    let claimed = s
        .events("B1")
        .await
        .into_iter()
        .find(|e| e["kind"] == "claimed")
        .unwrap();
    assert_eq!(claimed["host"], "devbox");
}

#[tokio::test]
async fn test_a_refusal_writes_nothing() {
    let s = Scratch::new().await;
    s.open("B", "x").await;
    s.ok("start", json!({ "id": "B1", "branch": "audit/b1-1" }))
        .await;
    s.refused(
        "close",
        json!({ "id": "B1", "resolution": "abc", "gates": "fine", "branch": "audit/b1-1" }),
    )
    .await;
    assert_eq!(s.events("B1").await.len(), 2);
    assert_eq!(s.item("B1").await["state"], "open");
}

#[tokio::test]
async fn test_decide_on_a_question_is_refused_in_favour_of_answer() {
    let s = Scratch::new().await;
    s.open("Q", "One cache or two").await;
    let why = s
        .refused(
            "decide",
            json!({ "id": "Q1", "choice": "One.", "basis": "CID1" }),
        )
        .await;
    assert!(why.contains("docket answer"), "{why}");
    assert!(s.events("Q1").await.iter().all(|e| e["kind"] != "decided"));
}

#[tokio::test]
async fn test_decide_appends_the_choice_and_its_basis() {
    let s = Scratch::new().await;
    s.open_with_body("B", "x", "- **Fix.** do it.\n").await;
    let out = s
        .ok(
            "decide",
            json!({ "id": "B1", "choice": "Keep one owner.", "basis": "CID1" }),
        )
        .await;
    let body = out["item"]["body"].as_str().unwrap();
    assert!(body.starts_with("- **Fix.** do it.\n\n**Decided, "));
    assert!(body.ends_with(".** Keep one owner. Basis: CID1"));
    let decided = s
        .events("B1")
        .await
        .into_iter()
        .find(|e| e["kind"] == "decided")
        .unwrap();
    assert_eq!(decided["note"], "Keep one owner.");
    assert_eq!(decided["data"], json!({"derived": "CID1"}));
}

async fn two_releases() -> Scratch {
    let s = Scratch::new().await;
    for name in ["1.0", "1.1"] {
        s.ok("releases", json!({ "action": "add", "name": name }))
            .await;
    }
    s
}

async fn filed(s: &Scratch, key: &str, release: &str) {
    s.ok(
        "new",
        json!({ "key": key, "title": "Filed", "release": release }),
    )
    .await;
}

#[tokio::test]
async fn test_a_wait_on_an_item_in_a_later_release_is_refused_unless_forced() {
    let s = two_releases().await;
    filed(&s, "T", "1.0").await;
    filed(&s, "T", "1.1").await;
    filed(&s, "T", "1.0").await;
    assert_eq!(
        s.refused("wait", json!({ "id": "T1", "on": "T2" })).await,
        "T1 (1.0) would be held by T2 (1.1), which ships later. Move T2 to 1.0, or move T1 to 1.1, or pass --force."
    );
    s.ok("wait", json!({ "id": "T2", "on": "T1" })).await;
    s.ok("wait", json!({ "id": "T3", "on": "T1" })).await;
    s.ok("resume", json!({ "id": "T3" })).await;
    s.ok("wait", json!({ "id": "T3", "on": "T2", "force": true }))
        .await;
    assert_eq!(s.item("T3").await["wait_item"], s.item("T2").await["rid"]);
}

#[tokio::test]
async fn test_a_plan_is_not_linked_to_a_member_in_a_later_release_unless_forced() {
    let s = two_releases().await;
    filed(&s, "A", "1.0").await;
    filed(&s, "T", "1.1").await;
    assert_eq!(
        s.refused("parent", json!({ "a": ["T1"], "plan": "A1" }))
            .await,
        "A1 (1.0) would be held by T1 (1.1), which ships later. Move T1 to 1.0, or move A1 to 1.1, or pass --force."
    );
    s.ok(
        "parent",
        json!({ "a": ["T1"], "plan": "A1", "force": true }),
    )
    .await;
}

#[tokio::test]
async fn test_a_release_edit_that_puts_a_hold_into_a_later_release_is_refused() {
    let s = two_releases().await;
    filed(&s, "T", "1.0").await;
    filed(&s, "T", "1.0").await;
    s.ok("wait", json!({ "id": "T1", "on": "T2" })).await;
    assert_eq!(
        s.refused("edit", json!({ "id": "T2", "release": "1.1" }))
            .await,
        "T1 (1.0) would be held by T2 (1.1), which ships later. Move T2 to 1.0, or move T1 to 1.1, or pass --force."
    );
    assert_eq!(
        s.refused("edit", json!({ "id": "T2", "release": "" }))
            .await,
        "T1 (1.0) would be held by T2 (the backlog), which ships later. Move T2 to 1.0, or move T1 to the backlog, or pass --force."
    );
    s.ok("edit", json!({ "id": "T1", "release": "1.1" })).await;
    s.ok("edit", json!({ "id": "T2", "release": "1.1" })).await;
}

#[tokio::test]
async fn test_putting_a_waiter_under_the_plan_it_waits_on_is_refused_with_the_path() {
    let s = Scratch::new().await;
    s.open("B", "Till freezes at opening").await;
    s.open("A", "Shelf plan").await;
    s.ok("start", json!({ "id": "B1" })).await;
    s.ok("wait", json!({ "id": "B1", "on": "A1" })).await;
    let said = s
        .refused("parent", json!({ "a": ["B1"], "plan": "A1" }))
        .await;
    assert!(said.contains("the cycle A1 -> B1 -> A1"), "{said}");
    assert_eq!(s.item("B1").await["parent"], Value::Null);
}

#[tokio::test]
async fn test_a_task_is_refused_as_a_parent() {
    let s = Scratch::new().await;
    s.open("T", "Stack the crates").await;
    s.open("B", "A crate splits").await;
    assert_eq!(
        s.refused("parent", json!({ "a": ["B1"], "plan": "T1" }))
            .await,
        "T1 is a work, and only a plan holds children. Record what spawned an item with docket link ID origin T1."
    );
    assert_eq!(s.item("B1").await["parent"], Value::Null);
}

#[tokio::test]
async fn test_a_plan_under_its_own_child_is_refused_with_the_path() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("A", "The cellar plan").await;
    s.open("A", "The rack plan").await;
    s.ok("parent", json!({ "a": ["A2"], "plan": "A1" })).await;
    s.ok("parent", json!({ "a": ["A3"], "plan": "A2" })).await;
    assert_eq!(
        s.refused("parent", json!({ "a": ["A1"], "plan": "A3" }))
            .await,
        "A3 cannot be the parent of A1: that closes the cycle A3 -> A1 -> A2 -> A3, and a plan finishes only after its children. Move A3 out from under A1 first."
    );
    assert_eq!(s.item("A1").await["parent"], Value::Null);
}

#[tokio::test]
async fn test_an_open_item_is_refused_under_a_closed_plan() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.ok("start", json!({ "id": "A1" })).await;
    s.ok("close", json!({ "id": "A1", "resolution": "audited" }))
        .await;
    s.open("B", "A gap the audit found").await;
    let said = s
        .refused("parent", json!({ "a": ["B1"], "plan": "A1" }))
        .await;
    assert!(
        said.contains("a closed plan holds no open children"),
        "{said}"
    );
    s.ok("link", json!({ "a": ["B1"], "kind": "origin", "b": "A1" }))
        .await;
    assert_eq!(s.item("B1").await["origin"], json!(["A1"]));
}

#[tokio::test]
async fn test_closing_a_decided_question_that_says_it_opened_an_item_adds_no_link() {
    let s = Scratch::new().await;
    s.open("Q", "Which flour for the rye").await;
    s.open("T", "Order the dark rye flour").await;
    s.ok("answer", json!({ "id": "Q1", "decision": "dark rye" }))
        .await;
    s.ok("close", json!({ "id": "Q1", "resolution": "opened T1" }))
        .await;
    let (_, links) = s.send(Method::GET, "/links", "ownerkey", None).await;
    assert_eq!(links, json!([]));
    assert_eq!(s.item("T1").await["parent"], Value::Null);
}

#[tokio::test]
async fn test_link_takes_origin_and_refuses_opened_naming_parent() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("B", "The proofer drifts").await;
    let said = s
        .refused("link", json!({ "a": ["B1"], "kind": "opened", "b": "A1" }))
        .await;
    assert_eq!(
        said,
        "link kinds are related and origin; waits are set with docket wait. A plan's children are set with docket parent ID A1."
    );
    s.ok("link", json!({ "a": ["B1"], "kind": "origin", "b": "A1" }))
        .await;
    assert_eq!(s.item("B1").await["origin"], json!(["A1"]));
    assert_eq!(s.word("A1").await, "ready");
}

#[tokio::test]
async fn test_an_unclaim_stores_its_outcome_and_who_gave_it_back() {
    let s = Scratch::new().await;
    s.open("B", "Kettle trips the fuse").await;
    let (status, _) = s
        .post_as(
            "agentkey",
            "start",
            json!({ "id": "B1", "runner": "claude", "job": "b1-j", "on": "devbox", "model": "m-small", "effort": "low", "role": "build" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let open = s.assignments("B1").await;
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0]["ended_at"], Value::Null);
    assert_eq!(
        (&open[0]["kind"], &open[0]["assignee"], &open[0]["actor"]),
        (&json!("claim"), &json!("agent"), &json!("agent"))
    );
    assert_eq!(
        (&open[0]["job"], &open[0]["machine"], &open[0]["effort"]),
        (&json!("b1-j"), &json!("devbox"), &json!("low"))
    );
    assert_eq!(
        s.status("release", json!({ "id": "B1", "outcome": "sideways" }))
            .await,
        StatusCode::BAD_REQUEST
    );
    s.ok(
        "release",
        json!({ "id": "B1", "outcome": "conflict", "note": "the branch did not merge" }),
    )
    .await;
    let rows = s.assignments("B1").await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["outcome"], "conflict");
    assert_eq!(rows[0]["actor"], "owner");
    assert_eq!(rows[0]["note"], "the branch did not merge");
    assert_ne!(rows[0]["ended_at"], Value::Null);
}

#[tokio::test]
async fn test_a_job_report_stores_its_tokens_and_cost_on_the_open_attempt() {
    let s = Scratch::new().await;
    s.open("B", "Kettle trips the fuse").await;
    s.ok(
        "start",
        json!({ "id": "B1", "runner": "claude", "job": "b1-j" }),
    )
    .await;
    s.ok(
        "job-report",
        json!({ "id": "B1", "start": "2026-10-05T10:00:00Z", "end": "2026-10-05T10:20:00Z",
                "exit": 0, "tokens_in": 120, "tokens_out": 45, "cost_reported": 0.5 }),
    )
    .await;
    let rows = s.assignments("B1").await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["ended_at"], Value::Null);
    assert_eq!(
        (
            &rows[0]["tokens_in"],
            &rows[0]["tokens_out"],
            &rows[0]["cost_reported"]
        ),
        (&json!(120), &json!(45), &json!(0.5))
    );
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    assert_eq!(s.assignments("B1").await[0]["tokens_out"], json!(45));
}

#[tokio::test]
async fn test_a_holder_that_waits_ends_its_attempt_blocked_and_a_close_lands_the_next() {
    let s = Scratch::new().await;
    s.open("B", "Kettle trips the fuse").await;
    s.open("Q", "Which fuse rating").await;
    s.ok("start", json!({ "id": "B1" })).await;
    s.ok("wait", json!({ "id": "B1", "on": "Q1" })).await;
    let rows = s.assignments("B1").await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["outcome"], "blocked");
    s.ok("answer", json!({ "id": "Q1", "decision": "thirteen amps" }))
        .await;
    s.ok("start", json!({ "id": "B1" })).await;
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    let outcomes: Vec<Value> = s
        .assignments("B1")
        .await
        .iter()
        .map(|a| a["outcome"].clone())
        .collect();
    assert_eq!(outcomes, [json!("blocked"), json!("landed")]);
}

#[tokio::test]
async fn test_an_ask_assigns_the_owner_until_the_reply() {
    let s = Scratch::new().await;
    s.open("B", "Kettle trips the fuse").await;
    s.ok("start", json!({ "id": "B1" })).await;
    s.ok(
        "ask",
        json!({ "id": "B1", "note": "plug it in at the bench" }),
    )
    .await;
    let rows = s.assignments("B1").await;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0]["outcome"], "blocked");
    assert_eq!(
        (&rows[1]["kind"], &rows[1]["assignee"], &rows[1]["note"]),
        (
            &json!("ask"),
            &json!("owner"),
            &json!("plug it in at the bench")
        )
    );
    assert_eq!(rows[1]["ended_at"], Value::Null);
    s.ok("reply", json!({ "id": "B1", "note": "it trips at once" }))
        .await;
    let rows = s.assignments("B1").await;
    assert_ne!(rows[1]["ended_at"], Value::Null);
    assert_eq!(rows[1]["outcome"], Value::Null);
}

async fn asked(s: &Scratch, key: &str, title: &str, extra: Value) -> String {
    let id = s.open(key, title).await["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    s.ok("start", json!({ "id": id })).await;
    let mut body = json!({ "id": id, "note": "needs a hand" });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    s.ok("ask", body).await;
    id
}

#[tokio::test]
async fn test_an_ask_past_the_owner_limit_is_refused_naming_the_limit() {
    let s = Scratch::new().await;
    s.db.seed(&format!(
        "UPDATE projects SET skills = '{{\"owner_limit\": \"2\"}}' WHERE slug = '{SLUG}'"
    ))
    .await;
    asked(&s, "B", "Kettle trips the fuse", json!({})).await;
    asked(&s, "B", "Toaster hums", json!({})).await;
    let third = s.open("B", "Fan rattles").await["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    s.ok("start", json!({ "id": third })).await;
    let why = s
        .refused("ask", json!({ "id": third, "note": "needs a hand" }))
        .await;
    assert!(why.contains("owner_limit"), "{why}");
    let why = s
        .refused("new", json!({ "key": "Q", "title": "Which fan" }))
        .await;
    assert!(why.contains("owner_limit"), "{why}");
}

#[tokio::test]
async fn test_an_ask_keeps_its_need_and_the_owner_queue_groups_by_it() {
    let s = Scratch::new().await;
    let act = asked(&s, "B", "Run the migration", json!({ "need": "act" })).await;
    let hold = asked(&s, "B", "Plug in the meter", json!({ "need": "hold" })).await;
    let rows = s.assignments(&hold).await;
    assert_eq!(rows.last().unwrap()["need"], "hold");
    let (_, todo) = s
        .send(
            Method::GET,
            &format!("/todo?project={SLUG}"),
            "ownerkey",
            None,
        )
        .await;
    let listed: Vec<(&str, &str)> = todo
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap(),
                r["owner_group"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(listed, [(hold.as_str(), "hold"), (act.as_str(), "act")]);
    let why = s
        .refused("ask", json!({ "id": hold, "note": "x", "need": "sing" }))
        .await;
    assert!(why.contains("hold"), "{why}");
}

#[tokio::test]
async fn test_a_derived_answer_lists_first_in_the_owner_queue_until_it_is_confirmed() {
    let s = Scratch::new().await;
    s.open("Q", "One cache or two").await;
    s.open("Q", "Which colour for the alert").await;
    s.ok(
        "answer",
        json!({ "id": "Q2", "decision": "Two", "derived": "CID3, one owner per cache" }),
    )
    .await;
    let todo = async || {
        let (_, rows) = s
            .send(
                Method::GET,
                &format!("/todo?project={SLUG}"),
                "ownerkey",
                None,
            )
            .await;
        ids(&rows)
    };
    assert_eq!(todo().await, ["Q2", "Q1"]);
    s.ok("answer", json!({ "id": "Q2", "decision": "Two" }))
        .await;
    assert_eq!(todo().await, ["Q1"]);
}
