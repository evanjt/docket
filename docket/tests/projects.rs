//! Scenario: a checkout bound to a project whose slug the owner wants changed, and a server that
//! holds the project.
//! Expected behaviour: `docket projects rename OLD NEW` moves the project on the server, rewrites the
//! binding in this machine's roots, prints what is left to do by hand, and run again once the server
//! already knows the new slug it rewrites the roots only.

mod common;

use std::fs;
use std::process::{Command, Output};

use common::{Server, serve};

const SEED: &str = r"
INSERT INTO projects (slug, created_at, updated_at) VALUES ('acme/widgets', 'c', 'u');
INSERT INTO areas (id, project, name, position) VALUES (1, 'acme/widgets', 'general', 1);
INSERT INTO items (rid, project, key, num, title, state, body, opened_at, updated_at, area_id)
  VALUES (1, 'acme/widgets', 'T', 1, 'The proofing timer drifts after every restart', 'open', '', 'o', 'u', 1);
";

struct Checkout {
    _tmp: tempfile::TempDir,
    repo: std::path::PathBuf,
    config: std::path::PathBuf,
    server: Server,
}

impl Checkout {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let config = tmp.path().join("config");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(config.join("docket")).unwrap();
        fs::write(
            config.join("docket/roots"),
            format!("{}\tacme/widgets\tbind\tt\n", repo.display()),
        )
        .unwrap();
        Checkout {
            _tmp: tmp,
            repo,
            config,
            server: serve(SEED, "alpha owner key-owner\nbeta agent key-agent"),
        }
    }

    fn docket(&self, key: &str, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_docket"))
            .args(args)
            .current_dir(&self.repo)
            .env("XDG_CONFIG_HOME", &self.config)
            .env("DOCKET_SERVER", format!("http://{}", self.server.addr))
            .env("DOCKET_KEY", key)
            .env("USER", "nobody-in-particular")
            .env("HOME", self.config.parent().unwrap())
            .output()
            .unwrap()
    }

    fn roots(&self) -> String {
        fs::read_to_string(self.config.join("docket/roots")).unwrap()
    }
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn test_rename_moves_the_project_and_rewrites_this_machines_roots() {
    let c = Checkout::new();
    let before = c.docket("key-owner", &["status"]);
    assert!(before.status.success(), "{}", text(&before));
    assert!(text(&before).contains("acme/widgets"), "{}", text(&before));

    let dry = c.docket(
        "key-owner",
        &[
            "projects",
            "rename",
            "acme/widgets",
            "acme/gadgets",
            "--dry-run",
        ],
    );
    assert!(dry.status.success(), "{}", text(&dry));
    assert!(
        text(&dry).contains("would move acme/widgets -> acme/gadgets"),
        "{}",
        text(&dry)
    );
    assert!(c.roots().contains("\tacme/widgets\t"), "{}", c.roots());

    let out = c.docket(
        "key-owner",
        &["projects", "rename", "acme/widgets", "acme/gadgets"],
    );
    assert!(out.status.success(), "{}", text(&out));
    let said = text(&out);
    assert!(
        said.contains("moved acme/widgets -> acme/gadgets"),
        "{said}"
    );
    assert!(said.contains("1 root on this machine rewritten"), "{said}");
    assert!(said.contains("Left to do by hand"), "{said}");
    assert!(
        c.roots().contains("\tacme/gadgets\tbind\tt"),
        "{}",
        c.roots()
    );
    assert!(!c.roots().contains("acme/widgets"), "{}", c.roots());

    let after = c.docket("key-owner", &["status"]);
    assert!(after.status.success(), "{}", text(&after));
    assert!(text(&after).contains("acme/gadgets"), "{}", text(&after));
    let listed = c.docket("key-owner", &["projects"]);
    let listed = text(&listed);
    assert!(
        listed.contains("acme/gadgets") && !listed.contains("acme/widgets"),
        "{listed}"
    );
    let shown = c.docket("key-owner", &["-p", "acme/gadgets", "show", "T1"]);
    assert!(shown.status.success(), "{}", text(&shown));
}

#[test]
fn test_rename_run_again_where_the_server_already_knows_the_new_slug_rewrites_roots_only() {
    let c = Checkout::new();
    let first = c.docket(
        "key-owner",
        &["projects", "rename", "acme/widgets", "acme/gadgets"],
    );
    assert!(first.status.success(), "{}", text(&first));
    fs::write(
        c.config.join("docket/roots"),
        format!("{}\tacme/widgets\tbind\tt\n", c.repo.display()),
    )
    .unwrap();
    let again = c.docket(
        "key-owner",
        &["projects", "rename", "acme/widgets", "acme/gadgets"],
    );
    assert!(again.status.success(), "{}", text(&again));
    assert!(
        text(&again).contains("already the slug on the server"),
        "{}",
        text(&again)
    );
    assert!(c.roots().contains("\tacme/gadgets\t"), "{}", c.roots());
}

#[test]
fn test_rename_is_refused_on_an_agents_key_and_changes_nothing() {
    let c = Checkout::new();
    let out = c.docket(
        "key-agent",
        &["projects", "rename", "acme/widgets", "acme/gadgets"],
    );
    assert!(!out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("refused on an agent's key"),
        "{}",
        text(&out)
    );
    assert!(c.roots().contains("\tacme/widgets\t"), "{}", c.roots());
}
