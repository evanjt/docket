use std::collections::BTreeMap;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use docket_core::dump::{DumpPage, commit_subject, files, messages, page_messages};

use docket_migration::scratch::Scratch;

use crate::app;
use crate::auth::Keys;

const KEYS: &str = r#"[{"key": "B", "kind": "work", "meaning": "bugs", "turn": "agent"}, {"key": "Q", "kind": "decision", "meaning": "questions", "turn": "user"}, {"key": "A", "kind": "audit", "meaning": "audits", "turn": "agent"}, {"key": "CON", "kind": "concept", "meaning": "concepts", "turn": "agent"}, {"key": "PK", "kind": "package", "meaning": "packages", "turn": "agent"}]"#;
const SLUG: &str = "test/proj";

/// A database, and the dump files as a checkout would hold them after each incremental page.
struct Dumped {
    app: Router,
    files: BTreeMap<String, String>,
    cursor: i64,
    named: Vec<String>,
    _db: Scratch,
}

impl Dumped {
    async fn new() -> Self {
        let db = Scratch::new(2).await;
        db.seed(&format!(
            "INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('{SLUG}', '{KEYS}', 'c', 'u')"
        ))
        .await;
        let keys = Keys::parse("testbox owner ownerkey").unwrap();
        let mut d = Self {
            app: app(&db.db, keys),
            files: BTreeMap::new(),
            cursor: 0,
            named: Vec::new(),
            _db: db,
        };
        d.take(0).await;
        d
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
        if body.is_some() {
            req = req.header("content-type", "application/json");
        }
        let body = body.map_or_else(Body::empty, |v| Body::from(v.to_string()));
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

    async fn page(&self, since: i64) -> DumpPage {
        let (status, out) = self
            .send(
                Method::GET,
                &format!("/dump?since={since}"),
                "ownerkey",
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{out}");
        serde_json::from_value(out).unwrap()
    }

    /// The page after `since` applied to the files; returns the commit subject it would carry.
    async fn take(&mut self, since: i64) -> String {
        let page = self.page(since).await;
        let subject = commit_subject(&page_messages(&page, |p| self.files.get(p).cloned()));
        for (path, text) in files(&page, |p| self.files.get(p).cloned()) {
            self.files.insert(path, text);
        }
        self.cursor = page.cursor;
        subject
    }

    /// One verb, then the incremental page: it must leave the files a full dump writes.
    async fn write(&mut self, verb: &str, mut body: Value) -> String {
        body["project"] = json!(SLUG);
        body["branch"] = json!("audit/t-1");
        let (status, out) = self
            .send(
                Method::POST,
                &format!("/do/{verb}"),
                "ownerkey",
                Some(body.clone()),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{verb} {body}: {out}");
        let subject = self.take(self.cursor).await;
        let full = self.page(0).await;
        let whole: BTreeMap<String, String> = files(&full, |_| None).into_iter().collect();
        let differ: Vec<&String> = whole
            .keys()
            .chain(self.files.keys())
            .filter(|p| self.files.get(*p) != whole.get(*p))
            .collect();
        assert!(
            differ.is_empty(),
            "after {verb} {body}: {differ:?} differ from a full dump"
        );
        subject
    }
}

fn cited(paths: &[&str]) -> String {
    let cites: Vec<String> = paths.iter().map(|p| format!("`{p}:1`")).collect();
    format!("- **Evidence.** {}\n", cites.join(" "))
}

#[tokio::test]
async fn test_dump_without_key_is_401() {
    let d = Dumped::new().await;
    for key in ["", "wrong"] {
        assert_eq!(
            d.send(Method::GET, "/dump", key, None).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn test_dump_full_page_carries_every_row_and_the_last_seq() {
    let mut d = Dumped::new().await;
    d.write("new", json!({ "key": "B", "title": "First" }))
        .await;
    d.write("new", json!({ "key": "B", "title": "Second" }))
        .await;
    let page = d.page(0).await;
    assert!(page.full);
    assert_eq!(page.cursor, 2);
    assert_eq!(page.projects.len(), 1);
    let ids: Vec<&str> = page.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, vec!["B1", "B2"]);
    assert_eq!(page.events[1].item.as_deref(), Some("B2"));
    assert_eq!(page.events[1].kind, "opened");
}

#[tokio::test]
async fn test_dump_after_the_cursor_is_empty_until_a_write() {
    let mut d = Dumped::new().await;
    d.write("new", json!({ "key": "B", "title": "First" }))
        .await;
    let page = d.page(d.cursor).await;
    assert!(!page.full);
    assert_eq!(page.cursor, d.cursor);
    assert!(page.items.is_empty() && page.events.is_empty());
    assert_eq!(page.projects.len(), 1);
    d.write("new", json!({ "key": "B", "title": "Second" }))
        .await;
    let page = d.page(1).await;
    let ids: Vec<&str> = page.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, vec!["B2"]);
}

/// Each step, then the incremental page: it must equal a full dump and carry the step's subject.
async fn run(d: &mut Dumped, steps: Vec<(&str, Value, &str)>) {
    for (verb, body, subject) in steps {
        assert_eq!(d.write(verb, body.clone()).await, subject, "{verb} {body}");
        if verb != "key" {
            d.named.extend(subject.split("; ").map(str::to_string));
        }
    }
}

/// Plans: tickets opened under two plans, a waiter on one of them.
async fn plans(d: &mut Dumped) {
    let steps: Vec<(&str, Value, &str)> = vec![
        (
            "new",
            json!({ "key": "B", "title": "crust timer", "body": cited(&["src/crust.ts"]) }),
            "Open B1",
        ),
        (
            "new",
            json!({ "key": "B", "title": "proof timer", "body": cited(&["src/proof.ts"]) }),
            "Open B2",
        ),
        (
            "new",
            json!({ "key": "A", "title": "Shelf plan" }),
            "Open A1",
        ),
        (
            "link",
            json!({ "a": ["B1", "B2"], "kind": "opened", "b": "A1" }),
            "Link 2 items to A1",
        ),
        (
            "new",
            json!({ "key": "A", "title": "Proofing plan" }),
            "Open A2",
        ),
        (
            "new",
            json!({ "key": "B", "title": "rise timer", "body": cited(&["src/rise.ts"]) }),
            "Open B3",
        ),
        (
            "link",
            json!({ "a": ["B3"], "kind": "opened", "b": "A2" }),
            "Link B3",
        ),
        (
            "link",
            json!({ "a": ["A2"], "kind": "related", "b": "A1" }),
            "Link A2",
        ),
        (
            "new",
            json!({ "key": "B", "title": "after shelves", "body": cited(&["src/later.ts"]) }),
            "Open B4",
        ),
        ("wait", json!({ "id": "B4", "on": "A2" }), "Wait B4"),
        (
            "priority",
            json!({ "ids": ["A2", "B1"], "tier": "high" }),
            "Prioritise 2 items",
        ),
        ("start", json!({ "id": "B3" }), "Start B3"),
    ];
    run(d, steps).await;
}

/// Questions and claims: an answer releasing a waiter, a close linking what it opened.
async fn questions(d: &mut Dumped) {
    let steps: Vec<(&str, Value, &str)> = vec![
        (
            "new",
            json!({ "key": "Q", "title": "One cache or two" }),
            "Open Q1",
        ),
        ("wait", json!({ "id": "B1", "on": "Q1" }), "Wait B1"),
        (
            "answer",
            json!({ "id": "Q1", "decision": "Two" }),
            "Answer Q1",
        ),
        ("start", json!({ "id": "Q1" }), "Start Q1"),
        (
            "close",
            json!({ "id": "Q1", "resolution": "opened B4" }),
            "Close Q1",
        ),
        (
            "decide",
            json!({ "id": "B1", "choice": "Keep it", "basis": "CID1" }),
            "Decide on B1",
        ),
        ("rate", json!({ "id": "B1", "level": "low" }), "Rate B1"),
        (
            "edit",
            json!({ "id": "B1", "set": [{ "field": "title", "value": "crust timer \u{e9}" }], "append": "more" }),
            "Edit B1",
        ),
        (
            "ask",
            json!({ "id": "B2", "note": "which unit?" }),
            "Ask B2",
        ),
        ("reply", json!({ "id": "B2", "note": "metres" }), "Reply B2"),
        (
            "release",
            json!({ "id": "B3", "note": "later" }),
            "Unclaim B3",
        ),
        ("start", json!({ "id": "B3" }), "Start B3"),
        (
            "close",
            json!({ "id": "B3", "resolution": "abc1234" }),
            "Close B3",
        ),
    ];
    run(d, steps).await;
}

/// Drops, moves, waits, links removed, a key set, an item added.
async fn moves(d: &mut Dumped) {
    let steps: Vec<(&str, Value, &str)> = vec![
        (
            "drop",
            json!({ "id": "B2", "superseded_by": "B1" }),
            "Drop B2",
        ),
        ("reopen", json!({ "id": "B2", "why": "back" }), "Reopen B2"),
        (
            "wait",
            json!({ "id": "B2", "until": "the fleet is quiet" }),
            "Open B5; Wait B2",
        ),
        ("resume", json!({ "id": "B2" }), "Resume B2"),
        (
            "link",
            json!({ "a": ["B2"], "kind": "related", "b": "B3" }),
            "Link B2",
        ),
        (
            "link",
            json!({ "a": ["B2"], "kind": "related", "b": "B3", "remove": true }),
            "Link B2",
        ),
        (
            "key",
            json!({ "key": "I", "kind": "research", "meaning": "investigations" }),
            "Key I",
        ),
        ("add", json!({ "title": "Seen on the way" }), "Open B6"),
    ];
    run(d, steps).await;
}

#[tokio::test]
async fn test_dump_after_every_verb_equals_a_full_dump_and_names_the_write() {
    let mut d = Dumped::new().await;
    plans(&mut d).await;
    questions(&mut d).await;
    moves(&mut d).await;
    assert_eq!(messages(&d.page(0).await.events), d.named);
}
