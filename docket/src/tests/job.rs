use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use super::*;

/// Words a rendered brief must never carry: a project, an owner, a path on some machine, a
/// placeholder left unfilled.
/// What a brief must never carry: a home path, an unfilled placeholder, the sample project, an em
/// dash. Real project and owner names are kept out of every tracked file by `docket private check`.
const NAMED: [&str; 7] = [
    "~/", "/home/", "{id}", "{branch}", "{{", "o/sample", "\u{2014}",
];

#[test]
fn test_each_brief_names_its_item_and_branch_and_nothing_local() {
    for (role, _) in BRIEFS {
        let text = brief(role, "T14", "lead/t14-77").unwrap();
        for word in NAMED {
            assert!(!text.contains(word), "{role} carries {word}");
        }
        assert!(
            text.contains("T14") && text.contains("lead/t14-77"),
            "{role}"
        );
        assert!(
            text.contains("DONE <") && text.contains("FAILED <"),
            "{role}"
        );
        assert!(text.contains("NOTE "), "{role}");
        assert!(text.contains("foreground"), "{role}");
    }
    assert!(brief("review", "T14", "b").is_none());
}

#[test]
fn test_report_reads_the_three_forms_from_the_last_line() {
    assert_eq!(
        report("Built it.\nNOTE added the test\nDONE abcdef1\n"),
        (
            Some(Report::Done("abcdef1".into())),
            Some("added the test".into())
        )
    );
    assert_eq!(report("WAITING Q12").0, Some(Report::Waiting("Q12".into())));
    assert_eq!(
        report("```\nFAILED the gates fail on main\n```").0,
        Some(Report::Failed("the gates fail on main".into()))
    );
    assert_eq!(
        report("`DONE 0123abc`").0,
        Some(Report::Done("0123abc".into()))
    );
    for not in [
        "DONE",
        "DONE xyz1234",
        "DONE abc",
        "WAITING T3",
        "WAITING Q",
        "FAILED",
        "DONE abcdef1\nand then a word",
        "",
    ] {
        assert_eq!(report(not).0, None, "{not:?}");
    }
}

#[test]
fn test_claude_final_is_the_longest_result_and_tokens_sum() {
    let events = [
        r#"{"type":"system","subtype":"init"}"#,
        r#"{"type":"result","result":"short","usage":{"output_tokens":2}}"#,
        "not json",
        r#"{"type":"result","result":"NOTE n\nDONE abcdef1","usage":{"output_tokens":5}}"#,
    ]
    .join("\n");
    assert_eq!(
        read_events("claude", &events),
        Read {
            last: Some("NOTE n\nDONE abcdef1".into()),
            tokens: Some(7)
        }
    );
    assert_eq!(read_events("claude", ""), Read::default());
}

#[test]
fn test_codex_tokens_sum_over_completed_turns() {
    let events = [
        r#"{"type":"thread.started","thread_id":"x"}"#,
        r#"{"type":"turn.completed","usage":{"input_tokens":9,"output_tokens":3}}"#,
        r#"{"type":"turn.completed","usage":{"output_tokens":4}}"#,
    ]
    .join("\n");
    let read = read_events("codex", &events);
    assert_eq!(read.tokens, Some(7));
    assert_eq!(read.last, None);
}

#[test]
fn test_state_from_exit_kill_and_liveness() {
    assert_eq!(state(Some("0\n"), false, false), State::Done);
    assert_eq!(state(Some("1"), false, true), State::Failed);
    assert_eq!(state(None, true, true), State::Failed);
    assert_eq!(state(None, false, true), State::Running);
    assert_eq!(state(None, false, false), State::Lost);
}

#[test]
fn test_args_for_each_runner() {
    let (wt, last) = (Path::new("/w"), Path::new("/s/final"));
    assert_eq!(
        args("claude", "m1", Some("high"), "brief", wt, last),
        [
            "-p",
            "brief",
            "--model",
            "m1",
            "--effort",
            "high",
            "--dangerously-skip-permissions",
            "--output-format",
            "stream-json",
            "--verbose"
        ]
    );
    assert_eq!(
        args("codex", "m2", Some("low"), "brief", wt, last),
        [
            "exec",
            "--json",
            "-m",
            "m2",
            "-c",
            "model_reasoning_effort=\"low\"",
            "--dangerously-bypass-approvals-and-sandbox",
            "-C",
            "/w",
            "-o",
            "/s/final",
            "brief"
        ]
    );
    assert!(!args("codex", "m", None, "b", wt, last).contains(&"-c".to_string()));
    assert!(!args("claude", "m", None, "b", wt, last).contains(&"--effort".to_string()));
}

#[test]
fn test_only_the_leads_verbs_are_refused_in_a_job() {
    for verb in LEAD_VERBS {
        assert!(refusal(verb, Some("t14-1")).is_some(), "{verb}");
        assert!(refusal(verb, None).is_none(), "{verb}");
        assert!(refusal(verb, Some("")).is_none(), "{verb}");
    }
    for verb in ["new", "add", "wait", "answer", "edit", "link", "ask"] {
        assert!(refusal(verb, Some("t14-1")).is_none(), "{verb}");
    }
}

#[test]
fn test_name_of_a_branch() {
    assert_eq!(name_of("lead/t14-123"), "lead-t14-123");
    assert_eq!(name_of("/a b/"), "a-b");
    assert_eq!(project_dir(Path::new("/r"), "o/p"), Path::new("/r/o-p"));
}

/// A checkout with one commit and the branch a lead would have pushed, in its own directory.
struct Machine {
    tmp: tempfile::TempDir,
}

impl Machine {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let co = tmp.path().join("p");
        fs::create_dir_all(&co).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["commit", "-q", "--allow-empty", "-m", "first"],
            vec!["branch", "lead/t14-1"],
        ] {
            let ok = Command::new("git")
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(&args)
                .current_dir(&co)
                .status()
                .unwrap();
            assert!(ok.success());
        }
        Self { tmp }
    }

    fn checkout(&self) -> std::path::PathBuf {
        self.tmp.path().join("p")
    }

    fn state(&self) -> std::path::PathBuf {
        self.tmp.path().join("state")
    }

    /// A runner standing in for an agent: it runs the shell given in the worktree it starts in.
    fn runner(&self, body: &str) -> String {
        let path = self.tmp.path().join("runner");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    fn spec(branch: &str) -> Spec {
        Spec {
            project: "o/sample".into(),
            id: "T14".into(),
            branch: branch.into(),
            runner: "claude".into(),
            model: "m1".into(),
            effort: Some("high".into()),
            role: "build".into(),
        }
    }
}

const REPORTS: &str = r#"printf '%s\n' '{"type":"result","result":"NOTE built\nDONE abcdef1","usage":{"output_tokens":5}}'
printf '%s %s\n' "$PWD" "$DOCKET_JOB" > ran"#;

#[test]
fn test_run_starts_the_session_in_its_worktree_and_reads_its_report() {
    let m = Machine::new();
    let program = m.runner(REPORTS);
    let mut started = run(
        &Machine::spec("lead/t14-1"),
        &m.checkout(),
        &m.state(),
        &[("DOCKET_JOB", "lead-t14-1")],
        &program,
    )
    .unwrap();
    assert!(started.child.wait().unwrap().success());
    assert_eq!(started.name, "lead-t14-1");
    let wt = m.tmp.path().join("sample-lead-t14-1");
    let ran = fs::read_to_string(wt.join("ran")).unwrap();
    assert!(ran.ends_with(" lead-t14-1\n"), "{ran}");
    let real = fs::canonicalize(&wt).unwrap();
    assert!(ran.starts_with(&real.display().to_string()), "{ran}");
    let rows = rows(&m.state(), now());
    let [(r, _)] = rows.as_slice() else {
        panic!("{rows:?}")
    };
    assert_eq!(r.state, State::Done);
    assert_eq!(r.report.as_deref(), Some("DONE abcdef1"));
    assert_eq!(r.note.as_deref(), Some("built"));
    assert_eq!(r.last.as_deref(), Some("NOTE built\nDONE abcdef1"));
    assert_eq!(r.tokens, Some(5));
    assert_eq!((r.id.as_str(), r.project.as_str()), ("T14", "o/sample"));
    let brief = fs::read_to_string(started.dir.join("brief.md")).unwrap();
    assert!(brief.contains("T14") && brief.contains("lead/t14-1"));
}

#[test]
fn test_a_runner_that_exits_non_zero_is_failed() {
    let m = Machine::new();
    let program = m.runner("exit 3");
    let mut s = run(
        &Machine::spec("lead/t14-1"),
        &m.checkout(),
        &m.state(),
        &[],
        &program,
    )
    .unwrap();
    s.child.wait().unwrap();
    assert_eq!(row(&s.dir, now()).unwrap().state, State::Failed);
}

#[test]
fn test_run_refuses_a_branch_missing_from_the_checkout() {
    let m = Machine::new();
    let program = m.runner("true");
    let Err(why) = run(
        &Machine::spec("lead/t99-9"),
        &m.checkout(),
        &m.state(),
        &[],
        &program,
    ) else {
        panic!("started without its branch")
    };
    assert!(why.contains("no branch lead/t99-9"), "{why}");
    assert!(!m.state().exists());
}

#[test]
fn test_run_refuses_a_job_already_there_and_an_unknown_runner_or_role() {
    let m = Machine::new();
    let program = m.runner("true");
    let mut s = run(
        &Machine::spec("lead/t14-1"),
        &m.checkout(),
        &m.state(),
        &[],
        &program,
    )
    .unwrap();
    s.child.wait().unwrap();
    let Err(why) = run(
        &Machine::spec("lead/t14-1"),
        &m.checkout(),
        &m.state(),
        &[],
        &program,
    ) else {
        panic!("started twice")
    };
    assert!(why.contains("already here"), "{why}");
    let mut odd = Machine::spec("lead/t14-1");
    odd.runner = "other".into();
    assert!(run(&odd, &m.checkout(), &m.state(), &[], &program).is_err());
    let mut odd = Machine::spec("lead/t14-1");
    odd.role = "review".into();
    assert!(run(&odd, &m.checkout(), &m.state(), &[], &program).is_err());
}

#[test]
fn test_a_job_whose_process_is_gone_without_an_exit_is_lost() {
    let m = Machine::new();
    let program = m.runner("sleep 30");
    let mut s = run(
        &Machine::spec("lead/t14-1"),
        &m.checkout(),
        &m.state(),
        &[],
        &program,
    )
    .unwrap();
    assert_eq!(row(&s.dir, now()).unwrap().state, State::Running);
    let pgid = i32::try_from(s.child.id()).unwrap();
    // SAFETY: kills the test's own job group.
    unsafe {
        libc::killpg(pgid, libc::SIGKILL);
    }
    s.child.wait().unwrap();
    let r = row(&s.dir, now()).unwrap();
    assert_eq!(r.state, State::Lost);
    assert_eq!(r.report, None);
}

#[test]
fn test_kill_leaves_no_process_of_the_group() {
    let m = Machine::new();
    let program = m.runner("sleep 30 &\nsleep 30");
    let mut s = run(
        &Machine::spec("lead/t14-1"),
        &m.checkout(),
        &m.state(),
        &[],
        &program,
    )
    .unwrap();
    let pgid = i32::try_from(s.child.id()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(group_alive(pgid));
    kill(&s.dir).unwrap();
    s.child.wait().unwrap();
    assert!(!group_alive(pgid));
    let r = row(&s.dir, now()).unwrap();
    assert_eq!(r.state, State::Failed);
    assert_eq!(r.report.as_deref(), Some("FAILED killed"));
}

#[test]
fn test_find_and_tail() {
    let m = Machine::new();
    let program = m.runner(REPORTS);
    let mut s = run(
        &Machine::spec("lead/t14-1"),
        &m.checkout(),
        &m.state(),
        &[],
        &program,
    )
    .unwrap();
    s.child.wait().unwrap();
    assert_eq!(find(&m.state(), "lead-t14-1", None).unwrap(), s.dir);
    assert!(find(&m.state(), "lead-t14-1", Some("o/other")).is_err());
    assert!(find(&m.state(), "nope", None).is_err());
    assert!(tail(&s.dir, 1).contains("DONE abcdef1"));
}
