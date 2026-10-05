//! Scenario: a checkout of a public repository, and a docket server that knows the owner's private
//! project, name and machine.
//! Expected behaviour: `docket private check` reports every private name in the tracked files, the
//! staged change and a commit message, leaves out what the owner lists as public, and passes a clean
//! tree; the hooks it installs run it.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use common::{Server, serve};

const SEED: &str = r#"
INSERT INTO projects (slug, keys, skills, created_at, updated_at) VALUES
  ('acme/widgets', '[{"key":"T","kind":"work"},{"key":"A","kind":"audit"}]', '{"owner":"Ada Lovelace"}', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, tags, body, opened_at, updated_at)
  VALUES (1, 'acme/widgets', 'T', 1, 'The proofing timer drifts after every restart', 'open', 'agent', '[]', '', 'o', 'u');
INSERT INTO machines (name, ssh, slots, runners, note, updated_at)
  VALUES ('alpha', 'user@203.0.113.7', 2, '["claude"]', NULL, 'u');
"#;

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
        sh(&repo, "git init -q -b main");
        Checkout {
            _tmp: tmp,
            repo,
            config,
            server: serve(SEED, "alpha owner key-owner\nbeta agent key-agent"),
        }
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.repo.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
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
            .env("DOCKET_PROJECT", "acme/widgets")
            .output()
            .unwrap()
    }

    fn check(&self, args: &[&str]) -> Output {
        let mut all = vec!["-p", "acme/widgets", "private", "check"];
        all.extend(args);
        self.docket("key-owner", &all)
    }
}

fn sh(cwd: &Path, line: &str) -> String {
    let out = Command::new("sh")
        .arg("-c")
        .arg(line)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(out.status.success(), "{line}: {}", text(&out));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn test_check_reports_each_private_name_in_the_tracked_files() {
    let c = Checkout::new();
    c.write("src/a.rs", "fn main() {}\n// built on alpha for Ada\n");
    c.write(
        "notes.md",
        "Deploy to 203.0.113.7, see the widgets tracker.\n",
    );
    c.write("clean.rs", "fn nothing() {}\n");
    c.write("LICENSE", "Copyright (c) Ada Lovelace\n");
    sh(&c.repo, "git add -A");
    let out = c.check(&[]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let said = text(&out);
    for want in [
        "src/a.rs:2: alpha",
        "src/a.rs:2: Ada",
        "notes.md:1: 203.0.113.7",
        "notes.md:1: widgets",
    ] {
        assert!(said.contains(want), "{want} not in {said}");
    }
    assert!(!said.contains("clean.rs"), "{said}");
    assert!(!said.contains("LICENSE"), "{said}");
}

#[test]
fn test_a_name_listed_as_public_is_left_out() {
    let c = Checkout::new();
    fs::write(
        c.config.join("docket/public"),
        "widgets  # the product's public name\n",
    )
    .unwrap();
    c.write("notes.md", "the widgets tracker\n");
    sh(&c.repo, "git add -A");
    let out = c.check(&[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

#[test]
fn test_a_name_listed_as_private_is_found() {
    let c = Checkout::new();
    fs::write(
        c.config.join("docket/private"),
        "zeta  # another machine's alias for this one\n",
    )
    .unwrap();
    c.write("notes.md", "built on zeta\n");
    sh(&c.repo, "git add -A");
    let out = c.check(&[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("notes.md:1: zeta"), "{}", text(&out));
}

#[test]
fn test_a_clean_tree_passes_and_an_agent_key_is_refused() {
    let c = Checkout::new();
    c.write(
        "src/lib.rs",
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
    );
    sh(&c.repo, "git add -A");
    let out = c.check(&[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("no private name in 1 file"));
    let agent = c.docket("key-agent", &["-p", "acme/widgets", "private", "check"]);
    assert_ne!(agent.status.code(), Some(0));
}

#[test]
fn test_the_staged_change_and_the_message_are_checked_with_item_ids() {
    let c = Checkout::new();
    c.write("src/a.rs", "fn a() {}\n");
    sh(
        &c.repo,
        "git add -A && git -c user.name=t -c user.email=t@example.org commit -qm start",
    );
    c.write(
        "src/a.rs",
        "fn a() {}\n// as A3 decided\nfn b() {} // on alpha\n",
    );
    sh(&c.repo, "git add -A");
    let out = c.check(&["--staged", "--ids"]);
    assert_eq!(out.status.code(), Some(1));
    let said = text(&out);
    assert!(
        said.contains("src/a.rs:2: A3 (a docket item cited in a comment)"),
        "{said}"
    );
    assert!(said.contains("src/a.rs:3: alpha"), "{said}");
    c.write(
        "msg",
        "Fix the widgets sync\n# a comment line naming alpha\n",
    );
    let msg = c.check(&["--message", "msg"]);
    assert_eq!(msg.status.code(), Some(1));
    assert!(
        text(&msg).contains("commit message:1: widgets"),
        "{}",
        text(&msg)
    );
    assert!(!text(&msg).contains("alpha"), "{}", text(&msg));
}

#[test]
fn test_hook_installs_the_three_hooks_and_keeps_anothers() {
    let c = Checkout::new();
    let out = c.docket("key-owner", &["private", "hook"]);
    assert!(out.status.success(), "{}", text(&out));
    for h in ["pre-commit", "commit-msg", "pre-push"] {
        let body = fs::read_to_string(c.repo.join(".git/hooks").join(h)).unwrap();
        assert!(body.contains("docket private check"), "{h}: {body}");
    }
    fs::write(c.repo.join(".git/hooks/pre-push"), "#!/bin/sh\nmine\n").unwrap();
    let again = c.docket("key-owner", &["private", "hook"]);
    assert!(!again.status.success());
    assert!(text(&again).contains("--force"), "{}", text(&again));
}

#[test]
fn test_titles_and_private_addresses_are_found() {
    let c = Checkout::new();
    c.write(
        "notes.md",
        &format!(
            "Fixes: The proofing timer drifts after every restart.\nOn {}.168.4.5 only.\n",
            192
        ),
    );
    sh(&c.repo, "git add -A");
    let out = c.check(&[]);
    assert_eq!(out.status.code(), Some(1));
    let said = text(&out);
    assert!(
        said.contains(
            "notes.md:1: \"The proofing timer drifts after every restart\" (a docket item's title)"
        ),
        "{said}"
    );
    assert!(
        said.contains(&format!(
            "notes.md:2: {}.168.4.5 (a private address or a token)",
            192
        )),
        "{said}"
    );
}

#[test]
fn test_a_range_checks_every_commits_message_and_changes() {
    let c = Checkout::new();
    let commit = "git -c user.name=t -c user.email=t@example.org commit -qm";
    c.write("a.rs", "// runs on alpha\n");
    sh(
        &c.repo,
        &format!("git add -A && {commit} 'Sync the widgets'"),
    );
    c.write("a.rs", "// runs anywhere\n");
    sh(&c.repo, &format!("git add -A && {commit} 'Generalise'"));
    assert_eq!(c.check(&[]).status.code(), Some(0), "the tree is clean");
    let out = c.check(&["--range", "main"]);
    assert_eq!(out.status.code(), Some(1));
    let said = text(&out);
    assert!(said.contains(" message:1: widgets"), "{said}");
    assert!(said.contains(" a.rs:1: alpha"), "{said}");
    let newest = c.check(&["--range", "HEAD~1..HEAD"]);
    assert_eq!(newest.status.code(), Some(0), "{}", text(&newest));
    assert!(
        text(&newest).contains("no private name in 1 commit\n"),
        "{}",
        text(&newest)
    );
}

#[test]
fn test_a_branch_name_with_an_item_id_in_a_message_is_found() {
    let c = Checkout::new();
    let commit = "git -c user.name=t -c user.email=t@example.org commit -qm";
    c.write("a.rs", "fn main() {}\n");
    sh(
        &c.repo,
        &format!("git add -A && {commit} \"Merge branch 'lead/t14-123' into lead/batch\""),
    );
    let range = c.check(&["--range", "main"]);
    assert_eq!(range.status.code(), Some(1), "{}", text(&range));
    assert!(text(&range).contains("t14"), "{}", text(&range));
    c.write("msg", "Merge branch 'lead/t14-123' into lead/batch\n");
    let msg = c.check(&["--message", "msg"]);
    assert_eq!(msg.status.code(), Some(1), "{}", text(&msg));
    assert!(text(&msg).contains("t14"), "{}", text(&msg));
}
