use std::io::{Read, Write};
use std::net::SocketAddr;
use std::sync::mpsc;

use docket_migration::scratch::Scratch;
use docket_server::app;
use docket_server::auth::Keys;

use super::*;

const SEED: &str = r#"
INSERT INTO projects (slug, skills, created_at, updated_at) VALUES ('o/p', '{"mode":"run","land":"make land"}', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, body, opened_at, updated_at) VALUES
  (1, 'o/p', 'T', 1, 'Fix the sync', 'open', 'Body naming PK1', '2026-01-01T00:00:00Z', 'u1'),
  (2, 'o/p', 'PK', 1, 'Sync package', 'open', '', '2026-01-01T00:00:00Z', 'u2'),
  (3, 'o/p', 'Q', 1, 'Which way', 'open', 'Two ways', '2026-01-01T00:00:00Z', 'u3');
UPDATE items SET parent_rid=2, priority='high' WHERE rid=1;
UPDATE items SET type='plan' WHERE key='PK';
UPDATE items SET type='question' WHERE key='Q';
INSERT INTO links (rid, kind, to_rid) VALUES (1, 'origin', 3);
INSERT INTO links (rid, kind, to_path, to_line) VALUES (1, 'cites_file', 'src/sync.rs', 4);
INSERT INTO events (uid, project, rid, at, host, kind, note) VALUES
  ('e1', 'o/p', 1, '2026-01-01T00:00:00Z', 'devbox', 'opened', NULL),
  ('e2', 'o/p', 1, '2026-01-01T00:01:00Z', 'devbox', 'edited', 'title'),
  ('e3', 'o/p', 3, '2026-01-01T00:02:00Z', 'devbox', 'asked', 'pick one');
INSERT INTO assignments (rid, assignee, kind, started_at, host, note) VALUES
  (3, 'owner', 'ask', '2026-01-01T00:02:00Z', 'devbox', 'pick one');
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

    let status = api.status("o/p").unwrap();
    assert_eq!(status.count("ready"), 1);
    assert_eq!(status.count("waiting on owner"), 1);

    let next = api.next("o/p", 5, &[]).unwrap();
    assert_eq!(next[0].id, "T1");
    assert_eq!(next[0].priority, "high");
    assert_eq!(api.list("todo", "o/p").unwrap()[0].id, "Q1");
    assert_eq!(api.list("questions", "o/p").unwrap()[0].id, "Q1");
    assert_eq!(api.search("o/p", "sync", 5).unwrap()[0].id, "T1");
    assert!(api.group("o/p", "none").unwrap().is_empty());

    let shown = api.show("o/p", "T1").unwrap();
    assert_eq!(shown.row.word, "ready");
    assert_eq!(shown.parent.as_deref(), Some("PK1"));
    assert_eq!(shown.origin, ["Q1"]);
    assert_eq!(shown.cites[0].path.as_deref(), Some("src/sync.rs"));
}

#[test]
fn test_stored_lists_read_items_links_and_events() {
    let api = served();
    let items = api.items("o/p").unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].parent_rid, Some(2));
    assert_eq!(
        (items[2].turn.as_deref(), items[2].turn_note.as_deref()),
        (Some("user"), Some("pick one"))
    );
    assert_eq!(items[0].turn.as_deref(), Some("agent"));
    let origin = api.links_from(&[1, 2, 3], "origin").unwrap();
    assert_eq!((origin[0].rid, origin[0].to_rid), (1, Some(3)));
    assert_eq!(api.links_to(&[3], "origin").unwrap().len(), 1);
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
    let names: Vec<String> = Changes::over(Box::new(text.as_bytes()))
        .map(|e| e.name)
        .collect();
    assert_eq!(names, ["hello", "change", "message"]);
}

#[test]
fn test_changes_carry_their_data() {
    let text = "event: change\ndata: {\"n\":1,\"projects\":[\"o/q\"]}\n\n";
    let event = Changes::over(Box::new(text.as_bytes())).next().unwrap();
    assert_eq!(event.data, "{\"n\":1,\"projects\":[\"o/q\"]}");
}

#[test]
fn test_a_change_naming_other_projects_is_not_news_to_the_shown_one() {
    let event = |data: &str| Event {
        name: "change".into(),
        data: data.into(),
    };
    let other = event("{\"n\":1,\"projects\":[\"o/q\"]}");
    assert!(!other.concerns(Some("o/p")));
    assert!(other.concerns(Some("o/q")));
    assert!(other.concerns(None));
    assert!(event("{\"n\":1,\"projects\":[]}").concerns(Some("o/p")));
    assert!(event("1").concerns(Some("o/p")));
}

#[test]
fn test_changes_from_the_server_start_with_hello() {
    let api = served();
    let mut stream = api.changes().unwrap();
    assert_eq!(stream.next().map(|e| e.name).as_deref(), Some("hello"));
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
    let set = api.set_fact("o/p", "owner", "Ana", false).unwrap();
    assert_eq!(set.skills["owner"], "Ana");
    assert_eq!(api.facts("o/p").unwrap().skills["owner"], "Ana");
    match api.set_fact("o/p", "mode", "go", false) {
        Err(Error::Refused(409, why)) => {
            assert_eq!(why, "mode is one of run, drain, pause, not 'go'");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_machines_and_the_lead_claim_round_trip() {
    let api = served();
    assert!(api.machines().unwrap().machines.is_empty());
    let req = MachineRequest {
        set: docket_core::machine::Set {
            name: "alpha".into(),
            ssh: Some("alpha.example".into()),
            slots: Some(2),
            runners: Some(vec!["claude".into()]),
            note: None,
            path: None,
        },
        remove: false,
    };
    assert_eq!(api.set_machine(&req).unwrap().machines[0].slots, 2);
    assert_eq!(api.machines().unwrap().machines[0].name, "alpha");

    assert!(api.lead("o/p").unwrap().lead.is_none());
    let took = api.lead_act("o/p", Some("main"), "take", "lead-1").unwrap();
    assert_eq!(took.outcome.as_deref(), Some("took"));
    assert_eq!(api.lead("o/p").unwrap().lead.unwrap().session, "lead-1");
    let gave = api.lead_act("o/p", None, "give", "lead-1").unwrap();
    assert!(gave.lead.is_none());
    assert!(matches!(
        api.lead_act("o/p", None, "renew", "lead-1"),
        Err(Error::Refused(409, _))
    ));
}

#[test]
fn test_a_publication_round_trips_and_a_second_with_its_sha_is_refused() {
    let api = served();
    assert!(api.publications("o/p").unwrap().publications.is_empty());
    let req = PublicationRequest {
        common: Common {
            project: "o/p".into(),
            ..Common::default()
        },
        published: "a".repeat(40),
        work: "b".repeat(40),
        plans: vec!["PK1".into()],
    };
    let recorded = api.record_publication(&req).unwrap();
    assert_eq!(recorded.publications[0].plans, ["PK1"]);
    assert_eq!(api.publications("o/p").unwrap(), recorded);
    assert!(matches!(
        api.record_publication(&req),
        Err(Error::Refused(409, _))
    ));
}

#[derive(serde::Deserialize, Debug)]
struct Needs {
    #[allow(dead_code)]
    missing: String,
}

#[test]
fn test_a_landed_write_with_an_unreadable_answer_is_not_a_network_failure() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let thread = std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = sock.read(&mut buf).unwrap();
        let body = r#"{"id":"T1"}"#;
        write!(
            sock,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let api = Api::new(&Config {
        server: format!("http://{addr}"),
        key: "k".into(),
    })
    .unwrap();
    let e = api.post::<Needs>("new", &json!({})).unwrap_err();
    thread.join().unwrap();
    assert!(matches!(e, Error::Unreadable(_)), "{e:?}");
}
