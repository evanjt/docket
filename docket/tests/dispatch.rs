//! Scenario: a lead on machine alpha dispatches tickets to itself and to machine beta, each machine a
//! home of its own with its own key, checkout and job state, reached through a stand-in for ssh that
//! runs the command locally under that machine's home. The jobs run a stand-in agent that commits
//! and reports.
//! Expected behaviour: the code reaches the job's machine and comes back by git, a held ticket is
//! not dispatched twice, a failed push gives the claim back, and every machine's jobs are read.

use std::fs;
mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{Server, serve};

const SEED: &str = r#"
INSERT INTO projects (slug, skills, created_at, updated_at) VALUES ('o/p',
  '{"models":"low=claude:small medium=codex:middle high=claude:large:high audit=codex:middle:high"}',
  'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, body, complexity, opened_at, updated_at) VALUES
  (1, 'o/p', 'T', 1, 'Write one', 'open', '', 'low', '2026-01-01T00:00:00Z', 'u'),
  (2, 'o/p', 'T', 2, 'Write two', 'open', '', 'low', '2026-01-01T00:00:00Z', 'u'),
  (3, 'o/p', 'T', 3, 'Write three', 'open', '', 'high', '2026-01-01T00:00:00Z', 'u'),
  (4, 'o/p', 'A', 1, 'A plan', 'open', '', NULL, '2026-01-01T00:00:00Z', 'u'),
  (5, 'o/p', 'T', 4, 'Write four', 'open', '', 'medium', '2026-01-01T00:00:00Z', 'u'),
  (6, 'o/p', 'T', 5, 'Write five', 'open', '', 'medium', '2026-01-01T00:00:00Z', 'u'),
  (7, 'o/p', 'T', 6, 'Write six', 'open', '', 'medium', '2026-01-01T00:00:00Z', 'u'),
  (8, 'o/p', 'A', 2, 'A finished plan', 'open', '', NULL, '2026-01-01T00:00:00Z', 'u'),
  (9, 'o/p', 'T', 7, 'Written', 'open', '', 'low', '2026-01-01T00:00:00Z', 'u');
UPDATE items SET parent_rid = 8, state = 'done', resolution = 'abc1234' WHERE rid = 9;
INSERT INTO labels (id, project, name) VALUES (1, 'o/p', 'group:streams');
INSERT INTO item_labels (rid, label_id) VALUES (1, 1), (2, 1);
UPDATE items SET type = 'plan' WHERE key = 'A';
"#;

/// The stand-in for ssh: options skipped, the host's home put in place, the command run here.
const SSH: &str = r#"#!/bin/sh
while [ "${1#-}" != "$1" ]; do shift 2; done
host=$1; shift
echo "$host $*" >> "$MACHINES/ssh.log"
home="$MACHINES/$host"
[ -d "$home" ] || { echo "ssh: no host $host" >&2; exit 255; }
export HOME="$home" XDG_CONFIG_HOME="$home/.config" XDG_STATE_HOME="$home/.state"
export DOCKET_KEY="$(cat "$home/key")"
exec sh -c "$*"
"#;

/// The stand-in agent: one change left in its worktree, uncommitted, then its report as a Claude
/// Code result. `AGENT_DOES` is shell it runs first, `AGENT_SAYS` replaces the report, for an
/// agent that proposes no message, and `AGENT_EVENT` is one stream line it prints instead, then fails.
const AGENT: &str = r#"#!/bin/sh
eval "$AGENT_DOES"
if [ -n "$AGENT_EVENT" ]; then printf '%s\n' "$AGENT_EVENT"; exit 1; fi
echo "$DOCKET_JOB" > made-by-job
said=${AGENT_SAYS:-'NOTE wrote made-by-job\nMESSAGE Write the job marker\nDONE'}
printf '{"type":"result","result":"%s","usage":{"output_tokens":7}}\n' "$said"
"#;

/// Two machines, each a home with its key and a clone of the project bound as its root.
struct World {
    _tmp: tempfile::TempDir,
    /// What the stand-in agent reports instead of its usual note, message and DONE.
    says: String,
    /// The one stream line the stand-in agent prints, and fails, instead of reporting.
    event: String,
    /// Shell the stand-in agent runs in its worktree before its usual change.
    does: String,
    machines: PathBuf,
    bin: PathBuf,
    server: Server,
}

impl World {
    fn new() -> Self {
        Self::build(false)
    }

    /// A world whose project pins a submodule at `lib`, checked out in each machine's clone.
    fn with_submodule() -> Self {
        Self::build(true)
    }

    /// A world whose project root holds two sibling repositories, `kites` and `lanterns`, and no
    /// repository of its own: alpha's root is `src/p`, beta's one directory deeper, `src/deep/p`.
    fn with_siblings() -> Self {
        let w = Self::bare();
        for repo in ["kites", "lanterns"] {
            let origin = w.machines.join("origins").join(repo);
            fs::create_dir_all(&origin).unwrap();
            sh(
                &origin,
                &format!(
                    "git init -q -b main && echo {repo} > {repo}.txt && git add {repo}.txt \
                     && git -c user.name=o -c user.email=o@example.org commit -q -m 'Start {repo}'"
                ),
            );
        }
        for (name, root) in [("alpha", "src/p"), ("beta", "src/deep/p")] {
            let root = w.home(name).join(root);
            fs::create_dir_all(&root).unwrap();
            for repo in ["kites", "lanterns"] {
                let origin = w.machines.join("origins").join(repo);
                sh(&root, &format!("git clone -q {} {repo}", origin.display()));
            }
            let out = w.docket(name, &root, &["bind", "o/p"]);
            assert!(out.status.success(), "{}", text(&out));
        }
        w.set_machines();
        w
    }

    /// Two machine homes, each with its key and client file, and nothing cloned.
    fn bare() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let machines = tmp.path().join("machines");
        let bin = tmp.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        script(&bin.join("ssh"), SSH);
        script(&bin.join("agent"), AGENT);
        let w = World {
            says: String::new(),
            event: String::new(),
            does: String::new(),
            machines,
            bin,
            server: serve(
                SEED,
                "alpha owner key-alpha\nbeta owner key-beta\nalpha agent job-alpha\nbeta agent job-beta",
            ),
            _tmp: tmp,
        };
        for name in ["alpha", "beta"] {
            let home = w.home(name);
            fs::create_dir_all(home.join("src")).unwrap();
            fs::write(home.join("key"), format!("key-{name}")).unwrap();
            w.set_client_config(name, &format!("job_key = job-{name}\n"));
        }
        w
    }

    /// Both machines set on the server, two slots each, either runner.
    fn set_machines(&self) {
        for name in ["alpha", "beta"] {
            let out = self.lead(&[
                "machine",
                "set",
                name,
                "--ssh",
                name,
                "--slots",
                "2",
                "--runners",
                "claude,codex",
            ]);
            assert!(out.status.success(), "{}", text(&out));
        }
    }

    fn build(submodule: bool) -> Self {
        let w = Self::bare();
        let origin = w.machines.join("origin");
        fs::create_dir_all(&origin).unwrap();
        sh(
            &origin,
            "git init -q -b main && printf 'first\\n\\n' > notes.txt && git add notes.txt \
             && git -c user.name=o -c user.email=o@example.org commit -q -m start",
        );
        if submodule {
            let lib = w.machines.join("lib");
            fs::create_dir_all(&lib).unwrap();
            sh(
                &lib,
                "git init -q -b main && echo one > lib.txt && git add lib.txt \
                 && git -c user.name=o -c user.email=o@example.org commit -q -m 'Add lib'",
            );
            sh(
                &origin,
                &format!(
                    "git -c protocol.file.allow=always submodule add -q {} lib \
                     && git -c user.name=o -c user.email=o@example.org commit -q -m 'Pin lib'",
                    lib.display()
                ),
            );
        }
        for name in ["alpha", "beta"] {
            let home = w.home(name);
            sh(
                &home.join("src"),
                &format!(
                    "git -c protocol.file.allow=always clone -q --recurse-submodules {} p",
                    origin.display()
                ),
            );
            let out = w.docket(name, &home.join("src/p"), &["bind", "o/p"]);
            assert!(out.status.success(), "{}", text(&out));
        }
        w.set_machines();
        w
    }

    /// The machine's client file, which holds the key its jobs present.
    fn set_client_config(&self, name: &str, text: &str) {
        let dir = self.home(name).join(".config/docket");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("client"), text).unwrap();
    }

    fn home(&self, name: &str) -> PathBuf {
        self.machines.join(name)
    }

    /// `docket ARGS` as a client on the machine, in a directory there.
    fn docket(&self, name: &str, cwd: &Path, args: &[&str]) -> Output {
        let home = self.home(name);
        Command::new(env!("CARGO_BIN_EXE_docket"))
            .args(args)
            .current_dir(cwd)
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_STATE_HOME", home.join(".state"))
            .env("DOCKET_SERVER", format!("http://{}", self.server.addr))
            .env("DOCKET_KEY", format!("key-{name}"))
            .env("DOCKET_SSH", self.bin.join("ssh"))
            .env("DOCKET_JOB_RUNNER_CLAUDE", self.bin.join("agent"))
            .env("DOCKET_JOB_RUNNER_CODEX", self.bin.join("agent"))
            .env("MACHINES", &self.machines)
            .env("AGENT_SAYS", &self.says)
            .env("AGENT_DOES", &self.does)
            .env("AGENT_EVENT", &self.event)
            .env("GIT_AUTHOR_NAME", "lead")
            .env("GIT_AUTHOR_EMAIL", "lead@example.org")
            .env("GIT_COMMITTER_NAME", "lead")
            .env("GIT_COMMITTER_EMAIL", "lead@example.org")
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    Path::new(env!("CARGO_BIN_EXE_docket"))
                        .parent()
                        .unwrap()
                        .display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env_remove("GIT_SSH_COMMAND")
            // What the surrounding job or shell sets must not reach the machines under test.
            .env_remove("DOCKET_PROJECT")
            .env_remove("DOCKET_JOB")
            .env_remove("DOCKET_REPO")
            .env_remove("DOCKET_JOB_STATE")
            .env_remove("DOCKET_JOBS_FALLBACK")
            .output()
            .unwrap()
    }

    /// `docket ARGS` as the lead, on alpha in its checkout.
    fn lead(&self, args: &[&str]) -> Output {
        self.docket("alpha", &self.home("alpha").join("src/p"), args)
    }

    fn show(&self, id: &str) -> serde_json::Value {
        let out = self.lead(&["--json", "show", id]);
        assert!(out.status.success(), "{}", text(&out));
        serde_json::from_slice(&out.stdout).unwrap()
    }
}

fn script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
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

/// A collected job leaves its worktree parked on a detached head for the next job to reuse.
fn assert_parked(machine: &Path) {
    let list = sh(machine, "git worktree list");
    assert_eq!(list.lines().count(), 2, "{list}");
    assert!(
        list.lines().nth(1).unwrap().ends_with("(detached HEAD)"),
        "{list}"
    );
}

#[test]
fn test_a_lead_dispatches_to_another_machine_and_collects_the_branch() {
    let w = World::new();
    let out = w.lead(&["dispatch", "T1", "--on", "beta"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("dispatched T1 to beta"),
        "{}",
        text(&out)
    );
    let t1 = w.show("T1");
    assert_eq!(t1["claim_on"], "beta");
    assert_eq!(t1["claim_runner"], "claude");
    let branch = t1["claim_branch"].as_str().unwrap().to_string();

    let again = w.lead(&["dispatch", "T1"]);
    assert!(!again.status.success());
    assert!(text(&again).contains("is held by"), "{}", text(&again));

    let jobs = w.lead(&["jobs", "--wait", "--every", "1"]);
    assert!(jobs.status.success(), "{}", text(&jobs));
    let listed = text(&jobs);
    assert!(
        listed.contains("beta") && listed.contains("done"),
        "{listed}"
    );

    let collected = w.lead(&["collect", "T1"]);
    assert!(collected.status.success(), "{}", text(&collected));
    assert!(
        text(&collected).contains("committed here as"),
        "{}",
        text(&collected)
    );
    let alpha = w.home("alpha").join("src/p");
    assert_eq!(
        sh(&alpha, &format!("git show {branch}:made-by-job")),
        branch.replace('/', "-")
    );
    assert_eq!(
        sh(&alpha, &format!("git log -1 --format=%s {branch}")),
        "Write the job marker"
    );
    let beta = w.home("beta").join("src/p");
    assert_parked(&beta);
    assert_eq!(sh(&beta, &format!("git branch --list {branch}")), "");
    assert_eq!(
        sh(&beta, "git log --all --format=%s"),
        "start",
        "nothing was committed on the job's machine"
    );
}

#[test]
fn test_an_item_runs_in_the_repository_it_names_on_each_machine() {
    let w = World::with_siblings();
    let lead_root = w.home("alpha").join("src/p");
    let kites = lead_root.join("kites");
    let edit = w.docket("alpha", &kites, &["edit", "T1", "--set", "repo=lanterns"]);
    assert!(edit.status.success(), "{}", text(&edit));
    assert_eq!(w.show("T1")["repo"], "lanterns");

    let queue = w.docket("alpha", &kites, &["--json", "next", "--repo", "lanterns"]);
    assert!(queue.status.success(), "{}", text(&queue));
    let ids: Vec<String> = serde_json::from_slice::<serde_json::Value>(&queue.stdout)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, ["T1"]);

    let out = w.docket("alpha", &kites, &["dispatch", "T1", "--on", "beta"]);
    assert!(out.status.success(), "{}", text(&out));
    let branch = w.show("T1")["claim_branch"].as_str().unwrap().to_string();
    let beta_lanterns = w.home("beta").join("src/deep/p/lanterns");
    let worktrees = sh(&beta_lanterns, "git worktree list");
    assert_eq!(worktrees.lines().count(), 2, "{worktrees}");
    assert_eq!(
        sh(
            &w.home("beta").join("src/deep/p/kites"),
            "git worktree list"
        )
        .lines()
        .count(),
        1
    );

    let _ = w.docket("alpha", &kites, &["jobs", "--wait", "--every", "1"]);
    let collected = w.docket("alpha", &kites, &["collect", "T1"]);
    assert!(collected.status.success(), "{}", text(&collected));
    let lanterns = lead_root.join("lanterns");
    assert_eq!(
        sh(&lanterns, &format!("git show {branch}:made-by-job")),
        branch.replace('/', "-")
    );
    assert_eq!(
        sh(&lanterns, &format!("git log --format=%s main..{branch}")),
        "Write the job marker"
    );
    assert_eq!(sh(&kites, &format!("git branch --list {branch}")), "");
    assert_parked(&beta_lanterns);
}

/// The commands sent over ssh that read a machine's jobs.
fn status_reads(w: &World) -> usize {
    fs::read_to_string(w.machines.join("ssh.log"))
        .unwrap_or_default()
        .lines()
        .filter(|l| l.contains("job status"))
        .count()
}

#[test]
fn test_a_job_reports_its_end_and_jobs_wait_follows_it_without_reading_a_machine() {
    let mut w = World::new();
    w.does = "sleep 3".into();
    let out = w.lead(&["dispatch", "T1", "--on", "beta"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(status_reads(&w), 0, "dispatch reads no machine");

    let jobs = w.lead(&["jobs", "--wait", "--every", "1"]);
    assert!(jobs.status.success(), "{}", text(&jobs));
    assert!(text(&jobs).contains("done"), "{}", text(&jobs));
    assert_eq!(
        status_reads(&w),
        1,
        "only the table at the end reads a machine over ssh, beta, alpha being this one"
    );

    let log = w.lead(&["--json", "log", "T1"]);
    let log: serde_json::Value = serde_json::from_slice(&log.stdout).unwrap();
    let reported = log
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "job_reported")
        .unwrap_or_else(|| panic!("{log}"));
    assert_eq!(reported["data"]["exit"], 0);
    assert!(reported["data"]["end"].is_string(), "{reported}");
    assert_eq!(reported["data"]["report"], "DONE");

    let collected = w.lead(&["collect", "T1"]);
    assert!(collected.status.success(), "{}", text(&collected));
    let log = w.lead(&["--json", "log", "T1"]);
    let log: serde_json::Value = serde_json::from_slice(&log.stdout).unwrap();
    let reports = log
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "job_reported")
        .count();
    assert_eq!(reports, 1, "collect does not write the end a second time");
}

#[test]
fn test_a_lead_runs_a_job_on_its_own_machine_by_the_models_fact() {
    let w = World::new();
    let out = w.lead(&["dispatch", "T3", "--on", "alpha"]);
    assert!(out.status.success(), "{}", text(&out));
    let t3 = w.show("T3");
    assert_eq!(t3["claim_on"], "alpha");
    assert_eq!(t3["claim_runner"], "claude");
    let branch = t3["claim_branch"].as_str().unwrap().to_string();
    let jobs = w.lead(&["jobs", "--wait", "--every", "1"]);
    assert!(text(&jobs).contains("large"), "{}", text(&jobs));
    let collected = w.lead(&["collect", "T3"]);
    assert!(collected.status.success(), "{}", text(&collected));
    let alpha = w.home("alpha").join("src/p");
    assert!(sh(&alpha, &format!("git branch --list {branch}")).contains(&branch));
    assert_eq!(
        sh(&alpha, &format!("git log -1 --format=%s {branch}")),
        "Write the job marker"
    );
    assert_parked(&alpha);
}

#[test]
fn test_a_dispatch_whose_push_fails_gives_the_claim_back() {
    let w = World::new();
    let beta = w.home("beta").join("src/p");
    script(&beta.join(".git/hooks/pre-receive"), "#!/bin/sh\nexit 1\n");
    let out = w.lead(&["dispatch", "T2", "--on", "beta"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("claim given back"), "{}", text(&out));
    assert!(w.show("T2")["claim_branch"].is_null());
}

#[test]
fn test_a_dispatch_to_a_machine_without_the_runner_is_refused_before_any_claim() {
    let w = World::new();
    let set = w.lead(&["machine", "set", "beta", "--runners", "codex"]);
    assert!(set.status.success(), "{}", text(&set));
    let out = w.lead(&["dispatch", "T1", "--on", "beta"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("beta has no claude"), "{}", text(&out));
    assert!(w.show("T1")["claim_branch"].is_null());
}

#[test]
fn test_a_plan_with_nothing_under_it_is_dispatched_as_a_plan() {
    let w = World::new();
    let out = w.lead(&["dispatch", "A1", "--on", "beta"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("plan on lead/a1-"), "{}", text(&out));
}

#[test]
fn test_a_plan_with_children_is_dispatched_as_an_audit_on_the_audit_model() {
    let w = World::new();
    let out = w.lead(&["dispatch", "A2", "--on", "beta"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("audit on lead/a2-"), "{}", text(&out));
    let a2 = w.show("A2");
    assert_eq!(a2["claim_runner"], "codex");
    let jobs = w.lead(&["jobs", "--wait", "--every", "1"]);
    assert!(text(&jobs).contains("middle"), "{}", text(&jobs));
}

#[test]
fn test_a_discarded_job_is_cleared_and_nothing_is_committed() {
    let w = World::new();
    assert!(w.lead(&["dispatch", "T2", "--on", "beta"]).status.success());
    let branch = w.show("T2")["claim_branch"].as_str().unwrap().to_string();
    let _ = w.lead(&["jobs", "--wait", "--every", "1"]);
    let out = w.lead(&["collect", "T2", "--discard"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("discarded"), "{}", text(&out));
    let alpha = w.home("alpha").join("src/p");
    assert_eq!(sh(&alpha, &format!("git branch --list {branch}")), "");
    let beta = w.home("beta").join("src/p");
    assert_parked(&beta);
}

#[test]
fn test_a_change_without_a_message_is_refused_and_the_job_kept() {
    let mut w = World::new();
    w.says = "NOTE wrote it\\nDONE".into();
    assert!(w.lead(&["dispatch", "T2", "--on", "beta"]).status.success());
    let _ = w.lead(&["jobs", "--wait", "--every", "1"]);
    let out = w.lead(&["collect", "T2"]);
    assert!(
        text(&out).contains("committed here as"),
        "the note serves as the message: {}",
        text(&out)
    );
    w.says = "DONE".into();
    assert!(w.lead(&["dispatch", "T1", "--on", "beta"]).status.success());
    let _ = w.lead(&["jobs", "--wait", "--every", "1"]);
    let out = w.lead(&["collect", "T1"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("proposed no MESSAGE"), "{}", text(&out));
    let beta = w.home("beta").join("src/p");
    assert_eq!(
        sh(&beta, "git worktree list").lines().count(),
        2,
        "the job is kept"
    );
}

#[test]
fn test_collect_appends_a_jobs_observations_to_its_item() {
    let mut w = World::new();
    w.says = "OBSERVE the retry sleeps a fixed second\\nOBSERVE the log names no host\\nNOTE wrote it\\nMESSAGE Write the job marker\\nDONE".into();
    assert!(w.lead(&["dispatch", "T2", "--on", "beta"]).status.success());
    let _ = w.lead(&["jobs", "--wait", "--every", "1"]);
    let out = w.lead(&["collect", "T2"]);
    assert!(out.status.success(), "{}", text(&out));
    let body = w.show("T2")["body"].as_str().unwrap().to_string();
    assert!(
        body.contains("- the retry sleeps a fixed second\n- the log names no host"),
        "{body}"
    );
}

/// The stand-in agent checks the submodule out in its worktree from the machine's own clone, commits
/// one change inside it and leaves another there uncommitted.
const EDITS_LIB: &str = r#"
sha=$(git ls-tree HEAD lib | awk '{print $3}')
main=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")
rmdir lib && git clone -q --no-checkout "$main/lib" lib && git -C lib checkout -q "$sha"
echo committed > lib/committed.txt
git -C lib add committed.txt
git -C lib -c user.name=j -c user.email=j@example.org commit -q -m 'Commit inside the job'
echo two > lib/lib.txt
"#;

#[test]
fn test_collect_commits_a_change_inside_a_submodule_in_the_leads_submodule() {
    let mut w = World::with_submodule();
    w.does = EDITS_LIB.into();
    assert!(w.lead(&["dispatch", "T1", "--on", "beta"]).status.success());
    let branch = w.show("T1")["claim_branch"].as_str().unwrap().to_string();
    let _ = w.lead(&["jobs", "--wait", "--every", "1"]);
    let alpha = w.home("alpha").join("src/p");
    let pinned = sh(&alpha.join("lib"), "git rev-parse HEAD");

    let out = w.lead(&["collect", "T1"]);
    assert!(out.status.success(), "{}", text(&out));
    let lib = alpha.join("lib");
    assert_eq!(sh(&lib, &format!("git show {branch}:lib.txt")), "two");
    assert_eq!(
        sh(&lib, &format!("git show {branch}:committed.txt")),
        "committed"
    );
    assert_eq!(
        sh(&lib, &format!("git log --format=%s {pinned}..{branch}")),
        "Write the job marker",
        "one commit in the submodule, on the commit the project pinned"
    );
    let pointer = sh(&alpha, &format!("git rev-parse {branch}:lib"));
    assert_eq!(pointer, sh(&lib, &format!("git rev-parse {branch}")));
    assert_eq!(
        sh(&alpha, &format!("git show {branch}:made-by-job")),
        branch.replace('/', "-")
    );
    assert_eq!(
        sh(&alpha, &format!("git log --format=%s main..{branch}")),
        "Write the job marker"
    );
    assert_eq!(
        sh(&lib, "git rev-parse HEAD"),
        pinned,
        "the checkout is left"
    );
    assert_eq!(sh(&lib, "git status --porcelain"), "");
    assert_eq!(
        sh(&w.machines.join("lib"), "git log --all --format=%s"),
        "Add lib",
        "nothing was pushed"
    );
    let beta = w.home("beta").join("src/p");
    assert_parked(&beta);
}

#[test]
fn test_a_change_that_cannot_be_committed_here_keeps_the_job() {
    let w = World::new();
    assert!(w.lead(&["dispatch", "T1", "--on", "beta"]).status.success());
    let branch = w.show("T1")["claim_branch"].as_str().unwrap().to_string();
    let _ = w.lead(&["jobs", "--wait", "--every", "1"]);
    let alpha = w.home("alpha").join("src/p");
    let hook = alpha.join(".git/hooks/pre-commit");
    script(&hook, "#!/bin/sh\necho refused here >&2\nexit 1\n");

    let out = w.lead(&["collect", "T1"]);
    assert!(!out.status.success(), "{}", text(&out));
    let beta = w.home("beta").join("src/p");
    assert_eq!(sh(&beta, "git worktree list").lines().count(), 2);
    assert!(text(&out).contains("refused here"), "{}", text(&out));
    assert!(text(&out).contains("kept on beta"), "{}", text(&out));
    let name = branch.replace('/', "-");
    let status = w.docket("beta", &beta, &["-p", "o/p", "job", "status", &name]);
    assert!(text(&status).contains("done"), "{}", text(&status));
    assert_eq!(w.show("T1")["claim_branch"], branch.as_str());
    assert_eq!(sh(&alpha, &format!("git branch --list {branch}")), "");

    fs::remove_file(&hook).unwrap();
    let again = w.lead(&["collect", "T1"]);
    assert!(again.status.success(), "{}", text(&again));
    assert_eq!(sh(&alpha, &format!("git show {branch}:made-by-job")), name);
    assert_parked(&beta);
}

#[test]
fn test_a_patch_ending_in_a_blank_context_line_is_committed_whole() {
    let mut w = World::new();
    w.does = "printf 'second\\n\\n' > notes.txt".into();
    assert!(w.lead(&["dispatch", "T1", "--on", "beta"]).status.success());
    let branch = w.show("T1")["claim_branch"].as_str().unwrap().to_string();
    let _ = w.lead(&["jobs", "--wait", "--every", "1"]);
    let out = w.lead(&["collect", "T1"]);
    assert!(out.status.success(), "{}", text(&out));
    let alpha = w.home("alpha").join("src/p");
    assert_eq!(
        sh(
            &alpha,
            &format!("git show {branch}:notes.txt | od -c | head -1")
        ),
        sh(&alpha, "printf 'second\\n\\n' | od -c | head -1")
    );
}

#[test]
fn test_a_second_member_of_a_group_is_refused_while_one_runs() {
    let mut w = World::new();
    w.does = "sleep 30".into();
    let first = w.lead(&["dispatch", "T1", "--on", "beta"]);
    assert!(first.status.success(), "{}", text(&first));
    let name = w.show("T1")["claim_job"].as_str().unwrap().to_string();

    let out = w.lead(&["dispatch", "T2", "--on", "beta"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("streams") && text(&out).contains(&name),
        "{}",
        text(&out)
    );
    assert!(w.show("T2")["claim_branch"].is_null());

    let other = w.lead(&["dispatch", "T3", "--on", "alpha"]);
    assert!(other.status.success(), "{}", text(&other));
    let forced = w.lead(&["dispatch", "T2", "--on", "alpha", "--force"]);
    assert!(forced.status.success(), "{}", text(&forced));
    for (id, on) in [("T1", "beta"), ("T2", "alpha"), ("T3", "alpha")] {
        let job = w.show(id)["claim_job"].as_str().unwrap().to_string();
        let _ = w.docket(
            on,
            &w.home(on).join("src/p"),
            &["-p", "o/p", "job", "kill", &job],
        );
    }
}

#[test]
fn test_a_dispatch_to_a_machine_that_cannot_be_reached_says_why_and_holds_no_claim() {
    let w = World::new();
    let add = w.lead(&[
        "machine",
        "set",
        "gamma",
        "--ssh",
        "gamma",
        "--slots",
        "2",
        "--runners",
        "claude,codex",
    ]);
    assert!(add.status.success(), "{}", text(&add));
    let out = w.lead(&["dispatch", "T1", "--on", "gamma"]);
    assert!(!out.status.success());
    let said = text(&out);
    assert!(said.contains("no host gamma"), "{said}");
    assert!(w.show("T1")["claim_branch"].is_null());
}

#[test]
fn test_a_runner_that_reports_a_usage_limit_is_skipped_on_every_machine_until_its_reset() {
    let mut w = World::new();
    w.event = r#"{"type":"error","message":"weekly usage limit reached, resets 2999-10-07 18:29"}"#
        .into();
    for (id, on) in [("T4", "alpha"), ("T5", "beta")] {
        let out = w.lead(&["dispatch", id, "--on", on]);
        assert!(out.status.success(), "{}", text(&out));
        assert_eq!(w.show(id)["claim_runner"], "codex");
    }
    let jobs = w.lead(&["jobs", "--wait", "--every", "1"]);
    assert!(jobs.status.success(), "{}", text(&jobs));

    let machines = w.lead(&["machines"]);
    let listed = text(&machines);
    assert_eq!(
        listed
            .matches("codex limited until 2999-10-07T18:29:00Z")
            .count(),
        2,
        "{listed}"
    );

    let collected = w.lead(&["collect", "T4", "--discard"]);
    assert!(
        text(&collected).contains("usage limit: codex on alpha until 2999-10-07T18:29:00Z"),
        "{}",
        text(&collected)
    );

    let audit = w.lead(&["dispatch", "A1"]);
    assert!(audit.status.success(), "{}", text(&audit));
    let a1 = w.show("A1");
    assert_eq!(a1["claim_runner"], "claude");
    assert!(text(&audit).contains("claude large"), "{}", text(&audit));

    let on_alpha = w.lead(&[
        "dispatch", "T6", "--on", "alpha", "--runner", "codex", "--model", "m",
    ]);
    assert!(!on_alpha.status.success());
    assert!(
        text(&on_alpha)
            .contains("codex on alpha reported a usage limit until 2999-10-07T18:29:00Z"),
        "{}",
        text(&on_alpha)
    );
    assert!(w.show("T6")["claim_branch"].is_null());
}

#[test]
fn test_a_dispatch_with_no_runner_free_of_a_limit_is_refused_naming_the_reset() {
    let mut w = World::new();
    w.event = r#"{"type":"error","message":"usage limit reached, resets 2999-10-07 18:29"}"#.into();
    let set = w.lead(&[
        "skills",
        "set",
        "models",
        "medium=codex:middle low=codex:small",
    ]);
    assert!(set.status.success(), "{}", text(&set));
    for (id, on) in [("T4", "alpha"), ("T5", "beta")] {
        let out = w.lead(&["dispatch", id, "--on", on]);
        assert!(out.status.success(), "{}", text(&out));
    }
    w.lead(&["jobs", "--wait", "--every", "1"]);
    let out = w.lead(&["dispatch", "T6"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("usage limit"), "{}", text(&out));
}

#[test]
fn test_a_final_message_that_reports_a_limit_limits_the_runner_but_a_finished_job_quoting_one_does_not()
 {
    let mut w = World::new();
    w.says = "usage limit reached, resets 2999-10-07 18:29".into();
    assert!(
        w.lead(&["dispatch", "T3", "--on", "alpha"])
            .status
            .success()
    );
    w.lead(&["jobs", "--wait", "--every", "1"]);
    let listed = text(&w.lead(&["machines"]));
    assert!(
        listed.contains("claude limited until 2999-10-07T18:29:00Z"),
        "{listed}"
    );
    assert!(w.lead(&["collect", "T3", "--discard"]).status.success());

    let w = {
        let mut w = World::new();
        w.says =
            r"NOTE the usage limit reset line was resets 2999-10-07 18:29\nMESSAGE Write it\nDONE"
                .into();
        w
    };
    assert!(
        w.lead(&["dispatch", "T3", "--on", "alpha"])
            .status
            .success()
    );
    w.lead(&["jobs", "--wait", "--every", "1"]);
    assert!(!text(&w.lead(&["machines"])).contains("limited"));
}

#[test]
fn test_a_job_is_refused_unless_its_machine_names_an_agent_key() {
    let w = World::new();
    let beta = w.home("beta").join("src/p");
    let run = |w: &World| {
        w.docket(
            "beta",
            &beta,
            &[
                "-p",
                "o/p",
                "--branch",
                "lead/t1-1",
                "job",
                "run",
                "--id",
                "T1",
                "--runner",
                "claude",
                "--model",
                "small",
                "--role",
                "build",
            ],
        )
    };
    for (config, says) in [
        ("", "job_key"),
        ("job_key = key-beta\n", "owner key"),
        ("job_key = no-such-key\n", "is refused"),
    ] {
        w.set_client_config("beta", config);
        let out = run(&w);
        assert!(!out.status.success(), "{config}: {}", text(&out));
        assert!(text(&out).contains(says), "{config}: {}", text(&out));
        assert!(
            !text(&out).contains("no branch"),
            "refused before the branch is read: {}",
            text(&out)
        );
        assert!(
            !w.home("beta").join(".state").exists()
                || fs::read_dir(w.home("beta").join(".state"))
                    .unwrap()
                    .next()
                    .is_none(),
            "nothing started"
        );
    }
}
