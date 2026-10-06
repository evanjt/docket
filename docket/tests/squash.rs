//! Scenario: a work branch where two plans landed as interleaved merges, a remote ref to publish on
//! top of, and a project that names its published ref.
//! Expected behaviour: `docket squash` proposes one commit per cut point, builds the published ref
//! only from messages that pass the private guard, leaves the work branch and every close sha alone,
//! records what it wrote and never pushes.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use common::{Server, serve};

struct Repo {
    _tmp: tempfile::TempDir,
    dir: std::path::PathBuf,
    config: std::path::PathBuf,
    server: Server,
    /// The remote ref the first published commit sits on.
    base: String,
    /// The first-parent merges of the work branch, oldest first.
    merges: Vec<String>,
}

fn sh(cwd: &Path, line: &str) -> String {
    let out = Command::new("sh")
        .arg("-c")
        .arg(line)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{line}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// One job branch off `main` with a file, merged back at `at` with a merge commit.
fn land(dir: &Path, name: &str, at: &str) -> String {
    sh(dir, &format!("git checkout -q -b {name} main"));
    fs::write(dir.join(format!("{name}.txt")), name).unwrap();
    sh(
        dir,
        &format!("git add -A && git commit -q -m 'add {name}' && git checkout -q main"),
    );
    sh(
        dir,
        &format!(
            "GIT_COMMITTER_DATE={at} GIT_AUTHOR_DATE={at} git merge -q --no-ff -m 'merge {name}' {name}"
        ),
    );
    sh(dir, &format!("git rev-parse {name}"))
}

impl Repo {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("kiln");
        let config = tmp.path().join("config");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(config.join("docket")).unwrap();
        sh(&dir, "git init -q -b main");
        sh(&dir, "git config user.name 'Kiln Keeper'");
        sh(&dir, "git config user.email keeper@example.com");
        fs::write(dir.join("README"), "kiln").unwrap();
        sh(&dir, "git add -A && git commit -q -m start");
        let base = sh(&dir, "git rev-parse HEAD");
        sh(
            &dir,
            &format!("git update-ref refs/remotes/origin/main {base}"),
        );
        let closes: Vec<String> = [
            ("t1", "2026-02-01T10:00:00+00:00"),
            ("t2", "2026-02-02T10:00:00+00:00"),
            ("t3", "2026-02-03T10:00:00+00:00"),
            ("t4", "2026-02-04T10:00:00+00:00"),
        ]
        .iter()
        .map(|(n, at)| land(&dir, n, at))
        .collect();
        let merges = sh(
            &dir,
            "git rev-list --first-parent --reverse origin/main..main",
        )
        .lines()
        .map(str::to_string)
        .collect();
        let seed = format!(
            r#"
INSERT INTO projects (slug, skills, integration_ref, created_at, updated_at) VALUES
  ('acme/kiln',
   '{{"owner":"Ada Lovelace","publish":"shelf origin/main"}}', 'main', 'c', 'u');
INSERT INTO areas (id, project, name, description, position, priority) VALUES (1, 'acme/kiln', 'firing', '', 1, NULL);
INSERT INTO items (rid, project, key, num, title, state, body, type, opened_at, updated_at, resolution, parent_rid, area_id) VALUES
  (1, 'acme/kiln', 'A', 1, 'Fire the first bowls', 'done', '', 'plan', 'o', 'u', 'fired', NULL, 1),
  (2, 'acme/kiln', 'A', 2, 'Glaze the platters', 'done', '', 'plan', 'o', 'u', 'glazed', NULL, 1),
  (3, 'acme/kiln', 'T', 1, 'Stack the firewood', 'done', '', 'task', 'o', 'u', 'stacked at {}', 1, 1),
  (4, 'acme/kiln', 'T', 2, 'Mix the slip', 'done', '', 'task', 'o', 'u', 'mixed at {}', 2, 1),
  (5, 'acme/kiln', 'T', 3, 'Light the burner', 'done', '', 'task', 'o', 'u', 'lit at {}', 1, 1),
  (6, 'acme/kiln', 'T', 4, 'Dip the platters', 'done', '', 'task', 'o', 'u', 'dipped at {}', 2, 1);
"#,
            closes[0], closes[1], closes[2], closes[3]
        );
        let server = serve(
            Box::leak(seed.into_boxed_str()),
            "alpha owner key-owner\nbeta agent key-agent",
        );
        Repo {
            _tmp: tmp,
            dir,
            config,
            server,
            base,
            merges,
        }
    }

    fn squash(&self, args: &[&str]) -> Output {
        squash(&self.dir, &self.config, &self.server, args)
    }

    fn messages(&self, lines: &str) -> String {
        messages(&self.dir, lines)
    }

    fn published(&self) -> Option<String> {
        published(&self.dir)
    }
}

fn squash(dir: &Path, config: &Path, server: &Server, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_docket"))
        .args(["-p", "acme/kiln", "squash"])
        .args(args)
        .current_dir(dir)
        .env("XDG_CONFIG_HOME", config)
        .env("DOCKET_SERVER", format!("http://{}", server.addr))
        .env("DOCKET_KEY", "key-owner")
        .env("USER", "nobody-in-particular")
        .env("HOME", config.parent().unwrap())
        .env("DOCKET_PROJECT", "acme/kiln")
        .output()
        .unwrap()
}

fn messages(dir: &Path, lines: &str) -> String {
    let path = dir.join("../messages.txt");
    fs::write(&path, lines).unwrap();
    path.to_string_lossy().into_owned()
}

fn published(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "refs/heads/shelf"])
        .current_dir(dir)
        .output()
        .unwrap();
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[test]
fn test_without_messages_each_proposed_commit_is_printed_and_nothing_is_written() {
    let r = Repo::new();
    let tip = sh(&r.dir, "git rev-parse main");
    let out = r.squash(&[]);
    assert!(out.status.success(), "{}", text(&out));
    let said = text(&out);
    // The plans finish at the third and fourth merges.
    assert!(said.contains(&r.merges[2]), "{said}");
    assert!(said.contains(&r.merges[3]), "{said}");
    assert!(!said.contains(&r.merges[1]), "{said}");
    for want in ["A1", "A2", "Stack the firewood", "Dip the platters"] {
        assert!(said.contains(want), "{want}: {said}");
    }
    assert_eq!(r.published(), None);
    assert_eq!(sh(&r.dir, "git rev-parse main"), tip);
}

#[test]
fn test_two_messages_write_two_commits_on_the_snapshot_trees_and_leave_the_work_branch_alone() {
    let r = Repo::new();
    let tip = sh(&r.dir, "git rev-parse main");
    let closes = sh(&r.dir, "git rev-list main");
    let file = r.messages("Fire the first bowls\nGlaze the platters\n");
    let out = r.squash(&["--messages", &file]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(sh(&r.dir, "git rev-parse main"), tip);
    assert_eq!(sh(&r.dir, "git rev-list main"), closes);

    let published = r.published().expect("the published ref is written");
    let chain = sh(&r.dir, "git rev-list --reverse origin/main..shelf");
    let chain: Vec<&str> = chain.lines().collect();
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[1], published);
    assert_eq!(sh(&r.dir, &format!("git rev-parse {}^", chain[0])), r.base);
    assert_eq!(
        sh(&r.dir, &format!("git rev-parse {}^", chain[1])),
        chain[0]
    );
    for (commit, snapshot) in chain.iter().zip([&r.merges[2], &r.merges[3]]) {
        assert_eq!(
            sh(&r.dir, &format!("git rev-parse {commit}^{{tree}}")),
            sh(&r.dir, &format!("git rev-parse {snapshot}^{{tree}}"))
        );
        for field in ["%cI", "%aI"] {
            assert_eq!(
                sh(&r.dir, &format!("git log -1 --format={field} {commit}")),
                sh(&r.dir, &format!("git log -1 --format=%cI {snapshot}"))
            );
        }
        assert_eq!(
            sh(&r.dir, &format!("git log -1 --format='%an <%ae>' {commit}")),
            "Kiln Keeper <keeper@example.com>"
        );
    }
    assert_eq!(
        sh(&r.dir, &format!("git log -1 --format=%s {}", chain[0])),
        "Fire the first bowls"
    );
    let said = text(&out);
    assert!(said.contains("git push origin shelf:main"), "{said}");

    // The publications are recorded: the next squash starts after them.
    let again = r.squash(&[]);
    assert!(again.status.success(), "{}", text(&again));
    assert!(
        text(&again).contains("nothing to publish"),
        "{}",
        text(&again)
    );
}

#[test]
fn test_a_message_citing_an_item_or_a_job_branch_is_refused_and_nothing_is_written() {
    let r = Repo::new();
    for cited in ["Close T1 for good", "Merge lead/t1-123 in"] {
        let file = r.messages(&format!("{cited}\nGlaze the platters\n"));
        let out = r.squash(&["--messages", &file]);
        assert!(!out.status.success(), "{}", text(&out));
        assert!(text(&out).contains("message 1"), "{}", text(&out));
        assert_eq!(r.published(), None);
    }
}

#[test]
fn test_a_message_naming_an_agent_tool_is_refused_and_nothing_is_written() {
    let r = Repo::new();
    let file = r.messages("Fire the first bowls, generated with Claude\nGlaze the platters\n");
    let out = r.squash(&["--messages", &file]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("message 1"), "{}", text(&out));
    assert_eq!(r.published(), None);
}

#[test]
fn test_a_published_diff_adding_an_agent_instructions_file_is_refused_and_nothing_is_written() {
    let r = Repo::new();
    sh(&r.dir, "git checkout -q -b t5 main");
    fs::write(r.dir.join("AGENTS.md"), "rules\n").unwrap();
    sh(
        &r.dir,
        "git add -A && git commit -q -m notes && git checkout -q main",
    );
    sh(&r.dir, "git merge -q --no-ff -m 'merge t5' t5");
    let file = r.messages("Fire the first bowls\nGlaze the platters\nAdd the notes\n");
    let out = r.squash(&["--messages", &file]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("AGENTS.md"), "{}", text(&out));
    assert_eq!(r.published(), None);
}

#[test]
fn test_a_message_count_that_differs_from_the_commit_count_is_refused() {
    let r = Repo::new();
    let file = r.messages("Fire the first bowls\n");
    let out = r.squash(&["--messages", &file]);
    assert!(!out.status.success(), "{}", text(&out));
    assert_eq!(r.published(), None);
}

#[test]
fn test_a_published_diff_carrying_a_private_name_is_refused_and_nothing_is_written() {
    let r = Repo::new();
    // The owner's name is private; a file the job branch added carries it.
    sh(&r.dir, "git checkout -q -b t5 main");
    fs::write(r.dir.join("notes.txt"), "kept by Ada Lovelace\n").unwrap();
    sh(
        &r.dir,
        "git add -A && git commit -q -m notes && git checkout -q main",
    );
    sh(&r.dir, "git merge -q --no-ff -m 'merge t5' t5");
    let file = r.messages("Fire the first bowls\nGlaze the platters\nAdd the notes\n");
    let out = r.squash(&["--messages", &file]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("Ada"), "{}", text(&out));
    assert_eq!(r.published(), None);
}

#[test]
fn test_a_work_branch_rewritten_since_the_last_publication_is_refused() {
    let r = Repo::new();
    let file = r.messages("Fire the first bowls\nGlaze the platters\n");
    assert!(r.squash(&["--messages", &file]).status.success());
    let published = r.published().unwrap();
    // The recorded snapshot leaves the work branch.
    sh(&r.dir, &format!("git checkout -q -B main {}", r.base));
    fs::write(r.dir.join("later.txt"), "later").unwrap();
    sh(&r.dir, "git add -A && git commit -q -m later");
    let out = r.squash(&["--messages", &file]);
    assert!(!out.status.success(), "{}", text(&out));
    assert_eq!(r.published(), Some(published));
}

/// A work branch whose two landings each move a submodule, named in the project's repos, to a
/// commit of its own work history.
struct Nested {
    _tmp: tempfile::TempDir,
    dir: std::path::PathBuf,
    glaze: std::path::PathBuf,
    config: std::path::PathBuf,
    server: Server,
    /// The submodule commits the two landings pin, oldest first.
    pins: Vec<String>,
}

const GLAZE: &str = "vendor/glaze";

/// One job branch off `main` that commits `file` in the submodule and pins it, merged back at `at`.
fn land_inside(dir: &Path, name: &str, at: &str) -> String {
    let glaze = dir.join(GLAZE);
    fs::write(glaze.join(format!("{name}.txt")), name).unwrap();
    sh(
        &glaze,
        &format!("git add -A && git commit -q -m 'add {name}'"),
    );
    let pin = sh(&glaze, "git rev-parse HEAD");
    sh(dir, &format!("git checkout -q -b {name} main"));
    sh(
        dir,
        &format!("git add {GLAZE} && git commit -q -m 'pin {name}' && git checkout -q main"),
    );
    sh(
        dir,
        &format!(
            "GIT_COMMITTER_DATE={at} GIT_AUTHOR_DATE={at} git merge -q --no-ff -m 'merge {name}' {name}"
        ),
    );
    pin
}

impl Nested {
    fn new(fake: bool) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("kiln");
        let glaze = dir.join(GLAZE);
        let config = tmp.path().join("config");
        fs::create_dir_all(&glaze).unwrap();
        fs::create_dir_all(config.join("docket")).unwrap();
        for d in [&dir, &glaze] {
            sh(d, "git init -q -b main");
            sh(d, "git config user.name 'Kiln Keeper'");
            sh(d, "git config user.email keeper@example.com");
        }
        fs::write(glaze.join("README"), "glaze").unwrap();
        sh(&glaze, "git add -A && git commit -q -m start");
        sh(&glaze, "git update-ref refs/remotes/origin/main HEAD");
        fs::write(dir.join("README"), "kiln").unwrap();
        sh(&dir, "git add -A && git commit -q -m start");
        sh(&dir, "git update-ref refs/remotes/origin/main HEAD");
        let mut closes = Vec::new();
        let mut pins = Vec::new();
        for (n, at) in [
            ("t1", "2026-02-01T10:00:00+00:00"),
            ("t2", "2026-02-02T10:00:00+00:00"),
        ] {
            pins.push(land_inside(&dir, n, at));
            closes.push(sh(&dir, &format!("git rev-parse {n}")));
        }
        if fake {
            sh(&dir, "git checkout -q -b t3 main");
            sh(
                &dir,
                &format!(
                    "git update-index --cacheinfo 160000,{},{GLAZE} && git commit -q -m 'pin t3' && git checkout -q main",
                    "0123456789abcdef0123456789abcdef01234567"
                ),
            );
            sh(&dir, "git merge -q --no-ff -m 'merge t3' t3");
            closes.push(sh(&dir, "git rev-parse t3"));
        }
        let cooled = closes.get(2).map_or(String::new(), |close| {
            format!(
                "INSERT INTO items (rid, project, key, num, title, state, body, type, opened_at, updated_at, resolution, parent_rid, area_id) VALUES
  (5, 'acme/kiln', 'A', 3, 'Cool the oven', 'done', '', 'plan', 'o', 'u', 'cooled', NULL, 1),
  (6, 'acme/kiln', 'T', 3, 'Open the vents', 'done', '', 'task', 'o', 'u', 'opened at {close}', 5, 1);"
            )
        });
        let seed = format!(
            r#"
INSERT INTO projects (slug, skills, integration_ref, repos, created_at, updated_at) VALUES
  ('acme/kiln',
   '{{"owner":"Ada Lovelace","publish":"shelf origin/main"}}', 'main', '[".", "{GLAZE}"]', 'c', 'u');
INSERT INTO areas (id, project, name, description, position, priority) VALUES (1, 'acme/kiln', 'firing', '', 1, NULL);
INSERT INTO items (rid, project, key, num, title, state, body, type, opened_at, updated_at, resolution, parent_rid, area_id) VALUES
  (1, 'acme/kiln', 'A', 1, 'Fire the first bowls', 'done', '', 'plan', 'o', 'u', 'fired', NULL, 1),
  (2, 'acme/kiln', 'A', 2, 'Glaze the platters', 'done', '', 'plan', 'o', 'u', 'glazed', NULL, 1),
  (3, 'acme/kiln', 'T', 1, 'Stack the firewood', 'done', '', 'task', 'o', 'u', 'stacked at {}', 1, 1),
  (4, 'acme/kiln', 'T', 2, 'Mix the slip', 'done', '', 'task', 'o', 'u', 'mixed at {}', 2, 1);
{cooled}
"#,
            closes[0], closes[1]
        );
        let server = serve(
            Box::leak(seed.into_boxed_str()),
            "alpha owner key-owner\nbeta agent key-agent",
        );
        Nested {
            _tmp: tmp,
            dir,
            glaze,
            config,
            server,
            pins,
        }
    }

    fn squash(&self, args: &[&str]) -> Output {
        squash(&self.dir, &self.config, &self.server, args)
    }
}

#[test]
fn test_each_published_tree_pins_the_published_submodule_commit_of_its_snapshots_pin() {
    let r = Nested::new(false);
    let file = messages(&r.dir, "Fire the first bowls\nGlaze the platters\n");
    let out = r.squash(&["--messages", &file]);
    assert!(out.status.success(), "{}", text(&out));

    let inner = published(&r.glaze).expect("the submodule's published ref is written");
    let inner_chain = sh(&r.glaze, "git rev-list --reverse origin/main..shelf");
    let inner_chain: Vec<&str> = inner_chain.lines().collect();
    assert_eq!(inner_chain.len(), 2);
    assert_eq!(inner_chain[1], inner);
    for (commit, pin) in inner_chain.iter().zip(&r.pins) {
        assert_eq!(
            sh(&r.glaze, &format!("git rev-parse {commit}^{{tree}}")),
            sh(&r.glaze, &format!("git rev-parse {pin}^{{tree}}"))
        );
    }

    let chain = sh(&r.dir, "git rev-list --reverse origin/main..shelf");
    let chain: Vec<&str> = chain.lines().collect();
    assert_eq!(chain.len(), 2);
    for (commit, want) in chain.iter().zip(&inner_chain) {
        let link = sh(&r.dir, &format!("git rev-parse {commit}:{GLAZE}"));
        assert_eq!(&link, want);
        assert_eq!(
            sh(&r.dir, &format!("git ls-tree {commit} README")),
            sh(&r.dir, "git ls-tree main README")
        );
    }
    // The work branch still pins the submodule's own work commits.
    assert_eq!(
        sh(&r.dir, &format!("git rev-parse main:{GLAZE}")),
        r.pins[1]
    );
    assert!(
        text(&out).contains("git push origin shelf:main"),
        "{}",
        text(&out)
    );

    // A later landing that leaves the submodule alone pins its already published commit.
    sh(&r.dir, "git checkout -q -b t9 main");
    fs::write(r.dir.join("README"), "fired twice").unwrap();
    sh(
        &r.dir,
        "git add README && git commit -q -m readme && git checkout -q main",
    );
    sh(&r.dir, "git merge -q --no-ff -m 'merge t9' t9");
    let file = messages(&r.dir, "Note the firing\n");
    let out = r.squash(&["--messages", &file]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(published(&r.glaze).as_deref(), Some(inner.as_str()));
    assert_eq!(sh(&r.dir, &format!("git rev-parse shelf:{GLAZE}")), inner);
}

#[test]
fn test_a_snapshot_pinning_a_submodule_commit_with_no_published_counterpart_is_refused() {
    let r = Nested::new(true);
    let file = messages(
        &r.dir,
        "Fire the first bowls\nGlaze the platters\nCool the oven\n",
    );
    let out = r.squash(&["--messages", &file]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("0123456789abcdef0123456789abcdef01234567"),
        "{}",
        text(&out)
    );
    assert_eq!(published(&r.dir), None);
    assert_eq!(published(&r.glaze), None);
}

/// Runs any `docket` verb in the checkout against the test server.
fn docket(r: &Repo, args: &[&str]) -> String {
    docket_in(&r.dir, &r.config, &r.server, args)
}

fn docket_in(dir: &Path, config: &Path, server: &Server, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_docket"))
        .args(["-p", "acme/kiln"])
        .args(args)
        .current_dir(dir)
        .env("XDG_CONFIG_HOME", config)
        .env("DOCKET_SERVER", format!("http://{}", server.addr))
        .env("DOCKET_KEY", "key-owner")
        .env("USER", "nobody-in-particular")
        .env("HOME", config.parent().unwrap())
        .env("DOCKET_PROJECT", "acme/kiln")
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {}", text(&out));
    text(&out)
}

#[test]
fn test_next_offers_a_squash_while_a_closed_plan_is_unpublished_and_stops_after_it() {
    let r = Repo::new();
    assert!(docket(&r, &["next"]).contains("Squash: A1, A2 closed after the last publication"));
    let file = r.messages("Fire the first bowls\nGlaze the platters\n");
    let out = r.squash(&["--messages", &file]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(!docket(&r, &["next"]).contains("Squash:"));
}

#[test]
fn test_show_names_the_publication_whose_snapshot_first_holds_the_close_sha() {
    let r = Repo::new();
    let file = r.messages("Fire the first bowls\nGlaze the platters\n");
    assert!(r.squash(&["--messages", &file]).status.success());
    let commits = sh(&r.dir, "git rev-list --reverse refs/heads/shelf");
    let commits: Vec<&str> = commits.lines().collect();
    let (first, second) = (commits[commits.len() - 2], commits[commits.len() - 1]);
    assert!(docket(&r, &["show", "T1"]).contains(&format!("published: {first}")));
    assert!(docket(&r, &["show", "T4"]).contains(&format!("published: {second}")));
}

#[test]
fn test_an_unpushed_publication_is_an_act_ask_until_the_remote_ref_holds_it() {
    let r = Repo::new();
    let file = r.messages("Fire the first bowls\nGlaze the platters\n");
    assert!(r.squash(&["--messages", &file]).status.success());
    let todo = docket(&r, &["todo"]);
    assert!(todo.contains("An action from your machine:"), "{todo}");
    assert!(todo.contains("Push shelf to origin/main"), "{todo}");
    let tip = r.published().unwrap();
    sh(
        &r.dir,
        &format!("git update-ref refs/remotes/origin/main {tip}"),
    );
    assert!(docket(&r, &["todo"]).contains("Nothing waiting on you."));
}

#[test]
fn test_the_push_ask_names_each_submodule_push_first_and_stays_open_until_all_are_pushed() {
    let r = Nested::new(false);
    let file = messages(&r.dir, "Fire the first bowls\nGlaze the platters\n");
    let out = r.squash(&["--messages", &file]);
    assert!(out.status.success(), "{}", text(&out));
    let todo = docket_in(&r.dir, &r.config, &r.server, &["todo"]);
    assert!(todo.contains("Push shelf to origin/main"), "{todo}");
    let todo = docket_in(&r.dir, &r.config, &r.server, &["show", "T3"]);
    let inner = todo
        .find(&format!("git -C {GLAZE} push origin shelf:main"))
        .unwrap_or_else(|| panic!("{todo}"));
    let parent = todo
        .find("git push origin shelf:main")
        .unwrap_or_else(|| panic!("{todo}"));
    assert!(inner < parent, "{todo}");

    let tip = published(&r.dir).unwrap();
    sh(
        &r.dir,
        &format!("git update-ref refs/remotes/origin/main {tip}"),
    );
    let todo = docket_in(&r.dir, &r.config, &r.server, &["todo"]);
    assert!(todo.contains("Push shelf to origin/main"), "{todo}");

    let tip = published(&r.glaze).unwrap();
    sh(
        &r.glaze,
        &format!("git update-ref refs/remotes/origin/main {tip}"),
    );
    let todo = docket_in(&r.dir, &r.config, &r.server, &["todo"]);
    assert!(todo.contains("Nothing waiting on you."), "{todo}");
}
