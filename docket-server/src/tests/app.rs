use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header::AUTHORIZATION};
use serde_json::Value;
use tower::ServiceExt;

use docket_migration::scratch::Scratch;

use super::*;

const SEED: &str = r"
INSERT INTO projects (slug, created_at, updated_at) VALUES ('o/p', 'c', 'u');
INSERT INTO areas (id, project, name, position) VALUES (1, 'o/p', 'general', 0);
INSERT INTO items (rid, project, key, num, title, state, body, opened_at, updated_at)
  VALUES (1, 'o/p', 'T', 1, 'First', 'open', 'Body text', 'o1', 'u1'),
         (2, 'o/p', 'PK', 1, 'Package', 'open', '', 'o2', 'u2'),
         (3, 'o/p', 'CON', 1, 'Concept', 'open', '', 'o3', 'u3');
INSERT INTO labels (id, project, name) VALUES (1, 'o/p', 'group:g'), (2, 'o/p', 'single');
INSERT INTO item_labels (rid, label_id) VALUES (1, 1), (1, 2);
INSERT INTO items (rid, project, key, num, title, state, resolution, superseded_by, opened_at, updated_at)
  VALUES (4, 'o/p', 'T', 2, 'Old', 'dropped', 'replaced', 1, 'o4', 'u4');
UPDATE items SET parent_rid=2, priority='high' WHERE rid=1;
UPDATE items SET type='plan' WHERE rid=2;
INSERT INTO links (rid, kind, to_rid) VALUES (1, 'related', 3);
INSERT INTO links (rid, kind, to_path, to_line) VALUES (1, 'cites_file', 'src/a.rs', 7);
INSERT INTO events (uid, project, rid, at, host, kind) VALUES ('e1', 'o/p', 1, 'o1', 'devbox', 'opened');
";

async fn seeded() -> (Router, Scratch) {
    let s = Scratch::new(2).await;
    s.seed(SEED).await;
    s.seed(crate::tests::TYPES_BY_KEY).await;
    (app(&s.db, Keys::parse("devbox agent secret").unwrap()), s)
}

/// One request to a server over a freshly seeded database.
async fn send(method: Method, uri: &str, key: Option<&str>) -> (StatusCode, Value) {
    let (app, _db) = seeded().await;
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(key) = key {
        req = req.header(AUTHORIZATION, format!("Bearer {key}"));
    }
    let resp = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn get(uri: &str) -> (StatusCode, Value) {
    send(Method::GET, uri, Some("secret")).await
}

#[tokio::test]
async fn test_request_without_known_key_is_401() {
    for key in [None, Some("wrong"), Some("")] {
        for uri in ["/items", "/show/T1?project=o/p", "/projects"] {
            let (status, _) = send(Method::GET, uri, key).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} with {key:?}");
        }
    }
}

#[tokio::test]
async fn test_health_answers_without_a_key() {
    for key in [None, Some("wrong"), Some("secret")] {
        let (status, _) = send(Method::GET, "/health", key).await;
        assert_eq!(status, StatusCode::OK, "with {key:?}");
    }
    let (status, _) = send(Method::POST, "/health", None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_show_matches_the_json_of_docket_show() {
    let (status, body) = get("/show/T1?project=o/p").await;
    assert_eq!(status, StatusCode::OK);
    let expected: Value = serde_json::from_str(
        r#"{
        "project": "o/p", "key": "T", "num": 1, "id": "T1", "title": "First", "state": "open",
        "turn": "agent", "turn_note": null, "asked_at": null,
        "claim_branch": null, "claim_host": null, "claim_since": null,
        "claim_runner": null, "claim_job": null, "claim_on": null,
        "wait_on": null, "wait_ref": null, "wait_since": null,
        "decision": null, "decided_at": null, "resolution": null,
        "complexity": null, "release": null, "area": "general",
        "type": "task", "labels": ["group:g", "single"], "body": "Body text",
        "opened_at": "o1", "updated_at": "u1",
        "group": "g", "repo": null, "word": "ready", "priority": "high", "superseded_by": null,
        "related": ["CON1"], "parent": "PK1", "origin": [], "children": [],
        "cites": [{"path": "src/a.rs", "line": 7, "kind": "cites_file"}]
        }"#,
    )
    .unwrap();
    assert_eq!(body, expected);
}

#[tokio::test]
async fn test_show_derives_word_from_kind_and_open_members() {
    let (_, package) = get("/show/PK1?project=o/p").await;
    assert_eq!(package["word"], "under way");
    assert_eq!(get("/show/CON1?project=o/p").await.1["word"], "ready");
    let (_, dropped) = get("/show/T2?project=o/p").await;
    assert_eq!(dropped["word"], "dropped");
    assert_eq!(dropped["superseded_by"], "T1");
}

#[tokio::test]
async fn test_offers_name_the_verbs_the_rules_accept_and_the_tiers() {
    let (status, body) = get("/offers/T1?project=o/p").await;
    assert_eq!(status, StatusCode::OK);
    let verbs: Vec<&str> = body["verbs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["verb"].as_str().unwrap())
        .collect();
    assert_eq!(verbs, ["start", "close", "ask", "wait", "drop"]);
    assert_eq!(
        body["priorities"],
        serde_json::json!(["critical", "high", "normal", "low"])
    );
    assert_eq!(body["levels"], serde_json::json!(["high", "medium", "low"]));
    let (_, closed) = get("/offers/T2?project=o/p").await;
    assert_eq!(closed["verbs"][0]["verb"], "reopen");
    assert_eq!(get("/offers/T9?project=o/p").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_show_unknown_item_or_project_is_404() {
    assert_eq!(get("/show/T9?project=o/p").await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        get("/show/T1?project=o/none").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn test_resources_list_filter_and_fetch_by_key() {
    let (status, items) = get("/items").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(items.as_array().unwrap().len(), 4);
    assert!(items[0].get("body").is_none());

    let (_, dropped) = get("/items?filter=%7B%22state%22%3A%22dropped%22%7D").await;
    assert_eq!(dropped.as_array().unwrap().len(), 1);
    assert_eq!(dropped[0]["id"], "T2");

    assert_eq!(get("/items/1").await.1["body"], "Body text");
    assert_eq!(get("/links").await.1.as_array().unwrap().len(), 2);
    let (_, children) = get("/items?filter=%7B%22parent_rid%22%3A2%7D").await;
    assert_eq!(children[0]["id"], "T1");
    assert_eq!(get("/events").await.1[0]["host"], "devbox");
    assert_eq!(get("/projects").await.1[0]["slug"], "o/p");
}

#[tokio::test]
async fn test_no_route_accepts_a_write() {
    for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
        for uri in [
            "/items",
            "/items/1",
            "/events",
            "/links/1",
            "/projects",
            "/show/T1?project=o/p",
        ] {
            let (status, _) = send(method.clone(), uri, Some("secret")).await;
            assert!(
                status == StatusCode::METHOD_NOT_ALLOWED || status == StatusCode::NOT_FOUND,
                "{method} {uri} gave {status}"
            );
        }
    }
    assert_eq!(get("/items").await.1.as_array().unwrap().len(), 4);
}

/// Scenario: the server is told to stop while a client holds the change stream open.
/// Expected behaviour: the stream ends and the server returns at once.
#[tokio::test]
async fn test_serve_returns_promptly_with_a_change_stream_open() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let s = Scratch::new(2).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let keys = Keys::parse("devbox agent secret").unwrap();
    let db = s.db.clone();
    let served = tokio::spawn(async move {
        serve(listener, &db, keys, None, async {
            stopped.await.ok();
        })
        .await
    });
    let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
    client
        .write_all(b"GET /changes HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer secret\r\n\r\n")
        .await
        .unwrap();
    let mut hello = [0u8; 256];
    let n = client.read(&mut hello).await.unwrap();
    assert!(String::from_utf8_lossy(&hello[..n]).contains("200 OK"));
    stop.send(()).unwrap();
    let ended = tokio::time::timeout(std::time::Duration::from_secs(3), served).await;
    assert!(ended.is_ok(), "the server waited on the open change stream");
    ended.unwrap().unwrap().unwrap();
}

/// A directory holding a built page and one asset, removed when dropped.
struct Built(std::path::PathBuf);

impl Built {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("docket-web-{name}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("index.html"), "<p>page</p>").unwrap();
        std::fs::write(dir.join("assets/app.js"), "let a = 1;").unwrap();
        Self(dir)
    }
}

impl Drop for Built {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

async fn fetch(app: Router, uri: &str, key: Option<&str>) -> (StatusCode, String) {
    let mut req = Request::builder().uri(uri);
    if let Some(key) = key {
        req = req.header(AUTHORIZATION, format!("Bearer {key}"));
    }
    let resp = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn test_web_client_serves_the_page_and_its_routes_without_a_key() {
    let built = Built::new("routes");
    let web = web_client(&built.0);
    assert_eq!(
        fetch(web.clone(), "/ui/", None).await,
        (StatusCode::OK, "<p>page</p>".into())
    );
    assert_eq!(
        fetch(web.clone(), "/ui/assets/app.js", None).await,
        (StatusCode::OK, "let a = 1;".into())
    );
    assert_eq!(
        fetch(web.clone(), "/ui/o/p/work?i=T1", None).await,
        (StatusCode::OK, "<p>page</p>".into())
    );
    let (status, _) = fetch(web, "/", None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn test_web_client_leaves_every_route_behind_its_key() {
    let built = Built::new("keyed");
    let (app, _db) = seeded().await;
    let app = app.merge(web_client(&built.0));
    assert_eq!(
        fetch(app.clone(), "/projects", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        fetch(app.clone(), "/projects", Some("secret")).await.0,
        StatusCode::OK
    );
    assert_eq!(fetch(app, "/ui/", None).await.0, StatusCode::OK);
}

#[tokio::test]
async fn test_offers_leave_close_off_a_plan_whose_opened_work_is_open() {
    let s = Scratch::new(2).await;
    s.seed(
        r"
INSERT INTO projects (slug, created_at, updated_at) VALUES ('o/p', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, body, opened_at, updated_at)
  VALUES (1, 'o/p', 'A', 1, 'Plan', 'open', '', 'o1', 'u1'),
         (2, 'o/p', 'T', 1, 'Work', 'open', '', 'o2', 'u2');
UPDATE items SET parent_rid=1 WHERE rid=2;
",
    )
    .await;
    s.seed(TYPES_BY_KEY).await;
    let app = app(&s.db, Keys::parse("devbox agent secret").unwrap());
    let req = Request::builder()
        .uri("/offers/A1?project=o/p")
        .header(AUTHORIZATION, "Bearer secret")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    let verbs: Vec<&str> = body["verbs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["verb"].as_str().unwrap())
        .collect();
    assert!(!verbs.contains(&"close"), "{verbs:?}");
}

/// Rows seeded by raw SQL carry the default type; this sets each to the type its key is filed under, a legacy plan key to plan and a key outside the fixed five to task.
pub(crate) const TYPES_BY_KEY: &str = "UPDATE items SET type = CASE key WHEN 'Q' THEN 'question' \
    WHEN 'A' THEN 'plan' WHEN 'PK' THEN 'plan' WHEN 'STY' THEN 'plan' WHEN 'B' THEN 'bug' \
    WHEN 'I' THEN 'investigation' ELSE 'task' END";
