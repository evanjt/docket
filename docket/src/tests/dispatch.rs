use std::path::{Path, PathBuf};

use super::*;

fn machine(name: &str, slots: i64, runners: &[&str]) -> Machine {
    Machine {
        name: name.into(),
        ssh: format!("user@{name}"),
        slots,
        runners: runners.iter().map(|r| (*r).to_string()).collect(),
        note: None,
        updated_at: String::new(),
        limits: BTreeMap::new(),
        path: None,
    }
}

const NOW: &str = "2026-10-05T12:00:00Z";

fn limited(mut m: Machine, runner: &str, until: &str) -> Machine {
    m.limits.insert(runner.into(), until.into());
    m
}

fn running(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
}

#[test]
fn test_role_follows_the_kind() {
    assert_eq!(role_of("audit", true), "audit");
    assert_eq!(role_of("audit", false), "plan");
    assert_eq!(role_of("research", false), "plan");
    assert_eq!(role_of("work", false), "build");
    assert_eq!(role_of("decision", false), "plan");
    assert_eq!(role_of("story", false), "build");
}

#[test]
fn test_branch_is_the_lead_prefix_the_id_and_the_number() {
    assert_eq!(branch_for("T14", 731), "lead/t14-731");
}

#[test]
fn test_choose_takes_the_most_free_slots_with_the_runner() {
    let ms = [
        machine("alpha", 2, &["claude", "codex"]),
        machine("beta", 4, &["claude"]),
    ];
    let busy = running(&[("alpha", 1), ("beta", 1)]);
    assert_eq!(
        choose(&ms, &busy, "claude", "alpha", NOW).unwrap().name,
        "beta"
    );
    assert_eq!(
        choose(&ms, &busy, "codex", "alpha", NOW).unwrap().name,
        "alpha"
    );
}

#[test]
fn test_choose_never_takes_a_full_machine_or_one_without_the_runner() {
    let ms = [
        machine("alpha", 1, &["claude"]),
        machine("beta", 2, &["codex"]),
    ];
    let busy = running(&[("alpha", 1)]);
    assert!(choose(&ms, &busy, "claude", "alpha", NOW).is_none());
    assert!(choose(&ms, &running(&[("beta", 2)]), "codex", "alpha", NOW).is_none());
}

#[test]
fn test_choose_prefers_this_machine_among_equals() {
    let ms = [
        machine("alpha", 2, &["claude"]),
        machine("beta", 2, &["claude"]),
    ];
    let idle = running(&[]);
    assert_eq!(
        choose(&ms, &idle, "claude", "beta", NOW).unwrap().name,
        "beta"
    );
    assert_eq!(
        choose(&ms, &idle, "claude", "gamma", NOW).unwrap().name,
        "alpha"
    );
}

#[test]
fn test_choose_skips_a_machine_whose_runner_is_limited_until_the_reset() {
    let ms = [
        limited(
            machine("alpha", 2, &["claude", "codex"]),
            "codex",
            "2026-10-07T18:29:00Z",
        ),
        machine("beta", 1, &["codex"]),
    ];
    let idle = running(&[]);
    assert_eq!(
        choose(&ms, &idle, "codex", "alpha", NOW).unwrap().name,
        "beta"
    );
    assert_eq!(
        choose(&ms, &idle, "claude", "alpha", NOW).unwrap().name,
        "alpha"
    );
    assert_eq!(
        choose(&ms, &idle, "codex", "alpha", "2026-10-07T18:29:00Z")
            .unwrap()
            .name,
        "alpha"
    );
}

#[test]
fn test_a_runner_is_limited_when_every_machine_that_has_it_is() {
    let ms = [
        limited(
            machine("alpha", 1, &["claude", "codex"]),
            "codex",
            "2026-10-07T18:29:00Z",
        ),
        limited(
            machine("beta", 1, &["codex"]),
            "codex",
            "2026-10-06T00:00:00Z",
        ),
        limited(
            machine("gamma", 1, &["claude"]),
            "claude",
            "2026-10-09T00:00:00Z",
        ),
        machine("delta", 1, &["claude"]),
    ];
    let out = limited_runners(&ms, NOW);
    assert_eq!(out.len(), 1);
    assert_eq!(out["codex"], "2026-10-06T00:00:00Z");
    assert!(limited_runners(&ms, "2026-10-08T00:00:00Z").is_empty());
}

#[test]
fn test_quote_keeps_plain_words_and_quotes_the_rest() {
    assert_eq!(quote("lead/t14-3"), "lead/t14-3");
    assert_eq!(quote("o/p"), "o/p");
    assert_eq!(quote("two words"), "'two words'");
    assert_eq!(quote("it's"), r"'it'\''s'");
    assert_eq!(quote(""), "''");
    assert_eq!(quote("$HOME"), "'$HOME'");
}

#[test]
fn test_remote_line_finds_docket_and_quotes_each_argument() {
    let line = remote_line(
        &["-p".into(), "o/p".into(), "job".into(), "a b".into()],
        None,
    );
    assert_eq!(
        line,
        "PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:$PATH\" docket -p o/p job 'a b'"
    );
}

#[test]
fn test_a_recorded_path_prefix_replaces_the_default_in_the_remote_command() {
    let mut m = machine("alpha", 1, &["claude"]);
    m.path = Some("/opt/tools/bin:$HOME/bin".into());
    let Via::Ssh(_, path) = Via::of(&m, "beta") else {
        panic!("another machine is reached over ssh");
    };
    assert_eq!(
        remote_line(&["status".into()], path.as_deref()),
        "PATH=\"/opt/tools/bin:$HOME/bin:$PATH\" docket status"
    );
}

#[test]
fn test_via_is_here_for_this_machine_and_ssh_for_another() {
    let a = machine("alpha", 1, &["claude"]);
    assert_eq!(Via::of(&a, "alpha"), Via::Here);
    assert_eq!(Via::of(&a, "beta"), Via::Ssh("user@alpha".into(), None));
    assert_eq!(Via::Here.git_url("/r"), "/r");
    assert_eq!(
        Via::Ssh("user@alpha".into(), None).git_url("/r"),
        "user@alpha:/r"
    );
}

#[test]
fn test_an_ssh_url_with_a_port_keeps_its_form_for_git() {
    let via = Via::Ssh("ssh://user@host:2222".into(), None);
    assert_eq!(via.git_url("/srv/r"), "ssh://user@host:2222/srv/r");
    assert_eq!(
        Via::Ssh("ssh://user@host:2222/".into(), None).git_url("/srv/r"),
        "ssh://user@host:2222/srv/r"
    );
}

/// `git ARGS` in `dir` with an identity, signing off and submodules over files allowed.
fn git_in(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.org"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "protocol.file.allow=always",
        ])
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(
        out.status.success(),
        "git {args:?}: {said}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    said
}

/// A project at `tmp/top` with `top.txt`, pinning the repository `tmp/lib-origin` at `lib`.
fn pinned(tmp: &Path) -> PathBuf {
    let lib = tmp.join("lib-origin");
    let top = tmp.join("top");
    for (dir, file) in [(&lib, "lib.txt"), (&top, "top.txt")] {
        std::fs::create_dir_all(dir).unwrap();
        git_in(dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join(file), "one\n").unwrap();
        git_in(dir, &["add", file]);
        git_in(dir, &["commit", "-q", "-m", "Start"]);
    }
    let from = lib.display().to_string();
    git_in(&top, &["submodule", "add", "-q", &from, "lib"]);
    git_in(&top, &["commit", "-q", "-m", "Pin lib"]);
    top
}

/// A lead's clone of `top`, committing as itself with nothing signed, its submodule left out.
fn lead_clone(tmp: &Path, top: &Path) -> PathBuf {
    let lead = tmp.join("lead");
    let (from, to) = (top.display().to_string(), lead.display().to_string());
    git_in(tmp, &["clone", "-q", &from, &to]);
    identify(&lead);
    lead
}

/// A repository's own identity and no signing, so a commit made through it needs no other config.
fn identify(dir: &Path) {
    for (k, v) in [
        ("user.name", "lead"),
        ("user.email", "lead@example.org"),
        ("commit.gpgsign", "false"),
    ] {
        git_in(dir, &["config", k, v]);
    }
}

/// A job's worktree of `top` on its head, its submodule cloned in plain and checked out at the
/// pinned commit, as an agent sets one up.
fn job_worktree(tmp: &Path, top: &Path) -> PathBuf {
    let wt = tmp.join("job");
    git_in(
        top,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &wt.display().to_string(),
        ],
    );
    let sha = git_in(&wt, &["rev-parse", "HEAD:lib"]);
    std::fs::remove_dir(wt.join("lib")).unwrap();
    let from = top.join("lib").display().to_string();
    git_in(
        tmp,
        &[
            "clone",
            "-q",
            "--no-checkout",
            &from,
            &wt.join("lib").display().to_string(),
        ],
    );
    git_in(&wt.join("lib"), &["checkout", "-q", &sha]);
    wt
}

#[test]
fn test_change_carries_what_changed_inside_a_submodule() {
    let tmp = tempfile::tempdir().unwrap();
    let top = pinned(tmp.path());
    let base = git_in(&top, &["rev-parse", "HEAD"]);
    let wt = tmp.path().join("unset");
    git_in(
        &top,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &wt.display().to_string(),
        ],
    );
    std::fs::write(wt.join("top.txt"), "two\n").unwrap();
    let unset = crate::job::change(&wt, &base).unwrap();
    assert!(unset.patch.contains("+two"), "{}", unset.patch);
    assert!(
        unset.submodules.is_empty(),
        "a submodule not checked out has no change"
    );

    let wt = job_worktree(tmp.path(), &top);
    let pin = git_in(&wt, &["rev-parse", "HEAD:lib"]);
    assert!(crate::job::change(&wt, &base).unwrap().is_empty());
    let lib = wt.join("lib");
    std::fs::write(lib.join("made.txt"), "made\n").unwrap();
    git_in(&lib, &["add", "made.txt"]);
    git_in(&lib, &["commit", "-q", "-m", "Commit inside the job"]);
    std::fs::write(lib.join("lib.txt"), "two\n").unwrap();
    let c = crate::job::change(&wt, &base).unwrap();
    assert_eq!(c.patch, "", "nothing outside the submodule changed");
    let [s] = c.submodules.as_slice() else {
        panic!("{c:?}")
    };
    assert_eq!((s.path.as_str(), s.base.as_str()), ("lib", pin.as_str()));
    assert!(s.change.patch.contains("b/made.txt"), "{}", s.change.patch);
    assert!(s.change.patch.contains("+two"), "{}", s.change.patch);
    assert!(c.text().starts_with(&format!("Submodule lib from {pin}\n")));
}

#[test]
fn test_commit_change_commits_inside_a_plain_clone_of_the_submodule() {
    let tmp = tempfile::tempdir().unwrap();
    let top = pinned(tmp.path());
    let base = git_in(&top, &["rev-parse", "HEAD"]);
    let wt = job_worktree(tmp.path(), &top);
    std::fs::write(wt.join("top.txt"), "two\n").unwrap();
    std::fs::write(wt.join("lib/lib.txt"), "two\n\n").unwrap();
    let change = crate::job::change(&wt, &base).unwrap();
    let lead = lead_clone(tmp.path(), &top);
    std::fs::remove_dir(lead.join("lib")).unwrap();
    let origin = tmp.path().join("lib-origin").display().to_string();
    git_in(&lead, &["clone", "-q", &origin, "lib"]);
    identify(&lead.join("lib"));
    let pin = git_in(&lead.join("lib"), &["rev-parse", "HEAD"]);

    let made = commit_change(&lead, &base, &change, "Change both").unwrap();
    let [(dir, inside)] = made.inside.as_slice() else {
        panic!("{made:?}")
    };
    assert_eq!(dir, &lead.join("lib"));
    assert_eq!(
        git_in(&lead, &["rev-parse", &format!("{}:lib", made.sha)]),
        *inside
    );
    assert_eq!(
        git_in(&lead, &["show", &format!("{}:top.txt", made.sha)]),
        "two"
    );
    let lib = lead.join("lib");
    assert_eq!(git_in(&lib, &["show", &format!("{inside}:lib.txt")]), "two");
    assert_eq!(git_in(&lib, &["rev-parse", &format!("{inside}^")]), pin);
    assert_eq!(
        git_in(&lib, &["log", "-1", "--format=%s", inside]),
        "Change both"
    );
    assert_eq!(
        git_in(&lib, &["rev-parse", "HEAD"]),
        pin,
        "the checkout is left"
    );
    assert_eq!(git_in(&lib, &["status", "--porcelain"]), "");
    assert_eq!(git_in(&lead, &["worktree", "list"]).lines().count(), 1);

    made.keep(&lead, "lead/t1-1").unwrap();
    assert_eq!(git_in(&lib, &["rev-parse", "lead/t1-1"]), *inside);
    assert_eq!(git_in(&lead, &["rev-parse", "lead/t1-1"]), made.sha);
    assert_eq!(git_in(&lib, &["rev-parse", "--git-dir"]), ".git");
}

#[test]
fn test_commit_change_signs_a_submodule_commit_only_when_its_repository_asks() {
    for (gpgsign, signed) in [("true", true), ("false", false)] {
        let tmp = tempfile::tempdir().unwrap();
        let top = pinned(tmp.path());
        let base = git_in(&top, &["rev-parse", "HEAD"]);
        let wt = job_worktree(tmp.path(), &top);
        std::fs::write(wt.join("lib/lib.txt"), "two\n\n").unwrap();
        let change = crate::job::change(&wt, &base).unwrap();
        let lead = lead_clone(tmp.path(), &top);
        std::fs::remove_dir(lead.join("lib")).unwrap();
        let origin = tmp.path().join("lib-origin").display().to_string();
        git_in(&lead, &["clone", "-q", &origin, "lib"]);
        let lib = lead.join("lib");
        identify(&lib);

        let calls = tmp.path().join("signer-calls");
        let signer = tmp.path().join("signer.sh");
        std::fs::write(
            &signer,
            format!(
                "#!/bin/sh\necho call >> {}\ncat >/dev/null\necho '[GNUPG:] SIG_CREATED ' >&2\necho signature\n",
                calls.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&signer, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        git_in(
            &lib,
            &["config", "gpg.program", &signer.display().to_string()],
        );
        git_in(&lib, &["config", "commit.gpgsign", gpgsign]);

        let top_gpgsign = git_in(&lead, &["config", "commit.gpgsign"]);
        assert_eq!(top_gpgsign, "false");
        let made = commit_change(&lead, &base, &change, "Change lib").unwrap();
        let [(_, inside)] = made.inside.as_slice() else {
            panic!("{made:?}")
        };
        let raw = git_in(&lib, &["cat-file", "commit", inside]);
        assert_eq!(raw.contains("gpgsig"), signed, "{raw}");
        assert_eq!(calls.exists(), signed, "commit.gpgsign={gpgsign}");
    }
}

#[test]
fn test_commit_change_refuses_without_the_submodule_and_leaves_no_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let top = pinned(tmp.path());
    let base = git_in(&top, &["rev-parse", "HEAD"]);
    let wt = job_worktree(tmp.path(), &top);
    std::fs::write(wt.join("lib/lib.txt"), "two\n").unwrap();
    let change = crate::job::change(&wt, &base).unwrap();
    let lead = lead_clone(tmp.path(), &top);

    let why = commit_change(&lead, &base, &change, "Change lib").unwrap_err();
    assert!(why.contains("submodule lib: not checked out"), "{why}");

    let mut broken = change.clone();
    broken.submodules.clear();
    broken.patch =
        "diff --git a/top.txt b/top.txt\n--- a/top.txt\n+++ b/top.txt\n@@ -1 +1 @@\n-gone\n+two\n"
            .into();
    let why = commit_change(&lead, &base, &broken, "Change top").unwrap_err();
    assert!(why.contains("git apply"), "{why}");
    assert_eq!(git_in(&lead, &["worktree", "list"]).lines().count(), 1);
    assert_eq!(
        git_in(&lead, &["log", "--all", "--format=%s"]),
        "Pin lib\nStart"
    );
}
