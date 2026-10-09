use serde_json::json;

use super::*;

#[test]
fn test_shas_finds_hex_words_of_seven_or_more() {
    assert_eq!(
        shas("landed abc1234, see 12345 and deadbeefcafe"),
        ["abc1234", "deadbeefcafe"]
    );
}

fn git_in(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit_file(dir: &Path, name: &str, body: &str) -> String {
    std::fs::write(dir.join(name), body).unwrap();
    git_in(dir, &["add", "."]);
    git_in(dir, &["commit", "-m", name]);
    git_in(dir, &["rev-parse", "HEAD"])
}

#[test]
fn test_sha_state_is_measured_against_the_integration_ref_not_head() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git_in(dir, &["init", "-b", "main"]);
    commit_file(dir, "base", "base");
    let merged = commit_file(dir, "merged", "m");
    git_in(dir, &["checkout", "-b", "audit/x"]);
    let unmerged = commit_file(dir, "unmerged", "u");
    git_in(dir, &["checkout", "-b", "lost"]);
    let lost = commit_file(dir, "lost", "l");
    git_in(dir, &["checkout", "audit/x"]);
    git_in(dir, &["branch", "-D", "lost"]);
    git_in(dir, &["checkout", "main"]);
    let picked = {
        git_in(dir, &["checkout", "-b", "feature"]);
        let c = commit_file(dir, "picked", "p");
        git_in(dir, &["checkout", "main"]);
        git_in(dir, &["cherry-pick", &c]);
        c
    };
    git_in(dir, &["checkout", "audit/x"]);
    let dirs = [dir.to_path_buf()];
    let state = |sha: &str| sha_state(&dirs, sha, Some("main"));
    assert_eq!(state(&merged[..10]), ShaState::OnIntegration);
    assert_eq!(
        state(&unmerged[..10]),
        ShaState::Unmerged(vec!["audit/x".into()])
    );
    assert_eq!(state(&lost[..10]), ShaState::NoBranch);
    assert_eq!(state("0123456789abcdef"), ShaState::Gone);
    // The patch is on main under another sha, so a rewrite is not reported as lost work.
    assert_eq!(state(&picked[..10]), ShaState::OnIntegration);
    // With no integration ref the checkout's own HEAD is the measure.
    assert_eq!(sha_state(&dirs, &merged, None), ShaState::OnIntegration);
}

#[test]
fn test_closed_work_is_sorted_by_its_best_sha() {
    let done = [
        json!({"id": "B1", "state": "done", "kind": "work", "resolution": "abc1234 def5678"}),
        json!({"id": "B2", "state": "done", "kind": "work", "resolution": "def5678"}),
        json!({"id": "B3", "state": "done", "kind": "work", "resolution": "no sha"}),
        json!({"id": "Q1", "state": "done", "kind": "decision", "resolution": "opened B1"}),
    ];
    let states = closed_states(&done, |s| match s {
        "abc1234" => ShaState::OnIntegration,
        _ => ShaState::Unmerged(vec!["audit/x".into()]),
    });
    assert_eq!(
        states,
        [
            ("B1".to_string(), ShaState::OnIntegration),
            ("B2".to_string(), ShaState::Unmerged(vec!["audit/x".into()])),
            ("B3".to_string(), ShaState::Gone),
        ]
    );
}

#[test]
fn test_a_stored_row_is_work_by_its_type_whatever_its_key() {
    let kind = |t: &str, key: &str| closed_kind(&json!({"key": key, "item_type": t}));
    assert_eq!(kind("task", "T"), "work");
    assert_eq!(kind("bug", "ZQ"), "work");
    assert_eq!(kind("question", "Q"), "other");
    assert_eq!(kind("plan", "T"), "other");
    assert_eq!(kind("investigation", "I"), "other");
}

#[test]
fn test_breakdown_lines_for_a_package() {
    let pkg =
        json!({"progress": {"done": 1, "total": 3, "live": 1}, "touches": ["a.rs", "b/c.rs"]});
    assert_eq!(
        breakdown_lines(&pkg),
        [
            "members: 1 of 3 done, 1 live",
            "touches, 2 files: a.rs, b/c.rs"
        ]
    );
}

#[test]
fn test_sections_are_found_by_word_and_open_counts_the_flow_words() {
    let rows: Vec<Value> = [
        "ready", "blocked", "blocked", "parked", "ready", "done", "dropped",
    ]
    .iter()
    .enumerate()
    .map(|(i, w)| serde_json::json!({"id": format!("T{i}"), "word": w}))
    .collect();
    let sections = sections_of(&rows);
    assert_eq!(section(&sections, "done").len(), 1);
    assert_eq!(section(&sections, "done")[0]["id"], "T5");
    assert_eq!(section(&sections, "blocked").len(), 2);
    assert_eq!(open_count(&sections), 5);
    assert_eq!(section(&sections, "inbox").len(), 0);
}

fn file(path: &str, lines: usize) -> (String, usize) {
    (path.to_string(), lines)
}

#[test]
fn test_a_cited_line_past_the_end_of_the_file_is_reported() {
    let found = [file("/r/src/hooks/useSplits.ts", 59)];
    assert_eq!(
        judge("cites_file", Some(132), &found, false),
        Verdict::PastEnd(59)
    );
    assert_eq!(
        judge("cites_file", Some(59), &found, false),
        Verdict::Resolves
    );
    assert_eq!(judge("cites_file", None, &found, false), Verdict::Resolves);
}

#[test]
fn test_a_line_is_judged_against_the_longest_file_that_matches() {
    let found = [file("/r/a/lib.rs", 10), file("/r/b/lib.rs", 200)];
    assert_eq!(
        judge("cites_file", Some(150), &found, false),
        Verdict::Resolves
    );
}

#[test]
fn test_a_cited_test_that_resolves_only_under_benches_is_reported() {
    let found = [file("/r/benches/parse.rs", 40)];
    assert_eq!(
        judge("cites_test", Some(5), &found, false),
        Verdict::Benches
    );
    assert_eq!(
        judge("cites_file", Some(5), &found, false),
        Verdict::Resolves
    );
    let both = [
        file("/r/benches/parse.rs", 40),
        file("/r/tests/parse.rs", 40),
    ];
    assert_eq!(judge("cites_test", None, &both, false), Verdict::Resolves);
}

#[test]
fn test_nothing_found_is_missing_unless_a_dependency_holds_it() {
    assert_eq!(judge("cites_file", None, &[], false), Verdict::Missing);
    assert_eq!(judge("cites_file", Some(9), &[], true), Verdict::Resolves);
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.email=a@b.c", "-c", "user.name=t"])
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn test_index_lists_tracked_files_and_not_worktrees_or_untracked_copies() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let repo = root.join("app");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/kept.ts"), "x").unwrap();
    std::fs::write(repo.join("src/gone.ts"), "x").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "one"]);
    git(&repo, &["worktree", "add", "-q", "../app-side"]);
    git(&repo, &["rm", "-q", "src/gone.ts"]);
    git(&repo, &["commit", "-qm", "two"]);
    std::fs::write(repo.join("src/loose.ts"), "x").unwrap();

    let roots = vec![root.to_string_lossy().into_owned()];
    let mut index = Index::default();
    assert_ne!(index.find(&roots, "src/kept.ts").0.len(), 0);
    assert_eq!(index.find(&roots, "src/gone.ts").0.len(), 0);
    assert_eq!(index.find(&roots, "src/loose.ts").0.len(), 0);
}

#[test]
fn test_cited_branches_skip_wildcards_and_scripts() {
    let body = "Parked on `audit/z9-1234`; also audit/z9-77. Not audit/z1xx, audit/z9-*, \
                audit/z9-$n, audit-status.sh or audit/ alone.";
    assert_eq!(cited_branches(body), ["audit/z9-1234", "audit/z9-77"]);
}

#[test]
fn test_gone_branches_are_those_no_repository_has() {
    let dir = std::env::temp_dir().join(format!("docket-branches-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| assert!(local::git(args, &dir).is_some(), "git {args:?}");
    run(&["init", "-q"]);
    run(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@example.invalid",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "x",
    ]);
    run(&["branch", "audit/z9-1234"]);
    let rows = vec![
        ("Z1".to_string(), "audit/z9-1234".to_string()),
        ("Z1".into(), "audit/z9-5678".into()),
    ];
    let gone = gone_branches(&rows, std::slice::from_ref(&dir));
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(gone, [("Z1".to_string(), "audit/z9-5678".to_string())]);
}

#[test]
fn test_closes_another_item_reads_the_id_a_commit_subject_closes() {
    assert_eq!(closes_another("I20", "Close I19 and rewrite"), Some("I19"));
    assert_eq!(closes_another("I20", "Close I20 with the fix"), None);
    assert_eq!(closes_another("I20", "Record I19 as done"), None);
    assert_eq!(closes_another("I2", "Close I20"), Some("I20"));
}

#[test]
fn test_misattributed_lists_done_items_whose_commit_closes_another_item() {
    let done = [
        json!({"id": "B1", "state": "done", "kind": "work", "resolution": "abc1234"}),
        json!({"id": "B2", "state": "done", "kind": "work", "resolution": "def5678 and abc1234"}),
        json!({"id": "B3", "state": "done", "kind": "work", "resolution": "no sha"}),
    ];
    let subject = |sha: &str| match sha {
        "abc1234" => Some("Close B1 with the fix".to_string()),
        "def5678" => Some("Tidy the parser".to_string()),
        _ => None,
    };
    assert_eq!(
        misattributed(&done, &subject),
        [("B2".to_string(), "B1".to_string())]
    );
}

#[test]
fn test_area_lines_give_the_description_then_the_priority() {
    let area = json!({"name": "kites", "description": "Things that fly", "priority": "high"});
    assert_eq!(area_lines(&area), ["Things that fly", "priority: high"]);
    assert_eq!(
        area_lines(&json!({"name": "kites", "description": "", "priority": null})).len(),
        0
    );
    assert_eq!(area_lines(&Value::Null).len(), 0);
}

#[test]
fn test_prune_removes_stale_job_branches_without_unique_commits_and_names_the_rest() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git_in(dir, &["init", "-b", "main"]);
    commit_file(dir, "base", "base");
    git_in(dir, &["checkout", "-b", "lead/a1-100"]);
    commit_file(dir, "one", "1");
    git_in(dir, &["checkout", "main"]);
    git_in(dir, &["merge", "--ff-only", "lead/a1-100"]);
    git_in(dir, &["checkout", "-b", "lead/a2-200"]);
    let second = commit_file(dir, "two", "2");
    git_in(dir, &["checkout", "main"]);
    git_in(dir, &["cherry-pick", &second]);
    git_in(dir, &["checkout", "-b", "lead/a3-300"]);
    commit_file(dir, "three", "3");
    git_in(dir, &["checkout", "main"]);
    git_in(dir, &["checkout", "-b", "lead/a4-400"]);
    commit_file(dir, "four", "4");
    git_in(dir, &["checkout", "main"]);

    let dirs = [dir.to_path_buf()];
    let state = |item: &str| match item {
        "A1" | "A2" => Some("done".to_string()),
        _ => Some("under way".to_string()),
    };
    // A4 is claimed by a live job; A3 is open and named by nothing.
    let named: HashSet<String> = ["lead/a4-400".to_string()].into();
    let found = stale_job_branches(&dirs, Some("main"), state, &named);
    let summary: Vec<(&str, Option<usize>)> =
        found.iter().map(|b| (b.name.as_str(), b.unique)).collect();
    assert_eq!(
        summary,
        [
            ("lead/a1-100", Some(0)),
            ("lead/a2-200", Some(0)),
            ("lead/a3-300", Some(1))
        ]
    );

    let kept = prune_job_branches(&dirs, &found);
    assert_eq!(kept, ["lead/a3-300"]);
    let left = git_in(dir, &["branch", "--format=%(refname:short)"]);
    assert_eq!(
        left.lines().collect::<Vec<_>>(),
        ["lead/a3-300", "lead/a4-400", "main"]
    );
}

#[test]
fn test_prune_keeps_a_job_branch_in_a_repository_without_the_work_ref() {
    let tmp = tempfile::tempdir().unwrap();
    let first = tmp.path().join("first");
    let second = tmp.path().join("second");
    for d in [&first, &second] {
        std::fs::create_dir(d).unwrap();
    }
    git_in(&first, &["init", "-b", "main"]);
    commit_file(&first, "base", "base");
    git_in(&second, &["init", "-b", "trunk"]);
    commit_file(&second, "base", "base");
    git_in(&second, &["checkout", "-b", "lead/x9-1"]);
    commit_file(&second, "work", "work");
    git_in(&second, &["checkout", "trunk"]);

    let dirs = [first, second.clone()];
    let state = |_: &str| Some("done".to_string());
    let found = stale_job_branches(&dirs, Some("main"), state, &HashSet::new());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].unique, None);

    let kept = prune_job_branches(&dirs, &found);
    assert_eq!(kept, ["lead/x9-1"]);
    let left = git_in(&second, &["branch", "--format=%(refname:short)"]);
    assert!(left.lines().any(|l| l == "lead/x9-1"));
}
