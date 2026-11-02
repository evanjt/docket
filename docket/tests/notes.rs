//! Scenario: a note written to a verb that takes note text, given as `-` with the text on stdin, the
//! form `--body -` accepts.
//! Expected behaviour: `-` reads the note from stdin for every such verb; an empty stdin, or a note
//! that is only whitespace or `-` after trimming, is refused with a non-zero exit and nothing is
//! written.

mod common;

use std::fs;
use std::io::Write;
use std::process::{Output, Stdio};

use common::{Server, serve};

const SEED: &str = r"
INSERT INTO projects (slug, created_at, updated_at) VALUES ('acme/kiln', 'c', 'u');
INSERT INTO areas (id, project, name, position) VALUES (1, 'acme/kiln', 'firing', 1);
INSERT INTO items (rid, project, key, num, title, state, body, opened_at, updated_at, area_id)
  VALUES (1, 'acme/kiln', 'T', 1, 'The glaze kiln overshoots its cone on a slow ramp', 'open', 'Original body.', 'o', 'u', 1);
INSERT INTO items (rid, project, key, num, title, state, body, opened_at, updated_at, area_id)
  VALUES (2, 'acme/kiln', 'T', 2, 'The bisque shelf map forgets its last layout', 'open', '', 'o', 'u', 1);
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
            format!("{}\tacme/kiln\tbind\tt\n", repo.display()),
        )
        .unwrap();
        Checkout {
            _tmp: tmp,
            repo,
            config,
            server: serve(SEED, "alpha owner key-owner"),
        }
    }

    fn docket(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = common::docket_command()
            .args(args)
            .current_dir(&self.repo)
            .env("XDG_CONFIG_HOME", &self.config)
            .env("DOCKET_SERVER", format!("http://{}", self.server.addr))
            .env("DOCKET_KEY", "key-owner")
            .env("USER", "nobody-in-particular")
            .env("HOME", self.config.parent().unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn body(&self, id: &str) -> String {
        let out = self.docket(&["--json", "show", id], "");
        assert!(out.status.success(), "{}", text(&out));
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        v["body"].as_str().unwrap_or_default().to_owned()
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
fn test_append_dash_appends_the_note_read_from_stdin() {
    let c = Checkout::new();
    let note = "Cone 6 reached at 1222 degrees.\nThe ramp held for 40 minutes.\n";
    let out = c.docket(&["edit", "T1", "--append", "-"], note);
    assert!(out.status.success(), "{}", text(&out));
    let body = c.body("T1");
    assert!(body.contains("Cone 6 reached at 1222 degrees."), "{body}");
    assert!(body.contains("The ramp held for 40 minutes."), "{body}");
    assert!(!body.trim_end().ends_with("\n-"), "{body}");
}

#[test]
fn test_append_dash_with_empty_stdin_is_refused_and_writes_nothing() {
    let c = Checkout::new();
    let before = c.body("T1");
    let out = c.docket(&["edit", "T1", "--append", "-"], "");
    assert!(!out.status.success(), "{}", text(&out));
    assert_eq!(c.body("T1"), before);
}

#[test]
fn test_append_of_a_bare_dash_or_whitespace_is_refused() {
    let c = Checkout::new();
    let before = c.body("T1");
    for note in [" - ", "   ", "\n-\n"] {
        let out = c.docket(&["edit", "T1", "--append", note], "");
        assert!(!out.status.success(), "{note:?}: {}", text(&out));
    }
    let out = c.docket(&["edit", "T1", "--append", "-"], "  -  \n");
    assert!(!out.status.success(), "{}", text(&out));
    assert_eq!(c.body("T1"), before);
}

#[test]
fn test_drop_reason_dash_reads_stdin() {
    let c = Checkout::new();
    let out = c.docket(
        &["drop", "T2", "-"],
        "The shelf map moved to the loading sheet.\n",
    );
    assert!(out.status.success(), "{}", text(&out));
    let out = c.docket(&["--json", "show", "T2"], "");
    let shown = String::from_utf8_lossy(&out.stdout);
    assert!(
        shown.contains("The shelf map moved to the loading sheet."),
        "{shown}"
    );
}

#[test]
fn test_drop_reason_dash_with_empty_stdin_is_refused() {
    let c = Checkout::new();
    let out = c.docket(&["drop", "T2", "-"], "");
    assert!(!out.status.success(), "{}", text(&out));
    let out = c.docket(&["--json", "show", "T2"], "");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["state"], "open", "{v}");
}
