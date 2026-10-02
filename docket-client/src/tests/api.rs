use std::net::SocketAddr;
use std::sync::mpsc;

use docket_migration::scratch::Scratch;
use docket_server::app;
use docket_server::auth::Keys;

use super::*;

const SEED: &str = r#"
INSERT INTO projects (slug, keys, themes, skills, created_at, updated_at) VALUES ('o/p',
  '[{"key":"T","kind":"work","meaning":"tasks","turn":"agent"},{"key":"Q","kind":"decision"},
    {"key":"PK","kind":"package"}]',
  '[{"name":"sync","note":""}]', '{"mode":"run","land":"make land"}', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, tags, body, opened_at, updated_at) VALUES
  (1, 'o/p', 'T', 1, 'Fix the sync', 'open', 'agent', '["high"]', 'Body naming PK1', '2026-01-01T00:00:00Z', 'u1'),
  (2, 'o/p', 'PK', 1, 'Sync package', 'open', 'agent', '[]', '', '2026-01-01T00:00:00Z', 'u2'),
  (3, 'o/p', 'Q', 1, 'Which way', 'open', 'user', '[]', 'Two ways', '2026-01-01T00:00:00Z', 'u3');
INSERT INTO links (rid, kind, to_rid) VALUES (1, 'opened', 2);
INSERT INTO links (rid, kind, to_path, to_line) VALUES (1, 'cites_file', 'src/sync.rs', 4);
INSERT INTO events (uid, project, rid, at, host, kind, note) VALUES
  ('e1', 'o/p', 1, '2026-01-01T00:00:00Z', 'devbox', 'opened', NULL),
  ('e2', 'o/p', 1, '2026-01-01T00:01:00Z', 'devbox', 'edited', 'title'),
  ('e3', 'o/p', 3, '2026-01-01T00:02:00Z', 'devbox', 'asked', 'pick one');
INSERT INTO search (rid, id, title, body, files) VALUES (1, 'T1', 'Fix the sync', 'Body naming PK1', '');
"#;

/// A server on a free port over a seeded database of its own. Dropping it stops the server and
/// drops the database.
struct Served {
    api: Api,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl std::ops::Deref for Served {
    type Target = Api;

    fn deref(&self) -> &Api {
        &self.api
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.send(()).ok();
        }
        if let Some(thread) = self.thread.take() {
            thread.join().ok();
        }
    }
}

fn served() -> Served {
    let (tx, rx) = mpsc::channel::<SocketAddr>();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let db = Scratch::new(2).await;
            db.seed(SEED).await;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            tx.send(listener.local_addr().unwrap()).unwrap();
            let routes = app(&db.db, Keys::parse("devbox owner secret").unwrap());
            axum::serve(listener, routes)
                .with_graceful_shutdown(async {
                    stopped.await.ok();
                })
                .await
                .unwrap();
        });
    });
    let addr = rx.recv().unwrap();
    let api = Api::new(&Config {
        server: format!("http://{addr}"),
        key: "secret".into(),
    })
    .unwrap();
    Served {
        api,
        stop: Some(stop),
        thread: Some(thread),
    }
}

#[test]
fn test_typed_rows_read_every_route_the_tui_uses() {
    let api = served();
    let projects = api.projects().unwrap();
    assert_eq!(projects[0].slug, "o/p");
    assert_eq!(projects[0].skills["land"], "make land");
    assert_eq!(projects[0].kind("PK"), docket_core::word::Kind::Package);

    let status = api.status("o/p").unwrap();
    assert_eq!(status.count("ready"), 1);
    assert_eq!(status.count("parked"), 1);

    let next = api.next("o/p", 5, None).unwrap();
    assert_eq!(next[0].id, "T1");
    assert_eq!(next[0].priority, "high");
    assert_eq!(api.list("todo", "o/p").unwrap()[0].id, "Q1");
    assert_eq!(api.list("questions", "o/p").unwrap()[0].body, "Two ways");
    assert_eq!(api.search("o/p", "sync", 5).unwrap()[0].id, "T1");
    assert!(api.group("o/p", "none").unwrap().is_empty());

    let shown = api.show("o/p", "T1").unwrap();
    assert_eq!(shown.row.word, "ready");
    assert_eq!(shown.opened, ["PK1"]);
    assert_eq!(shown.cites[0].path.as_deref(), Some("src/sync.rs"));
    let package = api.show("o/p", "PK1").unwrap();
    assert_eq!(package.progress.map(|p| (p.done, p.total)), Some((0, 1)));
}

#[test]
fn test_stored_lists_read_items_links_and_events() {
    let api = served();
    let items = api.items("o/p").unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].tags, ["high"]);
    let opened = api.links_from(&[1, 2, 3], "opened").unwrap();
    assert_eq!((opened[0].rid, opened[0].to_rid), (1, Some(2)));
    assert_eq!(api.links_to(&[2], "opened").unwrap().len(), 1);
    let log = api.log(1).unwrap();
    assert_eq!(
        log.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
        ["opened", "edited"]
    );
    let recent = api.recent("o/p", &["opened", "asked"], 10).unwrap();
    assert_eq!(recent[0].kind, "asked");
}

#[test]
fn test_a_refusal_carries_the_servers_words() {
    let api = served();
    match api.show("o/p", "T99") {
        Err(Error::Refused(404, why)) => assert_eq!(why, "no item T99 in o/p"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_a_route_the_server_lacks_names_itself() {
    let api = served();
    match api.get::<serde_json::Value>("/no-such-route", &[]) {
        Err(Error::Refused(404, why)) => assert_eq!(
            why,
            "the server has no route /no-such-route; it may be older than this client"
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_changes_name_each_event_and_skip_comments() {
    let text = ": keep-alive\n\nevent: hello\ndata: 0\n\nevent: change\ndata: 1\n\ndata: x\n\n";
    let names: Vec<String> = Changes::over(Box::new(text.as_bytes())).collect();
    assert_eq!(names, ["hello", "change", "message"]);
}

#[test]
fn test_changes_from_the_server_start_with_hello() {
    let api = served();
    let mut stream = api.changes().unwrap();
    assert_eq!(stream.next().as_deref(), Some("hello"));
}

#[test]
fn test_facts_read_and_set_through_their_routes() {
    let api = served();
    let facts = api.facts("o/p").unwrap();
    assert_eq!(facts.skills["mode"], "run");
    assert!(
        !facts.skills.contains_key("land"),
        "a retired fact is left out"
    );
    let set = api.set_fact("o/p", "owner", "Ana").unwrap();
    assert_eq!(set.skills["owner"], "Ana");
    assert_eq!(api.facts("o/p").unwrap().skills["owner"], "Ana");
    match api.set_fact("o/p", "mode", "go") {
        Err(Error::Refused(409, why)) => {
            assert_eq!(why, "mode is one of run, drain, pause, not 'go'");
        }
        other => panic!("{other:?}"),
    }
}
