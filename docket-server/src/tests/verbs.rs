use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::app;
use crate::auth::Keys;

use docket_migration::scratch::Scratch as Database;
const SLUG: &str = "test/proj";

/// A scratch database and project, every call made as the owner on one branch unless told otherwise.
struct Scratch {
    app: Router,
    db: Database,
    /// The area `new` and `add` file in when a test names neither an area nor a plan.
    filing_area: Option<&'static str>,
}

impl Scratch {
    /// A project with one area, which an item filed without naming any lands in.
    async fn new() -> Self {
        let mut s = Self::bare().await;
        s.ok("areas", json!({ "action": "add", "name": "general" }))
            .await;
        s.filing_area = Some("general");
        s
    }

    /// A project with no area, and nothing filed for a test that names none.
    async fn bare() -> Self {
        let db = Database::new(2).await;
        db.seed(&format!(
            "INSERT INTO projects (slug, created_at, updated_at) VALUES ('{SLUG}', 'c', 'u')"
        ))
        .await;
        let keys = Keys::parse("testbox owner ownerkey\ndevbox agent agentkey").unwrap();
        Self {
            app: app(&db.db, keys),
            db,
            filing_area: None,
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
        if let Some(area) = self.filing_area
            && matches!(verb, "new" | "add")
            && !fields.contains_key("parent")
        {
            fields.entry("area").or_insert(json!(area));
        }
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

    /// An item of a kind kept to read, written as the move onto plans left it.
    async fn kept(&self, key: &str, num: i64, title: &str) {
        self.db
            .seed(&format!(
                "INSERT INTO items (project, key, num, title, state, body, opened_at, updated_at) \
                 VALUES ('{SLUG}', '{key}', {num}, '{title}', 'open', '', 'o', 'u')"
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

// ---- the core pathways ----

#[tokio::test]
async fn test_question_pathway() {
    let s = Scratch::new().await;
    s.open("B", "Till freezes at opening").await;
    s.open("Q", "One cache or two").await;
    assert_eq!(s.word("B1").await, "ready");
    assert_eq!(s.word("Q1").await, "waiting on owner");
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
async fn test_an_agent_key_answers_only_with_a_basis() {
    let s = Scratch::new().await;
    s.open("Q", "One cache or two").await;
    let (status, out) = s
        .post_as(
            "agentkey",
            "answer",
            json!({ "id": "Q1", "decision": "Two" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{out}");
    assert!(out.to_string().contains("--derived"), "{out}");
    assert_eq!(s.item("Q1").await["decision"], Value::Null);
    let (status, out) = s
        .post_as(
            "agentkey",
            "answer",
            json!({ "id": "Q1", "decision": "Two", "derived": "one owner per cache" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{out}");
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
        json!({ "key": "T", "title": "Wider settings page", "release": "1.1.0", "theme": "upkeep" }),
    )
    .await;
    s.ok("add", json!({ "title": "Hook runs twice" })).await;
    assert_eq!(s.item("B1").await["release"], "1.0.0");
    assert_eq!(s.item("T1").await["release"], "1.1.0");
    assert_eq!(s.item("T1").await["labels"], json!(["upkeep"]));
    assert_eq!(s.item("B2").await["release"], Value::Null);
    assert_eq!(
        s.refused(
            "add",
            json!({ "title": "Lint the hooks", "release": "upkeep" })
        )
        .await,
        "upkeep is not a release here: give current or one of 1.0.0 1.1.0"
    );
}

#[tokio::test]
async fn test_edit_to_a_release_the_project_does_not_have_is_refused() {
    let s = two_releases().await;
    filed(&s, "T", "1.0.0").await;
    assert_eq!(
        s.refused("edit", json!({ "id": "T1", "release": "0.42.0" }))
            .await,
        "0.42.0 is not a release here: give current or one of 1.0.0 1.1.0"
    );
    s.ok("edit", json!({ "id": "T1", "release": "1.1.0" }))
        .await;
    assert_eq!(s.item("T1").await["release"], "1.1.0");
    s.ok("edit", json!({ "id": "T1", "release": "" })).await;
    assert_eq!(s.item("T1").await["release"], Value::Null);
}

#[tokio::test]
async fn test_a_release_ships_only_once_its_open_items_are_closed_or_moved_on() {
    let s = two_releases().await;
    filed(&s, "T", "1.0.0").await;
    filed(&s, "T", "1.0.0").await;
    s.ok("close", json!({ "id": "T2", "resolution": "fixed" }))
        .await;
    assert_eq!(
        s.refused("releases", json!({ "action": "ship", "name": "1.0.0" }))
            .await,
        "1.0.0 still holds 1 open: T1. Close them, or pass --move-open-to a later release."
    );
    let out = s
        .ok(
            "releases",
            json!({ "action": "ship", "name": "1.0.0", "to": "1.1.0" }),
        )
        .await;
    assert_eq!(out["moved"], json!(["T1"]));
    assert_eq!(s.item("T1").await["release"], "1.1.0");
    assert!(out["releases"][0]["shipped_at"].is_string());
    assert_eq!(
        s.refused("edit", json!({ "id": "T1", "release": "1.0.0" }))
            .await,
        "1.0.0 has shipped: give current or one of 1.1.0"
    );
    filed(&s, "T", "current").await;
    assert_eq!(s.item("T3").await["release"], "1.1.0");
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
    assert_eq!(names, ["1.0.0", "1.0.5", "1.1.0"]);
    assert_eq!(
        s.refused("releases", json!({ "action": "add", "name": "1.1.0" }))
            .await,
        "1.1.0 is already a release"
    );
    assert_eq!(
        s.refused("releases", json!({ "action": "add", "name": "1.1" }))
            .await,
        "1.1 is not a semantic version: give MAJOR.MINOR.PATCH, as 1.1.0"
    );
    let (status, _) = s
        .post_as(
            "agentkey",
            "releases",
            json!({ "action": "add", "name": "2.0.0" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    filed(&s, "T", "1.0.0").await;
    let out = s
        .ok(
            "releases",
            json!({ "action": "move", "name": "1.0.0", "to": "1.1.0" }),
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
async fn test_a_new_item_takes_its_type_from_its_key_and_its_priority_into_the_column() {
    let s = Scratch::new().await;
    let out = s
        .ok(
            "new",
            json!({ "key": "B", "title": "Till freezes at opening", "priority": "critical" }),
        )
        .await;
    assert_eq!(out["item"]["type"], "bug");
    assert_eq!(out["item"]["priority"], "critical");
    assert_eq!(out["item"]["labels"], json!([]));
    assert_eq!(s.open("A", "Open the till").await["item"]["type"], "plan");
}

#[tokio::test]
async fn test_setting_tags_leaves_the_priority_column_alone() {
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
    assert_eq!(b1["labels"], json!(["single"]));
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
    assert_eq!(s.word("Q1").await, "waiting on owner");
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
    assert_eq!(out["item"]["word"], "waiting on owner");
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
            "B1 is waiting on T1, a task on the owner's turn, since {}. resume it first.",
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
async fn test_a_plan_with_open_members_is_not_waiting_so_resume_is_refused() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("B", "Port the proofer").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    assert_eq!(
        s.refused("resume", json!({ "id": "A1" })).await,
        "A1 is not waiting."
    );
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
        "state is not editable; fields are title, complexity, theme, group, repo, tags, turn_note. State and turn move with their own verbs. Also priority: docket priority, release: --release NAME, area: --area NAME."
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
        "edit needs --set field=value, --release NAME, --area NAME, --append \"text\" or --body FILE|-."
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
    s.ok("release", json!({ "id": "B1", "outcome": "ended" }))
        .await;
    assert_eq!(s.item("B1").await["claim_on"], Value::Null);
}

#[tokio::test]
async fn test_every_verb_that_ends_a_claim_clears_its_runner_and_job() {
    let s = Scratch::new().await;
    s.open("Q", "which way").await;
    let verbs = [
        ("release", json!({ "outcome": "ended" })),
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

// ---- audit ----

async fn plan(s: &Scratch) {
    s.open_with_body("A", "The pantry plan", "Every oven has one schedule.\n")
        .await;
}

#[tokio::test]
async fn test_a_plan_stores_no_wait_and_closes_once_everything_under_it_is_closed() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("B", "Port the proofer").await;
    s.open("Q", "Which scale does the mixer read").await;
    s.ok("parent", json!({ "a": ["B1", "Q1"], "plan": "A1" }))
        .await;
    assert_eq!(s.word("A1").await, "under way");
    assert_eq!(s.item("A1").await["wait_on"], Value::Null);
    assert_eq!(s.item("A1").await["wait_ref"], Value::Null);
    assert_eq!(
        s.refused("close", json!({ "id": "A1", "resolution": "clean" }))
            .await,
        "A1 has work under it that is still open: B1, Q1. It closes only when nothing under it is open; unclaim it and it comes back when they close."
    );
    assert_eq!(
        s.refused("start", json!({ "id": "A1" })).await,
        "A1 has work under it that is still open: B1, Q1. It is audited once nothing under it is open."
    );
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
    assert_eq!(s.word("A1").await, "under way");
    let out = s
        .ok("drop", json!({ "id": "B2", "why": "not needed" }))
        .await;
    assert_eq!(out["released"], json!([]));
    assert_eq!(s.word("A1").await, "audit due");
    assert_eq!(s.item("A1").await["wait_on"], Value::Null);
    let kinds: Vec<Value> = s
        .events("A1")
        .await
        .into_iter()
        .map(|e| e["kind"].clone())
        .collect();
    assert!(!kinds.contains(&json!("waited")), "{kinds:?}");
    assert!(!kinds.contains(&json!("resumed")), "{kinds:?}");
    s.ok("start", json!({ "id": "A1" })).await;
}

#[tokio::test]
async fn test_a_reopened_item_holds_its_audit_again() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("B", "Port the proofer").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    assert_eq!(s.word("A1").await, "audit due");
    s.ok(
        "reopen",
        json!({ "id": "B1", "why": "the port reads the wrong constant" }),
    )
    .await;
    assert_eq!(s.word("A1").await, "under way");
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
    assert_eq!(s.word("A1").await, "under way");
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
async fn test_a_held_plan_closes_with_work_open_under_it_and_an_unheld_one_does_not() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.ok("start", json!({ "id": "A1" })).await;
    s.open("B", "The rye ratio is in no table").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    assert_eq!(s.word("A1").await, "in progress");
    s.ok("release", json!({ "id": "A1", "outcome": "ended" }))
        .await;
    assert_eq!(s.word("A1").await, "under way");
    assert_eq!(
        s.refused("close", json!({ "id": "A1", "resolution": "clean" }))
            .await,
        "A1 has work under it that is still open: B1. It closes only when nothing under it is open; unclaim it and it comes back when they close."
    );
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
    assert_eq!(next(&s, "audit").await.len(), 0);
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    assert_eq!(next(&s, "audit").await.len(), 0);
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
    assert_eq!(next(&s, "audit").await.len(), 0);
}

#[tokio::test]
async fn test_a_new_plan_is_the_plan_roles_until_it_opens_work() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("I", "How many ovens the bakery has").await;
    assert_eq!(next(&s, "plan").await, vec!["A1", "I1"]);
    assert_eq!(next(&s, "audit").await.len(), 0);
    s.open("B", "Port the proofer").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    assert_eq!(next(&s, "plan").await, vec!["I1"]);
    assert_eq!(next(&s, "work").await, vec!["B1"]);
}

#[tokio::test]
async fn test_new_refuses_a_key_outside_the_fixed_five() {
    let s = Scratch::new().await;
    for key in ["PK", "CON", "CID", "STY"] {
        assert_eq!(
            s.refused("new", json!({ "key": key, "title": "one more" }))
                .await,
            format!("{key} is not one of docket's keys. File under T, B, Q, I, A.")
        );
    }
}

#[tokio::test]
async fn test_an_empty_plan_is_ready_and_unlinking_the_last_open_item_makes_it_ready_again() {
    let s = Scratch::new().await;
    plan(&s).await;
    assert_eq!(s.word("A1").await, "ready");
    s.open("B", "Unrelated after all").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    assert_eq!(s.word("A1").await, "under way");
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
async fn test_new_refuses_any_key_but_the_five_and_a_stored_key_still_shows_and_closes() {
    let s = Scratch::new().await;
    assert_eq!(
        s.refused("new", json!({ "key": "ZQ", "title": "One more" }))
            .await,
        "ZQ is not one of docket's keys. File under T, B, Q, I, A."
    );
    s.kept("ZQ", 1, "Measure it").await;
    assert_eq!(s.item("ZQ1").await["id"], "ZQ1");
    s.ok("close", json!({ "id": "ZQ1", "resolution": "abc1234" }))
        .await;
    assert_eq!(s.word("ZQ1").await, "done");
    let (status, _) = s
        .post(
            "key",
            json!({ "key": "X", "kind": "research", "meaning": "explorations" }),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---- keys, and the kinds kept to read ----

#[tokio::test]
async fn test_three_capital_keys_keep_their_items_and_link() {
    let s = Scratch::new().await;
    s.open("Q", "Which fixes").await;
    s.ok("answer", json!({ "id": "Q1", "decision": "these" }))
        .await;
    assert_eq!(
        s.refused(
            "new",
            json!({ "key": "FIX", "title": "A baker corrects a step once" })
        )
        .await,
        "FIX is not one of docket's keys. File under T, B, Q, I, A."
    );
    s.kept("FIX", 1, "A baker corrects a step once").await;
    s.ok(
        "link",
        json!({ "a": ["FIX1"], "kind": "origin", "b": "Q1" }),
    )
    .await;
    assert_eq!(s.item("FIX1").await["origin"], json!(["Q1"]));
}

#[tokio::test]
async fn test_concept_and_idea_items_read_and_close_like_any_other_item() {
    let s = Scratch::new().await;
    s.kept("CON", 1, "The pantry").await;
    s.kept("CID", 1, "Nothing deletes").await;
    assert_eq!(s.word("CON1").await, "ready");
    s.ok("close", json!({ "id": "CON1", "resolution": "done" }))
        .await;
    assert_eq!(s.word("CON1").await, "done");
    s.ok("wait", json!({ "id": "CID1", "until": "later" }))
        .await;
}

// ---- flow ----

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
    s.ok("release", json!({ "id": "B1", "outcome": "ended" }))
        .await;
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

/// A job's key, posting on the branch its claim was made on.
async fn job_files(s: &Scratch, verb: &str, branch: &str, body: Value) -> (StatusCode, Value) {
    let mut body = body;
    body["branch"] = json!(branch);
    s.post_as("agentkey", verb, body).await
}

#[tokio::test]
async fn test_a_job_files_its_findings_from_the_research_it_holds() {
    let s = Scratch::new().await;
    s.open("I", "Measure the store against the fetch").await;
    s.ok("start", json!({ "id": "I1", "branch": "lead/i1-1" }))
        .await;
    for (verb, key) in [("new", "B"), ("add", "T")] {
        let (status, out) = job_files(
            &s,
            verb,
            "lead/i1-1",
            json!({ "key": key, "title": "Bound the results waiting to be stored" }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{verb}: {out}");
    }
    assert_eq!(s.item("B1").await["origin"], json!(["I1"]));
    assert_eq!(s.item("T1").await["origin"], json!(["I1"]));
}

#[tokio::test]
async fn test_a_job_files_under_the_plan_and_from_the_question_it_holds() {
    let s = Scratch::new().await;
    s.open("A", "Hill strength bests").await;
    s.ok("start", json!({ "id": "A1", "branch": "lead/a1-1" }))
        .await;
    let (status, out) = job_files(
        &s,
        "new",
        "lead/a1-1",
        json!({ "key": "B", "title": "Rank climbs by vertical power", "parent": "A1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(s.item("B1").await["parent"], "A1");
    assert_eq!(s.item("B1").await["origin"], json!(["A1"]));

    s.open("Q", "One cache or two").await;
    s.ok("answer", json!({ "id": "Q1", "decision": "Two" }))
        .await;
    s.ok("start", json!({ "id": "Q1", "branch": "lead/q1-1" }))
        .await;
    let (status, out) = job_files(
        &s,
        "new",
        "lead/q1-1",
        json!({ "key": "T", "title": "Split the tile cache from the route cache" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(s.item("T1").await["origin"], json!(["Q1"]));
}

#[tokio::test]
async fn test_a_job_building_a_ticket_still_files_nothing() {
    let s = Scratch::new().await;
    s.open("B", "Fix the timeout").await;
    s.ok("start", json!({ "id": "B1", "branch": "lead/b1-1" }))
        .await;
    s.open("I", "Measure the store").await;
    s.ok("start", json!({ "id": "I1", "branch": "lead/i1-1" }))
        .await;
    for branch in ["lead/b1-1", "lead/none-1"] {
        let (status, out) = job_files(
            &s,
            "new",
            branch,
            json!({ "key": "B", "title": "something I noticed" }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{branch}: {out}");
        assert!(
            out["error"].as_str().unwrap().contains("Observations"),
            "{branch}: {out}"
        );
    }
    s.ok(
        "release",
        json!({ "id": "I1", "branch": "lead/i1-1", "outcome": "ended" }),
    )
    .await;
    let (status, _) = job_files(
        &s,
        "new",
        "lead/i1-1",
        json!({ "key": "B", "title": "filed after the claim ended" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = s
        .send(
            Method::GET,
            &format!("/show/B2?project={SLUG}"),
            "ownerkey",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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

#[tokio::test]
async fn test_decide_with_an_area_is_refused_on_a_closed_item_unsorted_or_a_question() {
    let s = Scratch::new().await;
    s.open("B", "Mend the kite").await;
    s.open("Q", "Which string").await;
    s.ok("close", json!({ "id": "B1", "resolution": "fixed" }))
        .await;
    s.open("B", "Open one").await;
    let closed = s
        .refused(
            "decide",
            json!({ "id": "B1", "choice": "", "basis": "b", "area": "kites" }),
        )
        .await;
    assert!(closed.contains("closed"), "{closed}");
    let unsorted = s
        .refused(
            "decide",
            json!({ "id": "B2", "choice": "", "basis": "b", "area": "unsorted" }),
        )
        .await;
    assert!(unsorted.contains("unsorted"), "{unsorted}");
    let question = s
        .refused(
            "decide",
            json!({ "id": "Q1", "choice": "", "basis": "b", "area": "kites" }),
        )
        .await;
    assert!(question.contains("docket answer"), "{question}");
}

#[tokio::test]
async fn test_decide_with_an_area_is_refused_once_the_project_has_area_rows() {
    let s = two_areas().await;
    s.open("B", "Mend the kite").await;
    let why = s
        .refused(
            "decide",
            json!({ "id": "B1", "choice": "", "basis": "b", "area": "kites" }),
        )
        .await;
    assert!(why.contains("docket edit B1 --area kites"), "{why}");
}

async fn two_releases() -> Scratch {
    let s = Scratch::new().await;
    for name in ["1.0.0", "1.1.0"] {
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
    filed(&s, "T", "1.0.0").await;
    filed(&s, "T", "1.1.0").await;
    filed(&s, "T", "1.0.0").await;
    assert_eq!(
        s.refused("wait", json!({ "id": "T1", "on": "T2" })).await,
        "T1 (1.0.0) would be held by T2 (1.1.0), which ships later. Move T2 to 1.0.0, or move T1 to 1.1.0, or pass --force."
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
    filed(&s, "A", "1.0.0").await;
    filed(&s, "T", "1.1.0").await;
    assert_eq!(
        s.refused("parent", json!({ "a": ["T1"], "plan": "A1" }))
            .await,
        "A1 (1.0.0) would be held by T1 (1.1.0), which ships later. Move T1 to 1.0.0, or move A1 to 1.1.0, or pass --force."
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
    filed(&s, "T", "1.0.0").await;
    filed(&s, "T", "1.0.0").await;
    s.ok("wait", json!({ "id": "T1", "on": "T2" })).await;
    assert_eq!(
        s.refused("edit", json!({ "id": "T2", "release": "1.1.0" }))
            .await,
        "T1 (1.0.0) would be held by T2 (1.1.0), which ships later. Move T2 to 1.0.0, or move T1 to 1.1.0, or pass --carry to move them together, or --force."
    );
    assert_eq!(
        s.refused("edit", json!({ "id": "T2", "release": "" }))
            .await,
        "T1 (1.0.0) would be held by T2 (the backlog), which ships later. Move T2 to 1.0.0, or move T1 to the backlog, or pass --carry to move them together, or --force."
    );
    s.ok("edit", json!({ "id": "T1", "release": "1.1.0" }))
        .await;
    s.ok("edit", json!({ "id": "T2", "release": "1.1.0" }))
        .await;
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
async fn test_a_second_failed_unclaim_assigns_the_item_to_the_owner() {
    let s = Scratch::new().await;
    s.open("B", "Kettle trips the fuse").await;
    for attempt in 1..=2 {
        s.ok(
            "start",
            json!({ "id": "B1", "runner": "claude", "job": "b1-j" }),
        )
        .await;
        assert_eq!(s.item("B1").await["turn"], "agent", "attempt {attempt}");
        s.ok(
            "release",
            json!({ "id": "B1", "outcome": "failed", "note": "the job died" }),
        )
        .await;
        let turn = s.item("B1").await["turn"].clone();
        assert_eq!(turn, if attempt == 2 { "user" } else { "agent" });
    }
    let rows = s.assignments("B1").await;
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(
        (&rows[2]["assignee"], &rows[2]["kind"], &rows[2]["ended_at"]),
        (&json!("owner"), &json!("ask"), &Value::Null)
    );
}

#[tokio::test]
async fn test_an_unclaim_with_no_outcome_is_refused_naming_the_outcomes() {
    let s = Scratch::new().await;
    s.open("B", "Kettle trips the fuse").await;
    s.ok(
        "start",
        json!({ "id": "B1", "runner": "claude", "job": "b1-j" }),
    )
    .await;
    assert_eq!(
        s.refused("release", json!({ "id": "B1", "note": "plan filed" }))
            .await,
        "an unclaim names its outcome: one of landed, conflict, gate, blocked, failed, ended"
    );
    assert_eq!(s.assignments("B1").await[0]["ended_at"], Value::Null);
}

#[tokio::test]
async fn test_two_unclaims_that_ended_without_failing_leave_the_item_the_agents() {
    let s = Scratch::new().await;
    s.open("B", "Kettle trips the fuse").await;
    for _ in 0..2 {
        s.ok(
            "start",
            json!({ "id": "B1", "runner": "claude", "job": "b1-j" }),
        )
        .await;
        s.ok(
            "release",
            json!({ "id": "B1", "outcome": "ended", "note": "usage limit until noon" }),
        )
        .await;
    }
    assert_eq!(s.item("B1").await["turn"], "agent");
    assert_eq!(s.assignments("B1").await[1]["outcome"], "ended");
}

#[tokio::test]
async fn test_a_failed_unclaim_under_a_raised_failure_limit_stays_the_agents() {
    let s = Scratch::new().await;
    s.open("B", "Kettle trips the fuse").await;
    s.db.seed(&format!(
        "UPDATE projects SET skills = '{{\"failure_limit\": \"3\"}}' WHERE slug = '{SLUG}'"
    ))
    .await;
    for _ in 0..2 {
        s.ok(
            "start",
            json!({ "id": "B1", "runner": "claude", "job": "b1-j" }),
        )
        .await;
        s.ok("release", json!({ "id": "B1", "outcome": "failed" }))
            .await;
    }
    assert_eq!(s.item("B1").await["turn"], "agent");
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
async fn test_a_job_report_stores_its_start_end_and_exit_on_the_open_attempt() {
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
                "exit": 3 }),
    )
    .await;
    s.ok("job-report", json!({ "id": "B1", "tokens_out": 7 }))
        .await;
    let rows = s.assignments("B1").await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        (
            &rows[0]["job_started_at"],
            &rows[0]["job_ended_at"],
            &rows[0]["job_exit"]
        ),
        (
            &json!("2026-10-05T10:00:00Z"),
            &json!("2026-10-05T10:20:00Z"),
            &json!(3)
        )
    );
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

#[tokio::test]
async fn test_the_claim_and_the_owners_turn_are_read_from_the_open_assignment() {
    let s = Scratch::new().await;
    s.open("T", "Wind the clock").await;
    s.open("B", "Kettle trips the fuse").await;
    s.ok(
        "start",
        json!({ "id": "T1", "runner": "codex", "job": "wind-1" }),
    )
    .await;
    s.ok("start", json!({ "id": "B1" })).await;
    s.ok(
        "ask",
        json!({ "id": "B1", "note": "plug it in at the bench" }),
    )
    .await;
    s.open("T", "Oil the hinge").await;
    s.ok("wait", json!({ "id": "T2", "until": "the oil arrives" }))
        .await;
    let read = async |path: &str| {
        let (status, out) = s
            .send(
                Method::GET,
                &format!("/{path}?project={SLUG}"),
                "ownerkey",
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{path}: {out}");
        out
    };
    assert_eq!(ids(&read("wip").await), ["T1"]);
    assert_eq!(ids(&read("todo").await), ["B1", "T3"]);
    let held = read("held").await;
    assert_eq!(held[0]["held"]["Claim"]["branch"], "audit/t-1");
    assert_eq!(held[1]["held"]["Ask"]["note"], "plug it in at the bench");
    assert_eq!(next(&s, "work").await.len(), 0);
    assert_eq!(s.word("T1").await, "in progress");
    assert_eq!(s.word("B1").await, "waiting on owner");
    assert_eq!(s.item("T2").await["wait_on"], "condition");
    let t1 = s.item("T1").await;
    assert_eq!(
        (
            &t1["claim_branch"],
            &t1["claim_runner"],
            &t1["claim_job"],
            &t1["turn"]
        ),
        (
            &json!("audit/t-1"),
            &json!("codex"),
            &json!("wind-1"),
            &json!("agent")
        )
    );
    let b1 = s.item("B1").await;
    assert_eq!(
        (&b1["turn"], &b1["turn_note"], &b1["claim_branch"]),
        (
            &json!("user"),
            &json!("plug it in at the bench"),
            &Value::Null
        )
    );
    assert_ne!(b1["asked_at"], Value::Null);
    let why = s.refused("start", json!({ "id": "B1" })).await;
    assert!(why.contains("owner's turn"), "{why}");
    s.ok("reply", json!({ "id": "B1", "note": "it trips at once" }))
        .await;
    assert_eq!(s.word("B1").await, "ready");
    s.ok("release", json!({ "id": "T1", "outcome": "ended" }))
        .await;
    assert_eq!(ids(&read("wip").await).len(), 0);
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
async fn test_wait_until_past_the_owner_limit_is_refused_and_opens_no_task() {
    let s = Scratch::new().await;
    s.db.seed(&format!(
        "UPDATE projects SET skills = '{{\"owner_limit\": \"1\"}}' WHERE slug = '{SLUG}'"
    ))
    .await;
    asked(&s, "B", "Kettle trips the fuse", json!({})).await;
    let waiter = s.open("B", "Fan rattles").await["item"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    s.ok("start", json!({ "id": waiter })).await;
    let why = s
        .refused(
            "wait",
            json!({ "id": waiter, "until": "the vendor ships the part" }),
        )
        .await;
    assert!(why.contains("owner_limit"), "{why}");
    let (_, listed) = s
        .send(
            Method::GET,
            &format!("/todo?project={SLUG}"),
            "ownerkey",
            None,
        )
        .await;
    assert!(!listed.to_string().contains("vendor ships"), "{listed}");
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

#[tokio::test]
async fn test_moving_a_plan_before_its_child_is_refused_and_carry_moves_both() {
    let s = two_releases().await;
    filed(&s, "A", "1.1.0").await;
    filed(&s, "T", "1.1.0").await;
    s.ok("parent", json!({ "a": ["T1"], "plan": "A1" })).await;
    assert_eq!(
        s.refused("edit", json!({ "id": "A1", "release": "1.0.0" }))
            .await,
        "A1 (1.0.0) would be held by T1 (1.1.0), which ships later. Move T1 to 1.0.0, or move A1 to 1.1.0, or pass --carry to move them together, or --force."
    );
    s.ok(
        "edit",
        json!({ "id": "A1", "release": "1.0.0", "carry": true }),
    )
    .await;
    assert_eq!(s.item("A1").await["release"], "1.0.0");
    assert_eq!(s.item("T1").await["release"], "1.0.0");
    for id in ["A1", "T1"] {
        let moved: Vec<Value> = s
            .events(id)
            .await
            .into_iter()
            .filter(|e| {
                e["kind"] == "edited"
                    && e["note"].as_str().is_some_and(|n| n.starts_with("release"))
            })
            .collect();
        assert_eq!(moved.len(), 1, "{id}: {moved:?}");
    }
    let note = |e: &Value| e["note"].as_str().unwrap().to_string();
    let child = s.events("T1").await;
    assert!(
        child.iter().any(|e| note(e).contains("with A1")),
        "{child:?}"
    );
}

#[tokio::test]
async fn test_moving_a_release_later_than_a_plan_holding_its_child_is_refused_and_carry_moves_both()
{
    let s = two_releases().await;
    s.ok("releases", json!({ "action": "add", "name": "0.9.0" }))
        .await;
    filed(&s, "A", "1.0.0").await;
    filed(&s, "T", "0.9.0").await;
    s.ok("parent", json!({ "a": ["T1"], "plan": "A1" })).await;
    assert_eq!(
        s.refused(
            "releases",
            json!({ "action": "move", "name": "0.9.0", "to": "1.1.0" }),
        )
        .await,
        "A1 (1.0.0) would be held by T1 (1.1.0), which ships later. Move T1 to 1.0.0, or move A1 to 1.1.0, or pass --carry to move them together, or --force."
    );
    assert_eq!(s.item("T1").await["release"], "0.9.0");
    s.ok(
        "releases",
        json!({ "action": "move", "name": "0.9.0", "to": "1.1.0", "carry": true }),
    )
    .await;
    assert_eq!(s.item("T1").await["release"], "1.1.0");
    assert_eq!(s.item("A1").await["release"], "1.1.0");
    let events = s.events("A1").await;
    assert!(
        events
            .iter()
            .any(|e| e["note"].as_str().is_some_and(|n| n.contains("with T1"))),
        "{events:?}"
    );
}

#[tokio::test]
async fn test_carrying_a_dependency_later_moves_its_dependants_and_plans() {
    let s = two_releases().await;
    filed(&s, "A", "1.0.0").await;
    filed(&s, "T", "1.0.0").await;
    filed(&s, "T", "1.0.0").await;
    s.ok("parent", json!({ "a": ["T1"], "plan": "A1" })).await;
    s.ok("wait", json!({ "id": "T1", "on": "T2" })).await;
    assert!(
        s.refused("edit", json!({ "id": "T2", "release": "1.1.0" }))
            .await
            .contains("T1 (1.0.0)")
    );
    s.ok(
        "edit",
        json!({ "id": "T2", "release": "1.1.0", "carry": true }),
    )
    .await;
    for id in ["T1", "T2", "A1"] {
        assert_eq!(s.item(id).await["release"], "1.1.0", "{id}");
    }
}

#[tokio::test]
async fn test_a_child_filed_under_a_plan_takes_the_plans_release_and_never_a_later_one() {
    let s = two_releases().await;
    filed(&s, "A", "1.0.0").await;
    s.ok(
        "new",
        json!({ "key": "T", "title": "Filed", "parent": "A1" }),
    )
    .await;
    assert_eq!(s.item("T1").await["release"], "1.0.0");
    assert_eq!(s.item("T1").await["parent"], "A1");
    assert_eq!(
        s.refused(
            "new",
            json!({ "key": "T", "title": "Later", "parent": "A1", "release": "1.1.0" })
        )
        .await,
        "A1 (1.0.0) would be held by T2 (1.1.0), which ships later. Move T2 to 1.0.0, or move A1 to 1.1.0, or pass --force."
    );
}

async fn two_areas() -> Scratch {
    let mut s = Scratch::bare().await;
    for name in ["lanterns", "kites"] {
        s.ok("areas", json!({ "action": "add", "name": name }))
            .await;
    }
    s.filing_area = Some("lanterns");
    s
}

#[tokio::test]
async fn test_an_item_put_under_a_plan_takes_the_plans_area_to_any_depth() {
    let s = two_areas().await;
    s.ok(
        "new",
        json!({ "key": "A", "title": "Light the festival", "area": "lanterns" }),
    )
    .await;
    s.ok(
        "new",
        json!({ "key": "A", "title": "Fly at the shore", "area": "kites" }),
    )
    .await;
    s.ok(
        "new",
        json!({ "key": "T", "title": "Tie the tails", "parent": "A2" }),
    )
    .await;
    assert_eq!(s.item("T1").await["area"], "kites");
    s.ok("parent", json!({ "a": ["A2"], "plan": "A1" })).await;
    for id in ["A2", "T1"] {
        assert_eq!(s.item(id).await["area"], "lanterns", "{id}");
        let carried = s.events(id).await.into_iter().any(|e| {
            e["kind"] == "edited"
                && e["note"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("area lanterns") && n.contains("A1"))
        });
        assert!(carried, "{id}");
    }
    assert_eq!(
        s.refused("edit", json!({ "id": "A2", "area": "kites" }))
            .await,
        "A2 is under A1, and its area is A1's: edit the area of A1 instead"
    );
    s.ok("edit", json!({ "id": "A1", "area": "kites" })).await;
    for id in ["A1", "A2", "T1"] {
        assert_eq!(s.item(id).await["area"], "kites", "{id}");
    }
    s.ok("parent", json!({ "a": ["A2"] })).await;
    assert_eq!(s.item("A2").await["area"], "kites");
}

#[tokio::test]
async fn test_a_write_naming_an_area_the_project_lacks_is_refused_with_its_areas() {
    let s = two_areas().await;
    assert_eq!(
        s.refused(
            "new",
            json!({ "key": "T", "title": "Fold the hulls", "area": "Boats" })
        )
        .await,
        "Boats is not an area here: give one of lanterns, kites"
    );
    s.ok("add", json!({ "title": "Fold the hulls", "area": "Kites" }))
        .await;
    assert_eq!(s.item("B1").await["area"], "kites");
    s.ok(
        "new",
        json!({ "key": "A", "title": "Light", "area": "lanterns" }),
    )
    .await;
    assert_eq!(
        s.refused(
            "new",
            json!({ "key": "T", "title": "Wick", "parent": "A1", "area": "kites" })
        )
        .await,
        "an item under A1 takes its area, lanterns: leave out --area"
    );
}

#[tokio::test]
async fn test_areas_are_kept_by_the_owner_and_removed_only_when_nothing_carries_them() {
    let s = two_areas().await;
    assert_eq!(
        s.refused("areas", json!({ "action": "add", "name": "Lanterns" }))
            .await,
        "Lanterns is already an area: lanterns"
    );
    let out = s
        .ok(
            "areas",
            json!({ "action": "add", "name": "boats", "about": "paper hulls", "priority": "high" }),
        )
        .await;
    assert_eq!(out["areas"][2]["description"], "paper hulls");
    assert_eq!(out["areas"][2]["priority"], "high");
    let out = s
        .ok(
            "areas",
            json!({ "action": "move", "name": "boats", "to": 1 }),
        )
        .await;
    let names: Vec<&str> = out["areas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["boats", "lanterns", "kites"]);
    let out = s
        .ok(
            "areas",
            json!({ "action": "edit", "name": "boats", "rename": "rafts", "priority": "" }),
        )
        .await;
    assert_eq!(out["areas"][0]["name"], "rafts");
    assert_eq!(out["areas"][0]["priority"], Value::Null);
    s.ok(
        "new",
        json!({ "key": "T", "title": "Lash", "area": "rafts" }),
    )
    .await;
    assert_eq!(
        s.refused("areas", json!({ "action": "rm", "name": "rafts" }))
            .await,
        "rafts is carried by 1 items: T1. Move them to another area first."
    );
    s.ok("edit", json!({ "id": "T1", "area": "kites" })).await;
    let out = s
        .ok("areas", json!({ "action": "rm", "name": "rafts" }))
        .await;
    assert_eq!(out["areas"].as_array().unwrap().len(), 2);
    assert_eq!(out["areas"][0]["position"], 0);
    let (status, _) = s
        .post_as(
            "agentkey",
            "areas",
            json!({ "action": "add", "name": "sails" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_graph_nodes_carry_the_name_of_their_area() {
    let s = two_areas().await;
    s.ok(
        "new",
        json!({ "key": "T", "title": "Tie the tails", "area": "kites" }),
    )
    .await;
    let (status, graph) = s
        .send(
            Method::GET,
            &format!("/graph?project={SLUG}"),
            "ownerkey",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{graph}");
    assert_eq!(graph["nodes"][0]["area"], "kites");
}

#[tokio::test]
async fn test_a_label_is_given_to_an_item_and_read_by_everything_under_it() {
    let s = Scratch::new().await;
    s.ok("new", json!({ "key": "A", "title": "Move the data" }))
        .await;
    s.ok(
        "new",
        json!({ "key": "T", "title": "Copy it", "parent": "A1" }),
    )
    .await;
    s.ok("new", json!({ "key": "T", "title": "Alone" })).await;
    let out = s
        .ok(
            "label",
            json!({ "action": "add", "id": "A1", "name": "area:sync", "about": "moves data between replicas" }),
        )
        .await;
    assert_eq!(out["labels"][0]["name"], "area:sync");
    assert_eq!(s.item("A1").await["labels"], json!(["area:sync"]));
    assert_eq!(s.item("T1").await["labels"], json!(["area:sync"]));
    assert_eq!(s.item("T2").await["labels"], json!([]));
    let (_, listed) = s
        .send(
            Method::GET,
            &format!("/labels?project={SLUG}"),
            "ownerkey",
            None,
        )
        .await;
    assert_eq!(
        listed,
        json!([{"name": "area:sync", "description": "moves data between replicas", "items": 1}])
    );
    let noted = s
        .events("A1")
        .await
        .into_iter()
        .any(|e| e["kind"] == "edited" && e["note"] == "label area:sync");
    assert!(noted);
    s.ok(
        "label",
        json!({ "action": "add", "id": "T2", "name": "AREA:sync" }),
    )
    .await;
    assert_eq!(s.item("T2").await["labels"], json!(["area:sync"]));
    s.ok(
        "label",
        json!({ "action": "rm", "id": "A1", "name": "area:sync" }),
    )
    .await;
    assert_eq!(s.item("T1").await["labels"], json!([]));
    assert_eq!(
        s.refused(
            "label",
            json!({ "action": "rm", "id": "A1", "name": "area:sync" })
        )
        .await,
        "A1 does not carry the label area:sync"
    );
}

#[tokio::test]
async fn test_an_agent_may_label_an_item_and_a_blank_name_is_refused() {
    let s = Scratch::new().await;
    s.ok("new", json!({ "key": "T", "title": "Copy it" })).await;
    let (status, _) = s
        .post_as(
            "agentkey",
            "label",
            json!({ "action": "add", "id": "T1", "name": "slow" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        s.refused("label", json!({ "action": "add", "id": "T1", "name": " " }))
            .await,
        "a label needs a name"
    );
}

#[tokio::test]
async fn test_an_item_filed_with_no_area_is_refused_naming_the_areas_or_how_to_add_one() {
    let s = Scratch::bare().await;
    let why = s.refused("new", json!({ "key": "T", "title": "x" })).await;
    assert!(why.contains("docket areas add"), "{why}");
    for name in ["kites", "lanterns"] {
        s.ok("areas", json!({ "action": "add", "name": name }))
            .await;
    }
    let why = s.refused("new", json!({ "key": "T", "title": "x" })).await;
    assert!(why.contains("kites, lanterns"), "{why}");
    let why = s.refused("add", json!({ "title": "x" })).await;
    assert!(why.contains("kites, lanterns"), "{why}");
}

#[tokio::test]
async fn test_an_item_filed_under_a_plan_takes_the_plans_area_and_one_naming_an_area_takes_it() {
    let s = Scratch::bare().await;
    for name in ["kites", "lanterns"] {
        s.ok("areas", json!({ "action": "add", "name": name }))
            .await;
    }
    s.ok(
        "new",
        json!({ "key": "A", "title": "Fly", "area": "kites" }),
    )
    .await;
    s.ok("new", json!({ "key": "T", "title": "x", "parent": "A1" }))
        .await;
    assert_eq!(s.item("T1").await["area"], "kites");
    s.ok(
        "new",
        json!({ "key": "T", "title": "y", "area": "lanterns" }),
    )
    .await;
    assert_eq!(s.item("T2").await["area"], "lanterns");
}

/// A project with an area `kites` and a history area `unsorted` holding the done task `T1` and the done plan `A1`.
async fn with_history_area() -> Scratch {
    let s = Scratch::bare().await;
    s.ok("areas", json!({ "action": "add", "name": "kites" }))
        .await;
    s.ok("areas", json!({ "action": "add", "name": "unsorted" }))
        .await;
    s.ok(
        "new",
        json!({ "key": "T", "title": "Mend the kite", "area": "unsorted" }),
    )
    .await;
    s.ok("close", json!({ "id": "T1", "resolution": "fixed" }))
        .await;
    s.ok(
        "new",
        json!({ "key": "A", "title": "Old plan", "area": "unsorted" }),
    )
    .await;
    s.ok("close", json!({ "id": "A1", "resolution": "done" }))
        .await;
    s.db.seed(&format!(
        "UPDATE areas SET history=true WHERE project='{SLUG}' AND name='unsorted'"
    ))
    .await;
    s
}

#[tokio::test]
async fn test_new_into_a_history_area_is_refused_naming_the_other_areas() {
    let s = with_history_area().await;
    let why = s
        .refused(
            "new",
            json!({ "key": "T", "title": "x", "area": "unsorted" }),
        )
        .await;
    assert!(why.contains("kites"), "{why}");
}

#[tokio::test]
async fn test_reopen_from_a_history_area_needs_an_area_and_moves_the_item_there() {
    let s = with_history_area().await;
    let why = s
        .refused("reopen", json!({ "id": "T1", "why": "again" }))
        .await;
    assert!(why.contains("--area") && why.contains("kites"), "{why}");
    s.ok(
        "reopen",
        json!({ "id": "T1", "why": "again", "area": "kites" }),
    )
    .await;
    assert_eq!(s.item("T1").await["area"], "kites");
    let events = s.events("T1").await;
    assert!(events.iter().any(|e| e["kind"] == "edited"), "{events:?}");
    let into = s
        .refused(
            "reopen",
            json!({ "id": "T1", "why": "again", "area": "kites" }),
        )
        .await;
    assert!(into.contains("open"), "{into}");
}

#[tokio::test]
async fn test_reopen_into_a_history_area_is_refused() {
    let s = with_history_area().await;
    let why = s
        .refused(
            "reopen",
            json!({ "id": "T1", "why": "again", "area": "unsorted" }),
        )
        .await;
    assert!(why.contains("kites"), "{why}");
}

#[tokio::test]
async fn test_edit_area_into_a_history_area_is_refused_on_an_open_item_only() {
    let s = with_history_area().await;
    s.ok(
        "reopen",
        json!({ "id": "T1", "why": "again", "area": "kites" }),
    )
    .await;
    let why = s
        .refused("edit", json!({ "id": "T1", "area": "unsorted" }))
        .await;
    assert!(why.contains("kites"), "{why}");
    s.ok("close", json!({ "id": "T1", "resolution": "fixed" }))
        .await;
    s.ok("edit", json!({ "id": "T1", "area": "unsorted" }))
        .await;
}

#[tokio::test]
async fn test_parent_under_a_closed_plan_in_a_history_area_names_the_plan_area() {
    let s = with_history_area().await;
    s.ok(
        "new",
        json!({ "key": "T", "title": "Fresh", "area": "kites" }),
    )
    .await;
    let why = s
        .refused("parent", json!({ "a": ["T2"], "plan": "A1" }))
        .await;
    assert!(why.contains("unsorted"), "{why}");
}

#[tokio::test]
async fn test_a_theme_and_a_group_are_labels_that_next_groups_and_audit_read() {
    let s = Scratch::new().await;
    s.ok(
        "new",
        json!({ "key": "T", "title": "Oil the hinges", "theme": "ci", "group": "sweep" }),
    )
    .await;
    s.open("T", "Paint the fence").await;
    assert_eq!(s.item("T1").await["labels"], json!(["ci", "group:sweep"]));
    assert_eq!(s.item("T1").await["group"], "sweep");
    let read = |path: String| {
        let s = &s;
        async move {
            let (status, out) = s.send(Method::GET, &path, "ownerkey", None).await;
            assert_eq!(status, StatusCode::OK, "{path}: {out}");
            out
        }
    };
    assert_eq!(ids(&read(format!("/groups?project={SLUG}")).await), ["T1"]);
    assert_eq!(
        ids(&read(format!("/groups?project={SLUG}&name=sweep")).await),
        ["T1"]
    );
    let audit = read(format!("/audit?project={SLUG}&group=sweep")).await;
    assert_eq!(ids(&audit["rows"]), ["T1"]);
    let audit = read(format!("/audit?project={SLUG}&theme=ci")).await;
    assert_eq!(ids(&audit["rows"]), ["T1"]);
    assert_eq!(
        ids(&read(format!("/next?project={SLUG}&theme=ci")).await),
        ["T1"]
    );
    assert_eq!(
        ids(&read(format!("/next?project={SLUG}&label=ci")).await),
        ["T1"]
    );
    s.ok(
        "edit",
        json!({ "id": "T2", "set": [
            { "field": "tags", "value": "slow,wet" },
            { "field": "group", "value": "sweep" }
        ] }),
    )
    .await;
    assert_eq!(
        s.item("T2").await["labels"],
        json!(["group:sweep", "slow", "wet"])
    );
    s.ok(
        "edit",
        json!({ "id": "T2", "set": [
            { "field": "tags", "value": "slow" },
            { "field": "group", "value": "" }
        ] }),
    )
    .await;
    assert_eq!(s.item("T2").await["labels"], json!(["slow"]));
}

#[tokio::test]
async fn test_an_items_repository_is_a_label_its_plan_passes_down_and_next_reads() {
    let s = Scratch::new().await;
    s.ok(
        "new",
        json!({ "key": "T", "title": "Oil the hinges", "repo": "./kites/" }),
    )
    .await;
    assert_eq!(s.item("T1").await["labels"], json!(["repo:kites"]));
    assert_eq!(s.item("T1").await["repo"], "kites");
    s.open("A", "Hang the lanterns").await;
    s.ok(
        "edit",
        json!({ "id": "A1", "set": [{ "field": "repo", "value": "lanterns" }] }),
    )
    .await;
    s.open("T", "Trim the wicks").await;
    s.ok("parent", json!({ "a": ["T2"], "plan": "A1" })).await;
    assert_eq!(s.item("T2").await["repo"], "lanterns");
    s.open("T", "Sweep the yard").await;
    assert!(s.item("T3").await["repo"].is_null());

    s.ok(
        "edit",
        json!({ "id": "T1", "set": [{ "field": "tags", "value": "slow" }] }),
    )
    .await;
    assert_eq!(s.item("T1").await["repo"], "kites", "tags leave the repo");
    s.ok(
        "label",
        json!({ "id": "T1", "name": "repo:./lanterns/", "action": "add" }),
    )
    .await;
    assert_eq!(s.item("T1").await["repos"], json!(["kites", "lanterns"]));
    assert_eq!(s.item("T2").await["repos"], json!(["lanterns"]));
    let why = s
        .refused(
            "new",
            json!({ "key": "T", "title": "Climb out", "repo": "../elsewhere" }),
        )
        .await;
    assert!(why.contains("under the project root"), "{why}");

    let next = |repo: &'static str| {
        let s = &s;
        async move {
            let path = format!("/next?project={SLUG}&repo={repo}");
            let (status, out) = s.send(Method::GET, &path, "ownerkey", None).await;
            assert_eq!(status, StatusCode::OK, "{path}: {out}");
            let mut found = ids(&out);
            found.sort();
            found
        }
    };
    assert_eq!(next("kites").await, ["T1"]);
    assert_eq!(next("lanterns").await, ["T1", "T2"]);
    assert_eq!(
        next(".").await,
        ["T3"],
        "an item with none takes the checkout fact"
    );
    s.ok("fact", json!({ "key": "checkout", "value": "kites" }))
        .await;
    assert_eq!(next("kites").await, ["T1", "T3"]);

    s.ok(
        "edit",
        json!({ "id": "T1", "set": [{ "field": "repo", "value": "" }] }),
    )
    .await;
    assert_eq!(s.item("T1").await["labels"], json!(["slow"]));
}

#[tokio::test]
async fn test_an_item_names_several_repositories_on_new_and_edit() {
    let s = Scratch::new().await;
    s.ok(
        "new",
        json!({ "key": "T", "title": "Oil the hinges", "repo": "kites, @acme/lib ,kites" }),
    )
    .await;
    let item = s.item("T1").await;
    assert_eq!(item["repos"], json!(["@acme/lib", "kites"]));
    s.ok(
        "edit",
        json!({ "id": "T1", "set": [{ "field": "repo", "value": "lanterns,@acme/lib" }] }),
    )
    .await;
    assert_eq!(
        s.item("T1").await["labels"],
        json!(["repo:@acme/lib", "repo:lanterns"])
    );
    let why = s
        .refused(
            "new",
            json!({ "key": "T", "title": "Climb out", "repo": "kites,../elsewhere" }),
        )
        .await;
    assert!(why.contains("under the project root"), "{why}");
}

async fn waiting_ids(s: &Scratch, on: &str) -> Vec<String> {
    let (status, out) = s
        .send(
            Method::GET,
            &format!("/waiting?project={SLUG}{on}"),
            "ownerkey",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "waiting: {out}");
    ids(&out)
}

#[tokio::test]
async fn test_a_wait_is_read_from_its_dependency_and_never_from_the_wait_columns() {
    let s = Scratch::new().await;
    s.open("T", "Glaze the vases").await;
    s.open("T", "Sand the stands").await;
    s.open("Q", "Which glaze").await;
    s.ok("wait", json!({ "id": "T1", "on": "Q1" })).await;
    let rows: Option<i64> = crate::store::scalar(
        &s.db.db,
        "SELECT count(*) FROM dependencies d JOIN items w ON w.rid=d.rid JOIN items q ON q.rid=d.on_rid \
         WHERE w.id='T1' AND q.id='Q1'",
        vec![],
    )
    .await
    .unwrap();
    assert_eq!(rows, Some(1));
    assert_eq!(waiting_ids(&s, "").await, vec!["T1"]);
    assert_eq!(waiting_ids(&s, "&on=item").await, vec!["T1"]);
    assert_eq!(s.word("T1").await, "blocked");
    assert_eq!(s.item("T1").await["wait_ref"], "Q1");
    assert_eq!(s.word("T2").await, "ready");
    assert_eq!(s.item("T2").await["wait_on"], Value::Null);
    let queue = next(&s, "work").await;
    assert!(queue.contains(&"T2".to_string()), "{queue:?}");
    assert!(!queue.contains(&"T1".to_string()), "{queue:?}");
    let out = s
        .ok("answer", json!({ "id": "Q1", "decision": "celadon" }))
        .await;
    assert_eq!(ids(&out["released"]), vec!["T1"]);
    assert_eq!(s.word("T1").await, "ready");
    assert_eq!(waiting_ids(&s, "").await.len(), 0);
}

#[tokio::test]
async fn test_a_wait_until_a_condition_lists_as_a_condition_and_goes_stale_from_its_dependency() {
    let s = Scratch::new().await;
    s.open("T", "Repaint the kiln room").await;
    s.open("T", "Sweep the shelves").await;
    s.ok(
        "wait",
        json!({ "id": "T1", "until": "the kiln has cooled" }),
    )
    .await;
    s.ok("wait", json!({ "id": "T2", "on": "T1" })).await;
    assert_eq!(waiting_ids(&s, "&on=condition").await, vec!["T1"]);
    assert_eq!(waiting_ids(&s, "&on=item").await, vec!["T2"]);
    assert_eq!(s.item("T1").await["wait_on"], "condition");
    s.db.seed(&format!(
        "UPDATE dependencies SET created_at='2026-01-01T00:00:00Z' \
         WHERE rid=(SELECT rid FROM items WHERE project='{SLUG}' AND id='T1'); \
         UPDATE events SET at='2026-01-01T00:00:00Z' \
         WHERE rid=(SELECT rid FROM items WHERE project='{SLUG}' AND id='T1')"
    ))
    .await;
    let (status, out) = s
        .send(
            Method::GET,
            &format!("/check?project={SLUG}"),
            "ownerkey",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "check: {out}");
    let stale: Vec<&Value> = out
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["kind"] == "stale_wait")
        .collect();
    assert_eq!(stale.len(), 1, "{out}");
    assert_eq!(stale[0]["id"], "T1");
    assert_eq!(stale[0]["until"], "the kiln has cooled");
    s.ok(
        "close",
        json!({ "id": "T3", "resolution": "it has cooled" }),
    )
    .await;
    assert_eq!(s.word("T1").await, "ready");
    assert_eq!(waiting_ids(&s, "").await, vec!["T2"]);
}

#[tokio::test]
async fn test_remap_carries_closing_shas_through_a_map_and_records_each() {
    let s = Scratch::new().await;
    for t in ["First", "Second", "Third", "Fourth"] {
        s.open("B", t).await;
    }
    let (old_a, old_b) = (
        "aaaaaaaaa1111111111111111111111111111111",
        "bbbbbbbbb2222222222222222222222222222222",
    );
    let (new_a, new_b) = (
        "1234567890abcdef1234567890abcdef12345678",
        "fedcba0987654321fedcba0987654321fedcba09",
    );
    s.ok(
        "close",
        json!({ "id": "B1", "resolution": "aaaaaaaaa11 first words" }),
    )
    .await;
    s.ok(
        "close",
        json!({ "id": "B2", "resolution": format!("{old_b} second") }),
    )
    .await;
    s.ok(
        "drop",
        json!({ "id": "B3", "why": "bbbbbbbbb22 not needed" }),
    )
    .await;
    let map = json!([[old_a, new_a], [old_b, new_b]]);
    let dry = s.ok("remap", json!({ "map": map, "dry_run": true })).await;
    assert_eq!(dry["rows"].as_array().unwrap().len(), 3);
    assert_eq!(s.item("B1").await["resolution"], "aaaaaaaaa11 first words");
    let out = s.ok("remap", json!({ "map": map })).await;
    assert_eq!(
        out["rows"][0],
        json!({ "id": "B1", "old": "aaaaaaaaa11", "new": "1234567890a" })
    );
    assert_eq!(s.item("B1").await["resolution"], "1234567890a first words");
    assert_eq!(s.item("B2").await["resolution"], format!("{new_b} second"));
    assert_eq!(s.item("B3").await["resolution"], "fedcba09876 not needed");
    assert_eq!(s.item("B4").await["state"], "open");
    let kinds = |e: Vec<Value>| e.iter().filter(|e| e["kind"] == "remapped").count();
    assert_eq!(kinds(s.events("B1").await), 1);
    assert_eq!(kinds(s.events("B4").await), 0);
    let again = s.ok("remap", json!({ "map": map })).await;
    assert_eq!(again["rows"].as_array().unwrap().len(), 0);
    assert_eq!(kinds(s.events("B1").await), 1);
}

#[tokio::test]
async fn test_remap_is_the_owners() {
    let s = Scratch::new().await;
    let (status, _) = s.post_as("agentkey", "remap", json!({ "map": [] })).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// How many rows of a table carry the slug.
async fn rows_of(s: &Scratch, table: &str, slug: &str) -> i64 {
    use sea_orm::ConnectionTrait;
    s.db.db
        .query_one_raw(docket_migration::statement(
            &format!("SELECT COUNT(*) FROM {table} WHERE project=?"),
            vec![slug.into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index::<i64>(0)
        .unwrap()
}

/// One row in every table that carries a slug, and a second project whose labels name this one.
const ROWS_TO_MOVE: &str = "\
    INSERT INTO projects (slug, created_at, updated_at) VALUES ('peer/app', 'c', 'u'); \
    INSERT INTO labels (project, name) VALUES \
      ('peer/app', 'repo:@test/proj/sub'), ('peer/app', 'repo:@test/proj'), ('peer/app', 'repo:@test/project'); \
    INSERT INTO releases (project, name, position) VALUES ('test/proj', '1.0.0', 1); \
    INSERT INTO publications (project, published_sha, work_sha, created_at) VALUES \
      ('test/proj', repeat('a', 40), repeat('b', 40), 'c'); \
    INSERT INTO roots (host, path, project, bound_at, how) VALUES ('bench', '/srv/proj', 'test/proj', 'b', 'bind'); \
    INSERT INTO pending_dump (rid, project) VALUES (1, 'test/proj') ON CONFLICT (rid) DO UPDATE SET project=EXCLUDED.project; \
    INSERT INTO chores (name, project) VALUES ('dump', 'test/proj'); \
    INSERT INTO meta (k, v) VALUES ('pending_dump_projects', '[\"test/proj\"]') \
      ON CONFLICT (k) DO UPDATE SET v=EXCLUDED.v";

/// The project with an item carrying a label, and every row the seed adds, before any rename.
async fn with_rows_to_move() -> Scratch {
    let s = Scratch::new().await;
    s.open("T", "First").await;
    s.ok(
        "label",
        json!({ "action": "add", "id": "T1", "name": "slow-path" }),
    )
    .await;
    s.db.seed(ROWS_TO_MOVE).await;
    s
}

/// One value of a query on the scratch database.
async fn read<T: sea_orm::TryGetable>(s: &Scratch, text: &str, values: Vec<sea_orm::Value>) -> T {
    use sea_orm::ConnectionTrait;
    s.db.db
        .query_one_raw(docket_migration::statement(text, values))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index::<T>(0)
        .unwrap()
}

#[tokio::test]
async fn test_projects_rename_moves_every_row_and_records_the_old_slug() {
    let s = with_rows_to_move().await;
    let dry = s
        .ok(
            "projects-rename",
            json!({ "new": "other/name", "dry_run": true }),
        )
        .await;
    assert_eq!(dry["moved"]["items"], 1);
    assert_eq!(dry["moved"]["labels"], 1);
    assert_eq!(dry["labels"].as_array().unwrap().len(), 2);
    assert_eq!(rows_of(&s, "items", "test/proj").await, 1);
    let out = s
        .ok("projects-rename", json!({ "new": "other/name" }))
        .await;
    assert_eq!(out["old"], "test/proj");
    assert_eq!(out["new"], "other/name");
    for table in [
        "items",
        "events",
        "releases",
        "areas",
        "labels",
        "publications",
        "roots",
        "pending_dump",
        "chores",
    ] {
        let moved = out["moved"][table].as_i64().unwrap();
        assert!(moved >= 1, "{table}: {moved}");
        let now = moved + i64::from(table == "events");
        assert_eq!(rows_of(&s, table, "other/name").await, now, "{table}");
        assert_eq!(rows_of(&s, table, "test/proj").await, 0, "{table}");
    }
    let note: String = read(
        &s,
        "SELECT note FROM events WHERE project=? AND kind='renamed' AND rid IS NULL",
        vec!["other/name".into()],
    )
    .await;
    assert_eq!(note, "test/proj -> other/name");
    let pending: String = read(
        &s,
        "SELECT v FROM meta WHERE k='pending_dump_projects'",
        vec![],
    )
    .await;
    assert!(
        pending.contains("other/name") && !pending.contains("test/proj"),
        "{pending}"
    );
}

#[tokio::test]
async fn test_projects_rename_renames_the_labels_naming_it_and_lists_only_the_new_slug() {
    let s = with_rows_to_move().await;
    let out = s
        .ok("projects-rename", json!({ "new": "other/name" }))
        .await;
    assert_eq!(
        out["labels"],
        json!([
            { "project": "peer/app", "old": "repo:@test/proj", "new": "repo:@other/name" },
            { "project": "peer/app", "old": "repo:@test/proj/sub", "new": "repo:@other/name/sub" },
        ])
    );
    let (_, peer) = s
        .send(
            Method::GET,
            &format!("/labels?project={}", urlencode("peer/app")),
            "ownerkey",
            None,
        )
        .await;
    let mut names: Vec<&str> = peer
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "repo:@other/name",
            "repo:@other/name/sub",
            "repo:@test/project"
        ]
    );
    let (_, projects) = s.send(Method::GET, "/projects", "ownerkey", None).await;
    let mut slugs: Vec<&str> = projects
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["slug"].as_str().unwrap())
        .collect();
    slugs.sort_unstable();
    assert_eq!(slugs, ["other/name", "peer/app"]);
}

#[tokio::test]
async fn test_projects_rename_is_the_owners_and_refuses_a_taken_led_or_claimed_project() {
    let s = Scratch::new().await;
    s.open("T", "First").await;
    let (status, _) = s
        .post_as(
            "agentkey",
            "projects-rename",
            json!({ "new": "other/name" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let bad = s
        .refused("projects-rename", json!({ "new": "other/name/deep" }))
        .await;
    assert!(bad.contains("OWNER/NAME"), "{bad}");
    assert_eq!(
        s.status("projects-rename", json!({ "new": "test/proj" }))
            .await,
        StatusCode::BAD_REQUEST
    );
    s.db.seed("INSERT INTO projects (slug, created_at, updated_at) VALUES ('peer/app', 'c', 'u')")
        .await;
    let taken = s
        .refused("projects-rename", json!({ "new": "peer/app" }))
        .await;
    assert!(taken.contains("already exists"), "{taken}");
    s.db.seed(
        "INSERT INTO leads (project, host, session, since, renewed_at) \
         VALUES ('test/proj', 'loft', 's', 'a', 'r')",
    )
    .await;
    let led = s
        .refused("projects-rename", json!({ "new": "other/name" }))
        .await;
    assert!(led.contains("led from loft"), "{led}");
    s.db.seed("DELETE FROM leads WHERE project='test/proj'")
        .await;
    s.ok("start", json!({ "id": "T1" })).await;
    let claimed = s
        .refused("projects-rename", json!({ "new": "other/name" }))
        .await;
    assert!(claimed.contains("claimed (T1)"), "{claimed}");
    s.ok(
        "projects-rename",
        json!({ "new": "other/name", "force": true }),
    )
    .await;
    assert_eq!(rows_of(&s, "items", "other/name").await, 1);
    let (status, _) = s
        .send(
            Method::GET,
            &format!("/show/T1?project={}", urlencode("other/name")),
            "ownerkey",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn test_projects_rename_never_moves_under_a_running_job() {
    let s = Scratch::new().await;
    s.open("T", "First").await;
    s.ok(
        "start",
        json!({ "id": "T1", "runner": "codex", "job": "pk2-s1", "model": "gpt-5.4", "on": "devbox", "role": "build" }),
    )
    .await;
    let running = s
        .refused(
            "projects-rename",
            json!({ "new": "other/name", "force": true }),
        )
        .await;
    assert!(running.contains("running jobs (T1)"), "{running}");
    assert_eq!(rows_of(&s, "items", "test/proj").await, 1);
}

#[tokio::test]
async fn test_reopening_an_item_that_closes_a_cycle_is_refused_with_its_path() {
    let s = Scratch::new().await;
    for title in ["Throw", "Trim", "Fire"] {
        s.open("T", title).await;
    }
    s.ok("dep", json!({ "id": "T2", "on": ["T3"] })).await;
    s.ok("dep", json!({ "id": "T1", "on": ["T2"] })).await;
    s.ok(
        "drop",
        json!({ "id": "T2", "why": "waiting on nothing now" }),
    )
    .await;
    s.ok("dep", json!({ "id": "T3", "on": ["T1"] })).await;
    assert_eq!(
        s.refused("reopen", json!({ "id": "T2", "why": "needed after all" }))
            .await,
        "T2 cannot be reopened: it would close the cycle T2 -> T3 -> T1 -> T2."
    );
    assert_eq!(s.item("T2").await["state"], "dropped");
}

#[tokio::test]
async fn test_reopening_a_child_under_a_closed_plan_is_refused() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("B", "Port the proofer").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    s.ok("close", json!({ "id": "A1", "resolution": "clean" }))
        .await;
    let why = s
        .refused("reopen", json!({ "id": "B1", "why": "it slipped" }))
        .await;
    assert!(
        why.contains("under A1, which is done") && why.contains("closed plan"),
        "{why}"
    );
    assert_eq!(s.item("B1").await["state"], "done");
}

#[tokio::test]
async fn test_reopening_a_child_in_an_earlier_release_than_its_plan_is_refused_unless_forced() {
    let s = two_releases().await;
    s.ok(
        "new",
        json!({ "key": "A", "title": "The pantry plan", "release": "1.0.0" }),
    )
    .await;
    filed(&s, "B", "1.0.0").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    s.ok("edit", json!({ "id": "A1", "release": "1.1.0" }))
        .await;
    s.ok("edit", json!({ "id": "B1", "release": "1.1.0" }))
        .await;
    s.ok("edit", json!({ "id": "A1", "release": "1.0.0" }))
        .await;
    let why = s
        .refused("reopen", json!({ "id": "B1", "why": "it slipped" }))
        .await;
    assert_eq!(
        why,
        "A1 (1.0.0) would be held by B1 (1.1.0), which ships later. Move B1 to 1.0.0, or move A1 to 1.1.0, or pass --carry to move them together, or --force."
    );
    s.ok(
        "reopen",
        json!({ "id": "B1", "why": "it slipped", "force": true }),
    )
    .await;
    assert_eq!(s.word("B1").await, "ready");
}

#[tokio::test]
async fn test_reopening_an_item_of_a_shipped_release_names_a_release_that_has_not_shipped() {
    let s = two_releases().await;
    filed(&s, "T", "1.0.0").await;
    s.ok("close", json!({ "id": "T1", "resolution": "fixed" }))
        .await;
    s.ok("releases", json!({ "action": "ship", "name": "1.0.0" }))
        .await;
    assert_eq!(
        s.refused("reopen", json!({ "id": "T1", "why": "it came back" }))
            .await,
        "T1 is in 1.0.0, which has shipped: reopen it with --release current or one of 1.1.0, or \"\" for the backlog."
    );
    assert_eq!(s.item("T1").await["state"], "done");
    assert_eq!(
        s.refused(
            "reopen",
            json!({ "id": "T1", "why": "it came back", "release": "1.0.0" })
        )
        .await,
        "1.0.0 has shipped: give current or one of 1.1.0"
    );
    s.ok(
        "reopen",
        json!({ "id": "T1", "why": "it came back", "release": "current" }),
    )
    .await;
    assert_eq!(s.item("T1").await["release"], "1.1.0");
    assert_eq!(next(&s, "work").await, vec!["T1"]);
}

#[tokio::test]
async fn test_reopen_area_on_a_child_names_its_plan_and_on_a_plan_moves_its_subtree() {
    let s = Scratch::new().await;
    for area in ["kites", "ovens"] {
        s.ok("areas", json!({ "action": "add", "name": area }))
            .await;
    }
    s.ok(
        "new",
        json!({ "key": "A", "title": "The pantry plan", "area": "kites" }),
    )
    .await;
    s.ok(
        "new",
        json!({ "key": "B", "title": "Port the proofer", "parent": "A1" }),
    )
    .await;
    s.ok("close", json!({ "id": "B1", "resolution": "abc1234" }))
        .await;
    let why = s
        .refused(
            "reopen",
            json!({ "id": "B1", "why": "it slipped", "area": "ovens" }),
        )
        .await;
    assert_eq!(
        why,
        "B1 is under A1, and its area is A1's: edit the area of A1 instead"
    );
    assert_eq!(s.item("B1").await["state"], "done");
    s.ok("close", json!({ "id": "A1", "resolution": "clean" }))
        .await;
    s.ok(
        "reopen",
        json!({ "id": "A1", "why": "again", "area": "ovens" }),
    )
    .await;
    assert_eq!(s.item("A1").await["area"], "ovens");
    assert_eq!(s.item("B1").await["area"], "ovens");
}

#[tokio::test]
async fn test_a_drop_superseded_by_a_plan_its_dependant_is_under_is_refused_with_the_path() {
    let s = Scratch::new().await;
    plan(&s).await;
    s.open("T", "Throw").await;
    s.open("B", "Port the proofer").await;
    s.ok("parent", json!({ "a": ["B1"], "plan": "A1" })).await;
    s.ok("dep", json!({ "id": "B1", "on": ["T1"] })).await;
    assert_eq!(
        s.refused("drop", json!({ "id": "T1", "superseded_by": "A1" }))
            .await,
        "T1 cannot be superseded by A1: B1 would be held by A1, which closes the cycle B1 -> A1 -> B1."
    );
    assert_eq!(s.word("T1").await, "ready");
}

#[tokio::test]
async fn test_a_drop_superseded_by_an_item_of_a_later_release_is_refused_unless_forced() {
    let s = two_releases().await;
    filed(&s, "T", "1.0.0").await;
    filed(&s, "T", "1.0.0").await;
    filed(&s, "T", "1.1.0").await;
    s.ok("dep", json!({ "id": "T2", "on": ["T1"] })).await;
    assert_eq!(
        s.refused("drop", json!({ "id": "T1", "superseded_by": "T3" }))
            .await,
        "T2 (1.0.0) would be held by T3 (1.1.0), which ships later. Move T3 to 1.0.0, or move T2 to 1.1.0, or pass --force."
    );
    s.ok(
        "drop",
        json!({ "id": "T1", "superseded_by": "T3", "force": true }),
    )
    .await;
}
