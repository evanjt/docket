use std::cell::RefCell;

use docket_core::dump::{EventDump, ItemDump, ProjectDump};
use serde_json::json;

use super::*;

/// Pages handed out in turn, with the cursor each was asked from.
struct Pages {
    pages: RefCell<Vec<Result<DumpPage, String>>>,
    asked: RefCell<Vec<i64>>,
}

impl Pages {
    fn new(pages: Vec<Result<DumpPage, String>>) -> Self {
        Self {
            pages: RefCell::new(pages),
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl Source for Pages {
    fn page(&self, since: i64) -> Result<DumpPage, String> {
        self.asked.borrow_mut().push(since);
        self.pages.borrow_mut().remove(0)
    }
}

fn project() -> ProjectDump {
    ProjectDump {
        slug: "o/p".into(),
        remotes: json!([]),
        cite_roots: json!([]),
        repos: json!([]),
        skills: json!({}),
        created_at: "c".into(),
        updated_at: "u".into(),
        ..ProjectDump::default()
    }
}

fn item(id: &str, state: &str) -> ItemDump {
    ItemDump {
        project: "o/p".into(),
        id: id.into(),
        title: format!("Item {id}"),
        state: state.into(),
        opened_at: "o".into(),
        updated_at: "u".into(),
        ..ItemDump::default()
    }
}

fn event(uid: &str, kind: &str, id: &str) -> EventDump {
    EventDump {
        project: "o/p".into(),
        uid: uid.into(),
        at: format!("2026-01-01T00:00:0{}Z", uid.len()),
        host: "devbox".into(),
        kind: kind.into(),
        item: Some(id.into()),
        ..EventDump::default()
    }
}

fn page(cursor: i64, full: bool, items: Vec<ItemDump>, events: Vec<EventDump>) -> DumpPage {
    DumpPage {
        cursor,
        full,
        projects: vec![project()],
        items,
        events,
    }
}

fn checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "Dump"],
        &["config", "user.email", "dump@example.com"],
        &["config", "commit.gpgsign", "false"],
        &["config", "tag.gpgsign", "false"],
    ] {
        git::git(dir.path(), args).unwrap();
    }
    dir
}

#[test]
fn test_pass_without_cursor_writes_every_row_then_follows_the_cursor() {
    let repo = checkout();
    let source = Pages::new(vec![
        Ok(page(
            1,
            true,
            vec![item("T1", "open")],
            vec![event("a", "opened", "T1")],
        )),
        Ok(page(
            2,
            false,
            vec![item("T1", "done")],
            vec![event("bb", "closed", "T1")],
        )),
        Ok(page(2, false, vec![], vec![])),
    ]);
    let first = run_pass(&source, &repo, false);
    assert_eq!(first.commit.unwrap().1, "Sync dump (testhost)");
    assert_eq!(first.written, 3);
    let second = run_pass(&source, &repo, false);
    assert_eq!(second.commit.unwrap().1, "Close T1");
    assert_eq!(second.written, 2);
    assert_eq!(run_pass(&source, &repo, false).commit, None);
    assert_eq!(*source.asked.borrow(), vec![0, 1, 2]);
    let log = tree::read(repo.path(), "o/p/events.jsonl").unwrap();
    assert_eq!(log.lines().count(), 2);
    assert!(
        tree::read(repo.path(), "o/p/items/T1.md")
            .unwrap()
            .contains("state: \"done\"")
    );
}

#[test]
fn test_pass_that_fails_leaves_the_cursor_and_the_next_takes_it_whole() {
    let repo = checkout();
    git::set_cursor(repo.path(), 5).unwrap();
    let source = Pages::new(vec![
        Err("server down".into()),
        Ok(page(
            6,
            false,
            vec![item("T1", "open")],
            vec![event("a", "opened", "T1")],
        )),
    ]);
    assert!(pass(&source, repo.path(), false, "testhost").is_err());
    assert_eq!(git::cursor(repo.path()).unwrap(), Some(5));
    let report = run_pass(&source, &repo, false);
    assert_eq!(report.commit.unwrap().1, "Open T1");
    assert_eq!(*source.asked.borrow(), vec![5, 5]);
    assert_eq!(git::cursor(repo.path()).unwrap(), Some(6));
}

#[test]
fn test_pass_commits_files_an_earlier_pass_wrote_but_did_not_commit() {
    let repo = checkout();
    git::set_cursor(repo.path(), 1).unwrap();
    let written = page(
        2,
        false,
        vec![item("T1", "open")],
        vec![event("a", "opened", "T1")],
    );
    for (path, text) in files(&written, |_| None) {
        tree::write(repo.path(), &path, &text).unwrap();
    }
    let source = Pages::new(vec![Ok(written)]);
    let report = run_pass(&source, &repo, false);
    assert_eq!(report.written, 0);
    assert_eq!(report.commit.unwrap().1, "Open T1");
}

#[test]
fn test_pass_full_asks_from_zero_whatever_the_cursor() {
    let repo = checkout();
    git::set_cursor(repo.path(), 9).unwrap();
    let source = Pages::new(vec![Ok(page(9, true, vec![], vec![]))]);
    run_pass(&source, &repo, true);
    assert_eq!(*source.asked.borrow(), vec![0]);
}

fn run_pass(source: &Pages, repo: &tempfile::TempDir, full: bool) -> Report {
    pass(source, repo.path(), full, "testhost").unwrap()
}

/// A page that renames the project `o/p` to `o/q`: the project row, its item and the event under
/// the new slug.
fn rename_page(cursor: i64, item_state: &str) -> DumpPage {
    let renamed = EventDump {
        project: "o/q".into(),
        uid: "rename".into(),
        at: "2026-01-02T00:00:00Z".into(),
        host: "devbox".into(),
        kind: "renamed".into(),
        note: Some("o/p -> o/q".into()),
        data: Some(json!({ "old": "o/p", "new": "o/q" }).to_string()),
        ..EventDump::default()
    };
    DumpPage {
        cursor,
        full: false,
        projects: vec![ProjectDump {
            slug: "o/q".into(),
            ..project()
        }],
        items: vec![ItemDump {
            project: "o/q".into(),
            ..item("T1", item_state)
        }],
        events: vec![renamed],
    }
}

#[test]
fn test_a_rename_moves_the_project_directory_so_its_history_stays_whole() {
    let repo = checkout();
    let source = Pages::new(vec![
        Ok(page(
            1,
            true,
            vec![item("T1", "open")],
            vec![event("a", "opened", "T1")],
        )),
        Ok(rename_page(2, "open")),
    ]);
    run_pass(&source, &repo, false);
    assert!(repo.path().join("o/p/events.jsonl").is_file());
    let report = run_pass(&source, &repo, false);
    assert!(report.commit.is_some());
    assert!(!repo.path().join("o/p").exists());
    let log = tree::read(repo.path(), "o/q/events.jsonl").unwrap();
    assert_eq!(log.lines().count(), 2, "{log}");
    assert!(
        log.contains("\"opened\"") && log.contains("\"renamed\""),
        "{log}"
    );
    assert!(repo.path().join("o/q/items/T1.md").is_file());
    assert_eq!(tree::projects(repo.path()).unwrap(), ["o/q"]);
    let tracked = git::git(repo.path(), &["ls-files"]).unwrap();
    assert!(
        tracked.contains("o/q/events.jsonl") && !tracked.contains("o/p/"),
        "{tracked}"
    );
    assert_eq!(
        git::git(repo.path(), &["status", "--porcelain"]).unwrap(),
        ""
    );
}

#[test]
fn test_a_rename_found_after_the_new_directory_exists_merges_the_old_log_and_removes_the_rest() {
    let repo = checkout();
    let source = Pages::new(vec![
        Ok(page(
            1,
            true,
            vec![item("T1", "open")],
            vec![event("a", "opened", "T1")],
        )),
        Ok(rename_page(2, "done")),
    ]);
    run_pass(&source, &repo, false);
    tree::write(
        repo.path(),
        "o/q/events.jsonl",
        "{\"uid\":\"later\",\"at\":\"2026-01-03T00:00:00Z\",\"kind\":\"closed\"}\n",
    )
    .unwrap();
    tree::write(repo.path(), "o/q/project.json", "{}\n").unwrap();
    run_pass(&source, &repo, false);
    assert!(!repo.path().join("o/p").exists());
    let log = tree::read(repo.path(), "o/q/events.jsonl").unwrap();
    let kinds: Vec<&str> = log
        .lines()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .map(|k| Box::leak(k.into_boxed_str()) as &str)
        .collect();
    assert_eq!(kinds, ["opened", "renamed", "closed"], "{log}");
    assert_eq!(
        git::git(repo.path(), &["status", "--porcelain"]).unwrap(),
        ""
    );
}
