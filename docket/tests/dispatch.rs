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
INSERT INTO projects (slug, keys, skills, created_at, updated_at) VALUES ('o/p',
  '[{"key":"T","kind":"work"},{"key":"A","kind":"audit"},{"key":"Q","kind":"decision"}]',
  '{"models":"low=claude:small medium=codex:middle high=claude:large:high audit=codex:middle:high"}',
  'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, tags, body, complexity, opened_at, updated_at) VALUES
  (1, 'o/p', 'T', 1, 'Write one', 'open', 'agent', '[]', '', 'low', '2026-01-01T00:00:00Z', 'u'),
  (2, 'o/p', 'T', 2, 'Write two', 'open', 'agent', '[]', '', 'low', '2026-01-01T00:00:00Z', 'u'),
  (3, 'o/p', 'T', 3, 'Write three', 'open', 'agent', '[]', '', 'high', '2026-01-01T00:00:00Z', 'u'),
  (4, 'o/p', 'A', 1, 'A plan', 'open', 'agent', '[]', '', NULL, '2026-01-01T00:00:00Z', 'u');
"#;

/// The stand-in for ssh: options skipped, the host's home put in place, the command run here.
const SSH: &str = r#"#!/bin/sh
while [ "${1#-}" != "$1" ]; do shift 2; done
host=$1; shift
home="$MACHINES/$host"
[ -d "$home" ] || { echo "ssh: no host $host" >&2; exit 255; }
export HOME="$home" XDG_CONFIG_HOME="$home/.config" XDG_STATE_HOME="$home/.state"
export DOCKET_KEY="$(cat "$home/key")"
exec sh -c "$*"
"#;

/// The stand-in agent: one change left in its worktree, uncommitted, then its report as a Claude
/// Code result. `AGENT_DOES` is shell it runs first, and `AGENT_SAYS` replaces the report, for an
/// agent that proposes no message.
const AGENT: &str = r#"#!/bin/sh
eval "$AGENT_DOES"
echo "$DOCKET_JOB" > made-by-job
said=${AGENT_SAYS:-'NOTE wrote made-by-job\nMESSAGE Write the job marker\nDONE'}
printf '{"type":"result","result":"%s","usage":{"output_tokens":7}}\n' "$said"
"#;

/// Two machines, each a home with its key and a clone of the project bound as its root.
struct World {
    _tmp: tempfile::TempDir,
    /// What the stand-in agent reports instead of its usual note, message and DONE.
    says: String,
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

    fn build(submodule: bool) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let machines = tmp.path().join("machines");
        let bin = tmp.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        script(&bin.join("ssh"), SSH);
        script(&bin.join("agent"), AGENT);
        let w = World {
            says: String::new(),
            does: String::new(),
            machines,
            bin,
            server: serve(SEED, "alpha owner key-alpha\nbeta owner key-beta"),
            _tmp: tmp,
        };
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
            fs::create_dir_all(home.join("src")).unwrap();
            fs::write(home.join("key"), format!("key-{name}")).unwrap();
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
        for name in ["alpha", "beta"] {
            let out = w.lead(&[
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
        w
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
    assert_eq!(sh(&beta, "git worktree list").lines().count(), 1);
    assert_eq!(sh(&beta, &format!("git branch --list {branch}")), "");
    assert_eq!(
        sh(&beta, "git log --all --format=%s"),
        "start",
        "nothing was committed on the job's machine"
    );
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
    assert_eq!(sh(&alpha, "git worktree list").lines().count(), 1);
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
fn test_a_plan_is_dispatched_as_an_audit_on_the_audit_model() {
    let w = World::new();
    let out = w.lead(&["dispatch", "A1", "--on", "beta"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("audit on lead/a1-"), "{}", text(&out));
    let a1 = w.show("A1");
    assert_eq!(a1["claim_runner"], "codex");
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
    assert_eq!(sh(&beta, "git worktree list").lines().count(), 1);
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
    assert_eq!(sh(&beta, "git worktree list").lines().count(), 1);
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
    assert_eq!(sh(&beta, "git worktree list").lines().count(), 1);
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
