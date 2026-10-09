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
            text.contains("    DONE\n") && text.contains("FAILED <"),
            "{role}"
        );
        assert!(text.contains("NOTE "), "{role}");
        assert!(text.contains("foreground"), "{role}");
    }
    assert!(brief("review", "T14", "b").is_none());
}

#[test]
fn test_each_brief_keeps_the_private_docket_out_of_the_repository() {
    for role in ["build", "plan"] {
        let text = brief(role, "T14", "lead/t14-7").unwrap();
        assert!(!text.contains("private check"), "{role}");
        assert!(text.contains("the lead checks the change"), "{role}");
    }
    assert!(
        brief("audit", "A3", "lead/a3-7")
            .unwrap()
            .contains("commit nothing")
    );
}

#[test]
fn test_no_brief_lets_a_job_commit() {
    for (role, _) in BRIEFS {
        let text = brief(role, "T14", "lead/t14-7").unwrap();
        assert!(!text.contains("Commit on"), "{role}");
        assert!(
            text.contains("never commit")
                || text.contains("commit nothing")
                || text.contains("never committed"),
            "{role}"
        );
    }
}

#[test]
fn test_message_is_the_last_message_line() {
    assert_eq!(
        message("MESSAGE first\nMESSAGE Fix the sync\nDONE"),
        Some("Fix the sync".into())
    );
    assert_eq!(message("NOTE x\nDONE"), None);
    assert_eq!(message("MESSAGE \nDONE"), None);
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
    assert_eq!(
        report("NOTE x\nMESSAGE y\nDONE").0,
        Some(Report::Done(String::new()))
    );
    assert_eq!(Report::Done(String::new()).line(), "DONE");
    for not in [
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
            tokens: Some(7),
            error: None,
            ..Read::default()
        }
    );
    assert_eq!(read_events("claude", ""), Read::default());
}

#[test]
fn test_input_and_output_tokens_and_the_reported_cost_sum_over_results() {
    let events = [
        r#"{"type":"result","result":"a","total_cost_usd":0.25,"usage":{"input_tokens":10,"output_tokens":2}}"#,
        r#"{"type":"result","result":"b","total_cost_usd":0.5,"usage":{"input_tokens":4,"output_tokens":5}}"#,
    ]
    .join("\n");
    let read = read_events("claude", &events);
    assert_eq!(
        (read.tokens_in, read.tokens, read.cost),
        (Some(14), Some(7), Some(0.75))
    );
    let codex = r#"{"type":"turn.completed","usage":{"input_tokens":9,"output_tokens":3}}"#;
    let read = read_events("codex", codex);
    assert_eq!((read.tokens_in, read.cost), (Some(9), None));
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
fn test_codex_error_events_are_kept_for_a_limit_to_be_read() {
    let events = [
        r#"{"type":"error","message":"weekly usage limit reached, resets 2026-10-07 18:29"}"#,
        r#"{"type":"turn.failed","error":{"message":"weekly usage limit reached, resets 2026-10-07 18:29"}}"#,
    ]
    .join("\n");
    let read = read_events("codex", &events);
    assert_eq!(
        read.error
            .as_deref()
            .and_then(docket_core::machine::reset_of),
        Some("2026-10-07T18:29:00Z".into())
    );
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
fn test_a_job_reads_the_releases_and_areas_and_never_changes_them() {
    let why = owner_plan_refusal("releases", "ship", Some("t14-1")).unwrap();
    assert!(why.contains("the owner's"), "{why}");
    assert!(owner_plan_refusal("releases", "add", Some("t14-1")).is_some());
    assert!(owner_plan_refusal("releases", "list", Some("t14-1")).is_none());
    assert!(owner_plan_refusal("releases", "ship", None).is_none());
    for action in ["add", "edit", "move", "rm"] {
        let why = owner_plan_refusal("areas", action, Some("t14-1")).unwrap();
        assert!(
            why.starts_with(&format!("docket areas {action} is refused")),
            "{why}"
        );
    }
    assert!(owner_plan_refusal("areas", "list", Some("t14-1")).is_none());
}

#[test]
fn test_a_job_installs_nothing_outside_its_worktree() {
    let why = install_refusal("install", Some("t1-1")).unwrap();
    assert!(why.contains("t1-1"), "{why}");
    assert!(install_refusal("install", Some("")).is_none());
    assert!(install_refusal("diff", Some("t1-1")).is_none());
    assert!(install_refusal("install", None).is_none());
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
        let m = Self {
            tmp: tempfile::tempdir().unwrap(),
        };
        m.repo("p");
        m
    }

    /// A repository at `name` in the machine's directory, with one commit and the lead's branch.
    fn repo(&self, name: &str) -> std::path::PathBuf {
        let co = self.tmp.path().join(name);
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
        co
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
            provision: None,
            repo: None,
            launch: Launch {
                reporter: None,
                cap: Cap::Rlimit(u32::MAX),
            },
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
    let wt = m.tmp.path().join("sample-slot1");
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
fn test_a_job_in_a_named_repository_takes_a_slot_named_for_it_and_records_it() {
    let m = Machine::new();
    let spec = Spec {
        repo: Some("toys/kites".into()),
        ..Machine::spec("lead/t14-1")
    };
    let mut started = run(&spec, &m.checkout(), &m.state(), &[], &m.runner(REPORTS)).unwrap();
    started.child.wait().unwrap();
    assert_eq!(
        started.worktree,
        m.tmp.path().join("sample-toys-kites-slot1")
    );
    let r = row(&started.dir, now()).unwrap();
    assert_eq!(r.repo.as_deref(), Some("toys/kites"));
}

#[test]
fn test_a_repository_sits_in_a_job_directory_where_it_sits_under_the_root() {
    assert_eq!(place_of("kites"), Path::new("kites"));
    assert_eq!(place_of("toys/kites"), Path::new("toys/kites"));
    assert_eq!(place_of("@acme/lib"), Path::new("lib"));
    assert_eq!(place_of("@acme/lib/core"), Path::new("lib/core"));
    assert_eq!(place_of("."), Path::new(""));
}

#[test]
fn test_a_job_in_several_repositories_holds_a_worktree_of_each_on_its_branch_in_one_directory() {
    let m = Machine::new();
    let lanterns = m.repo("lanterns");
    let spec = Spec {
        provision: Some("echo once >> provisioned".into()),
        ..Machine::spec("lead/t14-1")
    };
    let program = m.runner(r#"printf '%s %s\n' "$PWD" "$DOCKET_BRANCH" > ran"#);
    let repos = [
        ("toys/kites".to_string(), m.checkout()),
        ("@acme/lanterns".to_string(), lanterns.clone()),
    ];
    let mut started = run_in_repos(&spec, &repos, &m.state(), &[], &program).unwrap();
    started.child.wait().unwrap();
    let tree = started.dir.join("tree");
    assert_eq!(started.worktree, tree);
    assert_eq!(
        fs::read_to_string(tree.join("provisioned")).unwrap(),
        "once\n"
    );
    let ran = fs::read_to_string(tree.join("ran")).unwrap();
    assert!(ran.ends_with(" lead/t14-1\n"), "{ran}");
    for at in ["toys/kites", "lanterns"] {
        let branch = git(&tree.join(at), &["branch", "--show-current"]).unwrap();
        assert_eq!(branch, "lead/t14-1", "{at}");
    }
    let r = row(&started.dir, now()).unwrap();
    assert_eq!(
        r.repos,
        ["@acme/lanterns", "toys/kites"],
        "the shallower first, as the directory was made"
    );
    fs::write(tree.join("lanterns/new.txt"), "lit\n").unwrap();
    let Diff::Several(changes) = diff(&started.dir).unwrap() else {
        panic!("one change for a job in several repositories")
    };
    assert_eq!(changes.len(), 2);
    assert!(changes[0].change.patch.contains("new.txt"), "{changes:?}");
    assert!(changes[1].change.is_empty());

    remove(&started.dir, false).unwrap();
    assert!(!started.dir.exists());
    for co in [m.checkout(), lanterns] {
        assert_eq!(git(&co, &["worktree", "list"]).unwrap().lines().count(), 1);
        assert_eq!(git(&co, &["branch", "--list", "lead/t14-1"]).unwrap(), "");
    }
}

#[test]
fn test_a_job_whose_repositories_sit_at_one_place_is_refused_and_leaves_nothing() {
    let m = Machine::new();
    let other = m.repo("other");
    let repos = [
        ("lib".to_string(), m.checkout()),
        ("@acme/lib".to_string(), other),
    ];
    let program = m.runner("true");
    let Err(why) = run_in_repos(
        &Machine::spec("lead/t14-1"),
        &repos,
        &m.state(),
        &[],
        &program,
    ) else {
        panic!("started two repositories at one place")
    };
    assert!(why.contains("both sit at lib"), "{why}");
    assert_eq!(rows(&m.state(), now()).len(), 0);
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

#[test]
fn test_a_brief_keeps_a_routine_choice_out_of_the_questions() {
    for role in ["build", "plan"] {
        let text = brief(role, "T14", "lead/t14-7").unwrap();
        assert!(text.contains("routine choice"), "{role}");
        assert!(text.contains("see in the product"), "{role}");
        assert!(text.contains("--carried-by"), "{role}");
    }
}

#[test]
fn test_a_job_files_nothing_without_naming_its_release_and_its_area_or_plan() {
    let why = unfiled("new", Some("lead-t14-7"), None, Some("kites"), None).unwrap();
    assert!(why.contains("--release"), "{why}");
    assert_eq!(
        unfiled("add", Some("lead-t14-7"), None, Some("kites"), None),
        Some(why.replace("new", "add"))
    );
    let why = unfiled("new", Some("lead-t1-1"), Some("current"), None, None).unwrap();
    assert!(why.contains("--area"), "{why}");
    assert!(why.contains("--parent"), "{why}");
    assert_eq!(
        unfiled("add", Some("lead-t14-7"), Some(""), Some("kites"), None),
        None
    );
    assert_eq!(
        unfiled("new", Some("lead-t14-7"), Some("current"), None, Some("A3")),
        None
    );
    assert_eq!(unfiled("new", None, None, None, None), None);
    assert_eq!(unfiled("new", Some(""), None, None, None), None);
}

#[test]
fn test_every_brief_that_files_names_the_area_or_the_plan_beside_the_release() {
    for role in ["build", "plan", "audit"] {
        let text = brief(role, "T14", "lead/t14-7").unwrap();
        assert!(text.contains("--area"), "{role}");
    }
    for role in ["build", "plan"] {
        let text = brief(role, "T14", "lead/t14-7").unwrap();
        assert!(text.contains("--parent"), "{role}");
    }
}

#[test]
fn test_every_brief_triages_a_filing_by_what_it_is() {
    for (role, _) in BRIEFS {
        let text = brief(role, "T14", "lead/t14-7").unwrap();
        assert!(!text.contains("Its nature"), "{role}");
        assert!(text.contains("takes the plan's release"), "{role}");
        assert!(text.contains("**Release.**"), "{role}");
    }
}

#[test]
fn test_an_observation_names_the_repository_it_is_about_in_brackets() {
    assert_eq!(
        observation("[@acme/lib] the cache never expires"),
        (Some("@acme/lib"), "the cache never expires")
    );
    assert_eq!(observation("[web] one"), (Some("web"), "one"));
    assert_eq!(observation("no repo here"), (None, "no repo here"));
    assert_eq!(observation("[../out] x"), (None, "[../out] x"));
    assert_eq!(observation("[] x"), (None, "[] x"));
    assert_eq!(
        observed("j", &["[web] one".into(), "two".into()]),
        "**Observations, from the job j.**\n\n- [web] one\n- two"
    );
}

#[test]
fn test_observations_are_every_observe_line_and_read_as_a_list() {
    let text = "OBSERVE the retry sleeps a fixed second\nNOTE built it\nOBSERVE  the log names no host \nOBSERVE \nDONE";
    assert_eq!(
        observations(text),
        ["the retry sleeps a fixed second", "the log names no host"]
    );
    assert_eq!(report(text).1, Some("built it".into()));
    assert_eq!(observations("NOTE x\nDONE").len(), 0);
    assert_eq!(
        observed("lead-t14-7", &["one".into(), "two".into()]),
        "**Observations, from the job lead-t14-7.**\n\n- one\n- two"
    );
}

#[test]
fn test_a_side_finding_below_the_bar_is_an_observation() {
    for role in ["build", "plan"] {
        let text = brief(role, "T14", "lead/t14-7").unwrap();
        assert!(text.contains("    OBSERVE "), "{role}");
        assert!(text.contains("critical or high"), "{role}");
        assert!(!text.contains("docket add"), "{role}");
    }
}

#[test]
fn test_an_audit_files_only_critical_and_high_gaps() {
    let text = brief("audit", "A3", "lead/a3-7").unwrap();
    assert!(text.contains("Only a critical or high gap is filed"));
    assert!(text.contains("normal or low gap is an `OBSERVE` line"));
    assert!(
        text.contains("outside the plan is an `OBSERVE` line too, unless it is critical or high")
    );
    assert!(text.contains("    OBSERVE "));
    assert!(!text.contains("at the priority it earns"));
}

#[test]
fn test_run_provisions_the_worktree_before_the_runner_starts() {
    let m = Machine::new();
    let program = m.runner("test -f provisioned && echo yes > saw");
    let mut spec = Machine::spec("lead/t14-1");
    spec.provision = Some("touch provisioned".into());
    let mut started = run(&spec, &m.checkout(), &m.state(), &[], &program).unwrap();
    assert!(started.child.wait().unwrap().success());
    let saw = fs::read_to_string(started.worktree.join("saw")).unwrap();
    assert_eq!(saw, "yes\n");
}

#[test]
fn test_run_refuses_and_leaves_nothing_when_provisioning_fails() {
    let m = Machine::new();
    let program = m.runner("touch ran");
    let mut spec = Machine::spec("lead/t14-1");
    spec.provision = Some("echo no space >&2; exit 1".into());
    let Err(why) = run(&spec, &m.checkout(), &m.state(), &[], &program) else {
        panic!("started")
    };
    assert!(
        why.contains("provision") && why.contains("no space"),
        "{why}"
    );
    assert!(!m.tmp.path().join("sample-slot1").exists());
    assert_eq!(rows(&m.state(), now()).len(), 0);
}

#[test]
fn test_marker_lines_may_be_wrapped_in_bold() {
    assert_eq!(
        message("**MESSAGE** Fix the sync\nDONE"),
        Some("Fix the sync".into())
    );
    assert_eq!(observations("**OBSERVE** x\nOBSERVE y"), ["x", "y"]);
    assert_eq!(report("**NOTE** built it\nDONE").1, Some("built it".into()));
}

#[test]
fn test_the_default_model_passes_no_model_flag() {
    let (wt, last) = (Path::new("/w"), Path::new("/l"));
    for (runner, flag) in [("claude", "--model"), ("codex", "-m")] {
        let with = args(runner, "m1", None, "b", wt, last);
        assert!(with.contains(&flag.to_string()), "{with:?}");
        let without = args(runner, DEFAULT_MODEL, None, "b", wt, last);
        assert!(!without.contains(&flag.to_string()), "{without:?}");
        assert!(!without.contains(&DEFAULT_MODEL.to_string()), "{without:?}");
    }
}

#[test]
fn test_removing_a_lead_job_leaves_the_checkout_and_its_branch() {
    let m = Machine::new();
    let dir = project_dir(&m.state(), "o/sample").join("lead-1");
    fs::create_dir_all(&dir).unwrap();
    let meta = Meta {
        name: "lead-1".into(),
        project: "o/sample".into(),
        id: String::new(),
        branch: "main".into(),
        runner: "claude".into(),
        model: "m1".into(),
        effort: None,
        role: "lead".into(),
        worktree: m.checkout().display().to_string(),
        started: now(),
        base: None,
        repo: None,
        parts: Vec::new(),
    };
    fs::write(dir.join("meta.json"), serde_json::to_string(&meta).unwrap()).unwrap();
    fs::write(dir.join("exit"), "0").unwrap();
    remove(&dir, false).unwrap();
    assert!(!dir.exists());
    assert!(m.checkout().join(".git").exists());
    let branch = git(&m.checkout(), &["branch", "--show-current"]).unwrap();
    assert_eq!(branch, "main");
}

#[test]
fn test_a_removed_job_parks_its_worktree_and_the_next_job_reuses_it() {
    let m = Machine::new();
    let program = m.runner("true");
    let mut first = run(
        &Machine::spec("lead/t14-1"),
        &m.checkout(),
        &m.state(),
        &[],
        &program,
    )
    .unwrap();
    first.child.wait().unwrap();
    fs::create_dir_all(first.worktree.join("target")).unwrap();
    fs::write(first.worktree.join("target/built"), "x").unwrap();
    fs::write(first.worktree.join(".gitignore"), "target\n").unwrap();
    fs::write(first.worktree.join("stray"), "x").unwrap();
    remove(&first.dir, false).unwrap();
    assert!(
        first.worktree.is_dir(),
        "the worktree is parked, not removed"
    );
    assert!(!first.worktree.join("stray").exists());
    assert!(first.worktree.join("target/built").exists());

    git(&m.checkout(), &["branch", "lead/t15-2"]).unwrap();
    let mut second = run(
        &Machine::spec("lead/t15-2"),
        &m.checkout(),
        &m.state(),
        &[],
        &program,
    )
    .unwrap();
    second.child.wait().unwrap();
    assert_eq!(second.worktree, first.worktree);
    let on = git(&second.worktree, &["branch", "--show-current"]).unwrap();
    assert_eq!(on, "lead/t15-2");
    assert!(second.worktree.join("target/built").exists());
}

#[test]
fn test_a_running_jobs_worktree_is_not_handed_to_another_job() {
    let m = Machine::new();
    let program = m.runner("true");
    let one = run(
        &Machine::spec("lead/t14-1"),
        &m.checkout(),
        &m.state(),
        &[],
        &program,
    )
    .unwrap();
    git(&m.checkout(), &["branch", "lead/t15-2"]).unwrap();
    let two = run(
        &Machine::spec("lead/t15-2"),
        &m.checkout(),
        &m.state(),
        &[],
        &program,
    )
    .unwrap();
    assert_ne!(one.worktree, two.worktree);
}

/// A reporter standing in for `docket`: it records its arguments and whether the exit file was
/// already there, then exits as `$REPORTER_EXIT`.
const REPORTER: &str = r#"#!/bin/sh
echo "$@" > "$(dirname "$3")/reported-args"
[ -f "$3/exit" ] && echo seen > "$(dirname "$3")/exit-first"
exit "${REPORTER_EXIT:-0}"
"#;

fn reporter(m: &Machine) -> std::path::PathBuf {
    let path = m.tmp.path().join("reporter");
    fs::write(&path, REPORTER).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn test_a_job_that_ends_writes_its_exit_then_calls_the_reporter_with_its_directory() {
    let m = Machine::new();
    let mut spec = Machine::spec("lead/t14-1");
    spec.launch.reporter = Some(reporter(&m));
    let mut started = run(&spec, &m.checkout(), &m.state(), &[], &m.runner("exit 3")).unwrap();
    assert!(started.child.wait().unwrap().success());
    assert_eq!(
        fs::read_to_string(started.dir.join("exit")).unwrap().trim(),
        "3"
    );
    let parent = started.dir.parent().unwrap();
    let args = fs::read_to_string(parent.join("reported-args")).unwrap();
    assert_eq!(args.trim(), format!("job report {}", started.dir.display()));
    assert!(
        parent.join("exit-first").exists(),
        "the exit file came after the report"
    );
}

#[test]
fn test_a_reporter_that_fails_changes_nothing_about_the_job() {
    let m = Machine::new();
    let mut spec = Machine::spec("lead/t14-1");
    spec.launch.reporter = Some(reporter(&m));
    let program = m.runner(r#"printf '%s\n' '{"type":"result","result":"NOTE n\nDONE abc","usage":{"output_tokens":1}}'"#);
    let mut started = run(
        &spec,
        &m.checkout(),
        &m.state(),
        &[("REPORTER_EXIT", "1")],
        &program,
    )
    .unwrap();
    assert!(started.child.wait().unwrap().success());
    let rows = rows(&m.state(), now());
    let [(r, _)] = rows.as_slice() else {
        panic!("{rows:?}")
    };
    assert_eq!(r.state, State::Done);
}

#[test]
fn test_a_job_without_a_reporter_calls_nothing() {
    let m = Machine::new();
    assert!(Launch::default().reporter.is_none());
    let mut started = run(
        &Machine::spec("lead/t14-1"),
        &m.checkout(),
        &m.state(),
        &[],
        &m.runner("true"),
    )
    .unwrap();
    assert!(started.child.wait().unwrap().success());
    assert!(started.dir.join("exit").exists());
}

#[test]
fn test_the_process_cap_is_systemd_where_a_user_manager_runs_and_rlimit_where_not() {
    assert_eq!(Cap::choose(true, true, 2000), Cap::Systemd(2000));
    assert_eq!(Cap::choose(true, false, 2000), Cap::Rlimit(2000));
    assert_eq!(Cap::choose(false, true, 2000), Cap::Rlimit(2000));
}

#[test]
fn test_a_systemd_cap_runs_the_wrapper_inside_a_scope_with_a_task_limit() {
    let words = vec!["sh".to_string(), "-c".to_string(), "x".to_string()];
    let (program, args) = Cap::Systemd(2000).command(&words);
    assert_eq!(program, "systemd-run");
    assert_eq!(
        args,
        [
            "--user",
            "--scope",
            "--quiet",
            "-p",
            "TasksMax=2000",
            "--",
            "sh",
            "-c",
            "x"
        ]
    );
    let (program, args) = Cap::Rlimit(2000).command(&words);
    assert_eq!((program.as_str(), args), ("sh", words[1..].to_vec()));
}

#[test]
fn test_a_job_that_forks_past_an_rlimit_cap_has_its_processes_refused() {
    let m = Machine::new();
    let mut spec = Machine::spec("lead/t14-1");
    spec.launch.cap = Cap::Rlimit(1);
    let program = m.runner("touch forked");
    let mut started = run(&spec, &m.checkout(), &m.state(), &[], &program).unwrap();
    started.child.wait().unwrap();
    assert!(!started.worktree.join("forked").exists());
}

/// `git ARGS` in `dir` with an identity, signing off and submodules over files allowed.
fn git_as(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.org",
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
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn test_a_parked_worktree_has_its_submodule_back_at_the_pinned_commit_and_clean() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, top, slot) = (
        tmp.path().join("lib-origin"),
        tmp.path().join("top"),
        tmp.path().join("top-slot1"),
    );
    for (dir, file) in [(&lib, "lib.txt"), (&top, "top.txt")] {
        fs::create_dir_all(dir).unwrap();
        git_as(dir, &["init", "-q", "-b", "main"]);
        fs::write(dir.join(file), "one\n").unwrap();
        git_as(dir, &["add", file]);
        git_as(dir, &["commit", "-q", "-m", "Start"]);
    }
    git_as(
        &top,
        &["submodule", "add", "-q", &lib.display().to_string(), "lib"],
    );
    git_as(&top, &["commit", "-q", "-m", "Pin lib"]);
    git_as(
        &top,
        &[
            "worktree",
            "add",
            "-q",
            &slot.display().to_string(),
            "-b",
            "job",
        ],
    );
    git_as(&slot, &["submodule", "update", "--init", "-q"]);
    let pinned = git_as(&slot, &["rev-parse", "HEAD:lib"]);

    // A job's leftovers in the submodule: a tracked edit, a new file and a commit.
    let inside = slot.join("lib");
    fs::write(inside.join("lib.txt"), "edited\n").unwrap();
    fs::write(inside.join("stray"), "x").unwrap();
    git_as(&inside, &["commit", "-q", "-am", "Leftover"]);
    fs::write(inside.join("lib.txt"), "edited again\n").unwrap();

    park(&slot).unwrap();

    assert_eq!(git_as(&inside, &["rev-parse", "HEAD"]), pinned);
    assert_eq!(git_as(&inside, &["status", "--porcelain"]), "");
    assert_eq!(git_as(&slot, &["status", "--porcelain"]), "");
}
