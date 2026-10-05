use super::*;

fn ctx() -> Ctx {
    Ctx {
        host: "testbox".into(),
        branch: "audit/t-1".into(),
        now: "2026-10-01T12:00:00Z".into(),
        force: false,
    }
}

fn open(id: &str) -> Item {
    let (key, num) = crate::text::split_id(id).unwrap();
    Item {
        rid: 1,
        project: "test/proj".into(),
        key,
        num,
        id: id.into(),
        title: "x".into(),
        state: "open".into(),
        turn: Some("agent".into()),
        ..Item::default()
    }
}

fn claimed(id: &str) -> Item {
    let mut row = open(id);
    row.apply(&[
        Field::ClaimBranch(Some("audit/t-1".into())),
        Field::ClaimHost(Some("testbox".into())),
        Field::ClaimSince(Some("2026-09-30T00:00:00Z".into())),
    ]);
    row
}

fn held_elsewhere(id: &str) -> Item {
    let mut row = claimed(id);
    row.claim_branch = Some("audit/other-2".into());
    row.claim_host = Some("otherbox".into());
    row
}

fn done(id: &str) -> Item {
    let mut row = open(id);
    row.state = "done".into();
    row.turn = None;
    row.resolution = Some("abc1234".into());
    row
}

fn project() -> Project {
    Project {
        slug: "test/proj".into(),
        keys: vec![
            KeySpec {
                key: "B".into(),
                kind: Kind::Work,
                meaning: None,
                turn: Some("agent".into()),
            },
            KeySpec {
                key: "Q".into(),
                kind: Kind::Decision,
                meaning: None,
                turn: None,
            },
        ],
    }
}

fn refusal<T: std::fmt::Debug>(r: Result<T, Refused>) -> String {
    r.expect_err("expected a refusal").0
}

#[test]
fn test_key_spec_kind_and_default_turn() {
    let p = project();
    assert_eq!(kind_of(&p, "Q").unwrap(), Kind::Decision);
    assert_eq!(default_turn(&p, "Q").unwrap(), "user");
    assert_eq!(default_turn(&p, "B").unwrap(), "agent");
    assert_eq!(
        refusal(key_spec(&p, "X")),
        "test/proj has no key X; its keys are B, Q"
    );
}

#[test]
fn test_holds_same_branch_or_force() {
    assert!(holds(&open("B1"), &ctx()));
    assert!(holds(&claimed("B1"), &ctx()));
    assert!(!holds(&held_elsewhere("B1"), &ctx()));
    let forced = Ctx {
        force: true,
        ..ctx()
    };
    assert!(holds(&held_elsewhere("B1"), &forced));
    assert_eq!(
        refusal(require_hold(&held_elsewhere("B1"), &ctx(), "close")),
        "B1 is held by audit/other-2 on otherbox since 2026-09-30T00:00:00Z. close would take it out from under that agent. Merge or unclaim the branch first, or pass --force."
    );
}

#[test]
fn test_require_open_names_the_state_and_resolution() {
    assert_eq!(
        refusal(require_open(&done("B1"), "start")),
        "B1 is done (abc1234); start needs an open item. reopen it first if that is what you mean."
    );
    assert!(require_open(&open("B1"), "start").is_ok());
}

#[test]
fn test_start_sets_the_claim() {
    assert_eq!(
        start(&open("B1"), &ctx()).unwrap(),
        vec![
            Field::ClaimBranch(Some("audit/t-1".into())),
            Field::ClaimHost(Some("testbox".into())),
            Field::ClaimSince(Some("2026-10-01T12:00:00Z".into())),
        ]
    );
}

#[test]
fn test_start_refusals() {
    assert_eq!(
        refusal(start(&claimed("B1"), &ctx())),
        "B1 is already yours, claimed 2026-09-30T00:00:00Z."
    );
    assert_eq!(
        refusal(start(&held_elsewhere("B1"), &ctx())),
        "B1 is held by audit/other-2 on otherbox since 2026-09-30T00:00:00Z. Pick another, or --force if that claim is abandoned."
    );
    let forced = Ctx {
        force: true,
        ..ctx()
    };
    assert!(start(&held_elsewhere("B1"), &forced).is_ok());
    let mut row = open("B1");
    row.scope = Some("inbox".into());
    assert!(start(&row, &ctx()).is_ok());
    let mut row = open("B1");
    row.wait_on = Some("item".into());
    row.wait_ref = Some("Q1".into());
    row.wait_since = Some("t".into());
    assert_eq!(
        refusal(start(&row, &ctx())),
        "B1 is waiting on item Q1 since t. resume it first."
    );
    row.wait_on = Some("condition".into());
    row.wait_ref = Some("the fleet is quiet".into());
    assert_eq!(
        refusal(start(&row, &ctx())),
        "B1 is waiting on the fleet is quiet since t. resume it first."
    );
    let mut row = open("Q1");
    row.turn = Some("user".into());
    row.asked_at = Some("t".into());
    assert_eq!(
        refusal(start(&row, &ctx())),
        "Q1 is the owner's turn: asked t. reply to it first if you are taking it back."
    );
    row.turn_note = Some("needs the handset".into());
    assert_eq!(
        refusal(start(&row, &ctx())),
        "Q1 is the owner's turn: needs the handset. reply to it first if you are taking it back."
    );
    let mut row = open("B1");
    row.conflict = 1;
    assert_eq!(
        refusal(start(&row, &ctx())),
        "B1 carries a sync conflict in its body. resolve it first."
    );
    assert!(start(&done("B1"), &ctx()).is_err());
}

#[test]
fn test_a_plan_came_due_when_its_last_gate_event_is_the_resume() {
    let waited = format!("until: {GATE} (2 open)");
    assert!(!came_due([]));
    assert!(!came_due([("waited", waited.as_str())]));
    assert!(came_due([("waited", waited.as_str()), ("resumed", GATE)]));
    assert!(!came_due([
        ("waited", waited.as_str()),
        ("resumed", GATE),
        ("waited", waited.as_str()),
    ]));
    assert!(came_due([
        ("waited", waited.as_str()),
        ("resumed", GATE),
        ("waited", "until: the owner is back"),
        ("resumed", "Q3 closed"),
    ]));
}

#[test]
fn test_a_plan_closes_with_its_gaps_open_only_once_due() {
    let plan = open("A1");
    let gaps = vec!["B7".to_string()];
    assert!(close_plan(&plan, &[], false).is_ok());
    assert!(close_plan(&plan, &gaps, true).is_ok());
    assert_eq!(
        refusal(close_plan(&plan, &gaps, false)),
        "A1 opened work that is still open: B7. It closes only when nothing it opened is open; unclaim it and it comes back when they close."
    );
}

#[test]
fn test_release_clears_every_claim_column() {
    assert_eq!(release(&claimed("B1"), &ctx()).unwrap(), unclaimed());
    assert_eq!(refusal(release(&open("B1"), &ctx())), "B1 is not claimed.");
    assert!(release(&held_elsewhere("B1"), &ctx()).is_err());
}

#[test]
fn test_close_sets_done_and_clears_claim_and_wait() {
    let got = close(&claimed("B1"), &ctx(), Some("abc1234"), Kind::Work).unwrap();
    let mut want = vec![
        Field::State("done".into()),
        Field::Turn(None),
        Field::TurnNote(None),
        Field::Resolution(Some("abc1234".into())),
    ];
    want.extend(unclaimed());
    want.extend(unwaiting());
    assert_eq!(got, want);
}

#[test]
fn test_close_refusals() {
    assert_eq!(
        refusal(close(
            &open("Q1"),
            &ctx(),
            Some("opened nothing"),
            Kind::Decision
        )),
        "Q1 is a decision and has none yet. answer it first, or drop it if it no longer needs one."
    );
    assert_eq!(
        refusal(close(&open("B1"), &ctx(), None, Kind::Work)),
        "close needs a resolution: the sha, or what the work opened."
    );
    assert_eq!(
        refusal(close(&open("B1"), &ctx(), Some(""), Kind::Work)),
        "close needs a resolution: the sha, or what the work opened."
    );
    assert!(close(&held_elsewhere("B1"), &ctx(), Some("x"), Kind::Work).is_err());
    assert!(close(&done("B1"), &ctx(), Some("x"), Kind::Work).is_err());
}

#[test]
fn test_prioritise_replaces_the_tier_and_keeps_other_tags() {
    let tags = vec!["single".to_string(), "low".to_string()];
    assert_eq!(
        prioritise(&tags, "high").unwrap(),
        vec![Field::Tags(vec!["high".into(), "single".into()])]
    );
    assert_eq!(
        prioritise(&tags, "normal").unwrap(),
        vec![Field::Tags(vec!["single".into()])]
    );
    assert_eq!(
        prioritise(&[], "critical").unwrap(),
        vec![Field::Tags(vec!["critical".into()])]
    );
    assert_eq!(
        refusal(prioritise(&[], "urgent")),
        "priority is one of critical, high, normal, low, not 'urgent'"
    );
}

#[test]
fn test_drop_records_why_and_what_supersedes_it() {
    let got = drop(&open("B2"), &ctx(), Some("superseded by B1"), Some(1)).unwrap();
    assert_eq!(got[0], Field::State("dropped".into()));
    assert_eq!(got[3], Field::Resolution(Some("superseded by B1".into())));
    assert_eq!(got[4], Field::SupersededBy(Some(1)));
    assert_eq!(got.len(), 5 + 6 + 4);
    assert_eq!(
        refusal(drop(&open("B2"), &ctx(), None, None)),
        "drop needs a reason."
    );
    assert!(drop(&held_elsewhere("B2"), &ctx(), Some("x"), None).is_err());
}

#[test]
fn test_reopen_needs_a_closed_item_and_a_reason() {
    assert_eq!(
        reopen(&done("B1"), Some("not a duplicate"), "agent").unwrap(),
        vec![
            Field::State("open".into()),
            Field::Turn(Some("agent".into())),
            Field::Resolution(None),
            Field::SupersededBy(None),
        ]
    );
    assert_eq!(
        refusal(reopen(&open("B1"), Some("x"), "agent")),
        "B1 is already open."
    );
    assert_eq!(
        refusal(reopen(&done("B1"), None, "agent")),
        "reopen needs a reason, it is recorded."
    );
}

#[test]
fn test_an_item_depends_on_another_never_on_itself() {
    assert!(depend(&claimed("B1"), &ctx(), 7).is_ok());
    let mut row = open("B1");
    row.wait_on = Some("item".into());
    row.wait_ref = Some("Q1".into());
    row.wait_since = Some("t".into());
    assert!(depend(&row, &ctx(), 7).is_ok(), "a second dependency");
    assert_eq!(
        refusal(depend(&open("B1"), &ctx(), 1)),
        "B1 cannot depend on itself."
    );
    assert!(depend(&held_elsewhere("B1"), &ctx(), 7).is_err());
    assert!(depend(&done("B1"), &ctx(), 7).is_err());
}

#[test]
fn test_resume_clears_the_wait_and_hands_it_to_an_agent() {
    let mut row = open("B1");
    row.wait_on = Some("condition".into());
    row.wait_ref = Some("x".into());
    row.wait_since = Some("t".into());
    let mut want = unwaiting();
    want.push(Field::Turn(Some("agent".into())));
    assert_eq!(resume(&row, &[], false).unwrap(), want);
    assert_eq!(
        refusal(resume(&open("B1"), &[], false)),
        "B1 is not waiting."
    );
}

#[test]
fn test_resume_on_a_gated_plan_is_refused_while_a_member_is_open() {
    let mut row = open("A1");
    row.wait_on = Some("condition".into());
    row.wait_ref = Some(GATE.into());
    row.wait_since = Some("t".into());
    let members = ["T4".to_string()];
    assert_eq!(
        refusal(resume(&row, &members, false)),
        "A1 waits until everything it opened is closed, and T4 is open. Close it first, or pass --force."
    );
    assert!(resume(&row, &members, true).is_ok());
    assert!(resume(&row, &[], false).is_ok());
}

#[test]
fn test_ask_parks_with_the_owner_and_drops_the_claim() {
    let got = ask(&claimed("B1"), &ctx(), Some("needs a run on the S22")).unwrap();
    let mut want = vec![
        Field::Turn(Some("user".into())),
        Field::TurnNote(Some("needs a run on the S22".into())),
        Field::AskedAt(Some("2026-10-01T12:00:00Z".into())),
    ];
    want.extend(unclaimed());
    assert_eq!(got, want);
    assert_eq!(
        refusal(ask(&open("B1"), &ctx(), None)),
        "ask needs a note saying what is needed."
    );
    assert!(ask(&done("B1"), &ctx(), Some("too late")).is_err());
}

#[test]
fn test_reply_needs_the_owners_turn() {
    let mut row = open("B1");
    row.turn = Some("user".into());
    assert_eq!(
        reply(&row, Some("crashes on open")).unwrap(),
        vec![
            Field::Turn(Some("agent".into())),
            Field::TurnNote(Some("crashes on open".into())),
        ]
    );
    assert_eq!(
        refusal(reply(&open("B1"), Some("nothing to reply to"))),
        "B1 is already the agent's turn."
    );
    assert_eq!(
        refusal(reply(&row, None)),
        "reply needs a note saying what happened."
    );
}

#[test]
fn test_answer_records_the_decision() {
    let mut row = open("Q1");
    row.turn = Some("user".into());
    let got = answer(&row, &ctx(), Some("Two"), Kind::Decision, None).unwrap();
    assert_eq!(got[0], Field::Turn(Some("agent".into())));
    assert_eq!(got[1], Field::TurnNote(Some("Two".into())));
    assert_eq!(got[2], Field::Decision(Some("Two".into())));
    assert_eq!(
        got[3],
        Field::DecidedAt(Some("2026-10-01T12:00:00Z".into()))
    );
    assert_eq!(got[4..], unclaimed()[..]);
}

#[test]
fn test_answer_derived_carries_its_basis_and_never_replaces_the_owners() {
    let row = open("Q1");
    let got = answer(
        &row,
        &ctx(),
        Some("Two, the read path is hot"),
        Kind::Decision,
        Some("CID3, one owner per cache"),
    )
    .unwrap();
    assert_eq!(
        got[2],
        Field::Decision(Some(
            "Derived from CID3, one owner per cache: Two, the read path is hot".into()
        ))
    );
    assert_eq!(
        refusal(answer(&row, &ctx(), Some("Blue"), Kind::Decision, Some(""))),
        "a derived answer needs its basis: the decision, central idea or practice it follows."
    );
    let mut decided = open("Q1");
    decided.decision = Some("Red".into());
    decided.decided_at = Some("2026-09-01T00:00:00Z".into());
    assert_eq!(
        refusal(answer(
            &decided,
            &ctx(),
            Some("Blue"),
            Kind::Decision,
            Some("Q2")
        )),
        "Q1 was decided by the owner on 2026-09-01T00:00:00Z: Red. A derived answer never replaces that."
    );
    decided.decision = Some("Derived from Q2: Red".into());
    assert!(answer(&decided, &ctx(), Some("Blue"), Kind::Decision, Some("Q3")).is_ok());
    assert_eq!(
        refusal(answer(&open("B1"), &ctx(), Some("x"), Kind::Work, None)),
        "B1 is work, not a decision. reply to it, or close it."
    );
    assert_eq!(
        refusal(answer(&row, &ctx(), None, Kind::Decision, None)),
        "answer needs the decision, in words."
    );
}

#[test]
fn test_rate_takes_one_of_three_levels() {
    assert_eq!(
        rate("high").unwrap(),
        vec![Field::Complexity(Some("high".into()))]
    );
    assert_eq!(
        refusal(rate("huge")),
        "complexity is one of high, medium, low, not 'huge'"
    );
}

#[test]
fn test_set_tags_keeps_the_priority_unless_the_list_names_one() {
    let high = vec!["high".to_string()];
    assert_eq!(set_tags(&high, "single"), vec!["high", "single"]);
    let two = vec!["high".to_string(), "old".to_string()];
    assert_eq!(set_tags(&two, "single, low"), vec!["low", "single"]);
    assert_eq!(set_tags(&[], "single"), vec!["single"]);
    assert_eq!(set_tags(&[], " , "), Vec::<String>::new());
}

#[test]
fn test_apply_reads_back_as_the_row_after_the_update() {
    let mut row = claimed("B1");
    row.apply(&close(&row, &ctx(), Some("abc1234"), Kind::Work).unwrap());
    assert_eq!(row.state, "done");
    assert_eq!(row.turn, None);
    assert_eq!(row.claim_branch, None);
    assert_eq!(row.resolution.as_deref(), Some("abc1234"));
}

#[test]
fn test_retry_hands_a_parked_item_back_to_the_agents() {
    let mut row = open("B3");
    row.turn = Some("user".into());
    assert_eq!(
        retry(&row, Some("go again")).unwrap(),
        [
            Field::Turn(Some("agent".into())),
            Field::TurnNote(Some("go again".into()))
        ]
    );
    assert_eq!(
        refusal(retry(&row, None)),
        "reply needs a note saying what happened."
    );
}

#[test]
fn test_retry_on_the_agents_turn_changes_no_column() {
    assert!(retry(&open("B3"), None).unwrap().is_empty());
}

#[test]
fn test_retry_refuses_a_held_or_closed_item() {
    assert_eq!(
        refusal(retry(&held_elsewhere("B3"), Some("x"))),
        "B3 is held by audit/other-2: docket kill B3 first if its job is stuck."
    );
    assert!(
        refusal(retry(&done("B3"), Some("x"))).starts_with("B3 is done (abc1234); retry needs")
    );
}

#[test]
fn supersede_relabels_every_earlier_decision() {
    let body =
        "Why.\n\n**Decision, 2026-09-14.** one\n\n**Decision, 2026-09-14, derived.** two. Basis: b";
    let (out, n) = supersede_decisions(body);
    assert_eq!(n, 2);
    assert_eq!(
        out,
        "Why.\n\n**Superseded decision, 2026-09-14.** one\n\n**Superseded decision, 2026-09-14.** two. Basis: b"
    );
}

#[test]
fn supersede_leaves_other_text_alone() {
    let body = "Mentions **Decision, x** inline.\n\n**Superseded decision, 2026-09-14.** old";
    assert_eq!(supersede_decisions(body), (body.to_string(), 0));
}

#[test]
fn repeat_answer_is_the_stored_decision() {
    let mut row = open("Q1");
    row.decision = Some("Go home".into());
    assert!(is_repeat_answer(&row, " Go home\n", None));
    assert!(!is_repeat_answer(&row, "Stay", None));
    assert!(!is_repeat_answer(&row, "Go home", Some("a basis")));
}

#[test]
fn test_refusals_say_unclaim_for_giving_a_claim_back() {
    let held = refusal(require_hold(&held_elsewhere("B1"), &ctx(), "close"));
    assert!(held.contains("Merge or unclaim the branch first"), "{held}");
    assert!(!held.contains("release"), "{held}");
    let plan = refusal(close_plan(&open("A1"), &["B7".to_string()], false));
    assert!(plan.contains("unclaim it and it comes back"), "{plan}");
    assert!(!plan.contains("release"), "{plan}");
    let own = refusal(release(&done("B1"), &ctx()));
    assert!(own.contains("unclaim needs an open item"), "{own}");
}
