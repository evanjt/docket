use super::*;
use crate::assignment::{Ask, Claim, Held};
use crate::word::kind_of_type;

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
        ..Item::default()
    }
}

fn claimed_on(id: &str, branch: &str, host: &str) -> Item {
    let mut row = open(id);
    row.held = Some(Held::Claim(Claim {
        branch: branch.into(),
        host: host.into(),
        since: "2026-09-30T00:00:00Z".into(),
        ..Claim::default()
    }));
    row
}

fn claimed(id: &str) -> Item {
    claimed_on(id, "audit/t-1", "testbox")
}

fn held_elsewhere(id: &str) -> Item {
    claimed_on(id, "audit/other-2", "otherbox")
}

fn asked(id: &str, note: Option<&str>) -> Item {
    let mut row = open(id);
    row.held = Some(Held::Ask(Ask {
        since: "t".into(),
        note: note.map(str::to_string),
    }));
    row
}

fn done(id: &str) -> Item {
    let mut row = open(id);
    row.state = "done".into();
    row.resolution = Some("abc1234".into());
    row
}

fn refusal<T: std::fmt::Debug>(r: Result<T, Refused>) -> String {
    r.expect_err("expected a refusal").0
}

#[test]
fn test_the_turn_reads_the_type() {
    assert_eq!(default_turn(ItemType::Question), "user");
    assert_eq!(default_turn(ItemType::Bug), "agent");
}

#[test]
fn test_kind_of_a_stored_type_word() {
    assert_eq!(kind_of_type("question"), Kind::Decision);
    assert_eq!(kind_of_type("plan"), Kind::Audit);
    assert_eq!(kind_of_type("investigation"), Kind::Research);
    assert_eq!(kind_of_type("bug"), Kind::Work);
    assert_eq!(kind_of_type(""), Kind::Work);
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
fn test_start_takes_an_open_unclaimed_item() {
    assert!(start(&open("B1"), &ctx(), None).is_ok());
}

#[test]
fn test_the_turn_is_the_owners_while_an_ask_is_open_and_none_once_closed() {
    assert_eq!(open("B1").turn(), Some("agent"));
    assert_eq!(claimed("B1").turn(), Some("agent"));
    assert_eq!(asked("B1", None).turn(), Some("user"));
    assert_eq!(done("B1").turn(), None);
    assert_eq!(claimed("B1").claim_branch(), Some("audit/t-1"));
    assert_eq!(asked("B1", None).claim_branch(), None);
}

#[test]
fn test_start_refusals() {
    assert_eq!(
        refusal(start(&claimed("B1"), &ctx(), None)),
        "B1 is already yours, claimed 2026-09-30T00:00:00Z."
    );
    assert_eq!(
        refusal(start(&held_elsewhere("B1"), &ctx(), None)),
        "B1 is held by audit/other-2 on otherbox since 2026-09-30T00:00:00Z. Pick another, or --force if that claim is abandoned."
    );
    let forced = Ctx {
        force: true,
        ..ctx()
    };
    assert!(start(&held_elsewhere("B1"), &forced, None).is_ok());
    let mut wait = Wait {
        on: "item",
        item: 7,
        id: "Q1".into(),
        since: "t".into(),
    };
    assert_eq!(
        refusal(start(&open("B1"), &ctx(), Some(&wait))),
        "B1 is waiting on item Q1 since t. resume it first."
    );
    wait.on = "condition";
    wait.id = "T4".into();
    assert_eq!(
        refusal(start(&open("B1"), &ctx(), Some(&wait))),
        "B1 is waiting on T4, a task on the owner's turn, since t. resume it first."
    );
    assert_eq!(
        refusal(start(&asked("Q1", None), &ctx(), None)),
        "Q1 is the owner's turn: asked t. reply to it first if you are taking it back."
    );
    assert_eq!(
        refusal(start(&asked("Q1", Some("needs the handset")), &ctx(), None)),
        "Q1 is the owner's turn: needs the handset. reply to it first if you are taking it back."
    );
    assert!(start(&done("B1"), &ctx(), None).is_err());
}

#[test]
fn test_a_plan_closes_with_work_open_under_it_only_while_held() {
    let gaps = vec!["B7".to_string()];
    assert!(close(&open("A1"), &ctx(), Some("clean"), Kind::Audit, &[]).is_ok());
    assert!(close(&claimed("A1"), &ctx(), Some("clean"), Kind::Audit, &gaps).is_ok());
    assert_eq!(
        refusal(close(
            &open("A1"),
            &ctx(),
            Some("clean"),
            Kind::Audit,
            &gaps
        )),
        "A1 has work under it that is still open: B7. It closes only when nothing under it is open; unclaim it and it comes back when they close."
    );
}

#[test]
fn test_release_takes_a_claim_held_here() {
    assert!(release(&claimed("B1"), &ctx()).is_ok());
    assert_eq!(refusal(release(&open("B1"), &ctx())), "B1 is not claimed.");
    assert!(release(&held_elsewhere("B1"), &ctx()).is_err());
}

#[test]
fn test_close_sets_done() {
    let got = close(&claimed("B1"), &ctx(), Some("abc1234"), Kind::Work, &[]).unwrap();
    let want = vec![
        Field::State("done".into()),
        Field::Resolution(Some("abc1234".into())),
    ];
    assert_eq!(got, want);
}

#[test]
fn test_close_refusals() {
    assert_eq!(
        refusal(close(
            &open("Q1"),
            &ctx(),
            Some("opened nothing"),
            Kind::Decision,
            &[]
        )),
        "Q1 is a decision and has none yet. answer it first, or drop it if it no longer needs one."
    );
    assert_eq!(
        refusal(close(&open("B1"), &ctx(), None, Kind::Work, &[])),
        "close needs a resolution: the sha, or what the work opened."
    );
    assert_eq!(
        refusal(close(&open("B1"), &ctx(), Some(""), Kind::Work, &[])),
        "close needs a resolution: the sha, or what the work opened."
    );
    assert!(close(&held_elsewhere("B1"), &ctx(), Some("x"), Kind::Work, &[]).is_err());
    assert!(close(&done("B1"), &ctx(), Some("x"), Kind::Work, &[]).is_err());
}

#[test]
fn test_prioritise_writes_the_priority_column_alone() {
    let mut row = open("B1");
    row.apply(&prioritise("high").unwrap());
    assert_eq!(row.priority, "high");
    assert_eq!(
        prioritise("critical").unwrap(),
        vec![Field::Priority("critical".into())]
    );
    assert_eq!(
        refusal(prioritise("urgent")),
        "priority is one of critical, high, normal, low, not 'urgent'"
    );
}

#[test]
fn test_drop_records_why_and_what_supersedes_it() {
    let got = drop(&open("B2"), &ctx(), Some("superseded by B1"), Some(1)).unwrap();
    assert_eq!(got[0], Field::State("dropped".into()));
    assert_eq!(got[1], Field::Resolution(Some("superseded by B1".into())));
    assert_eq!(got[2], Field::SupersededBy(Some(1)));
    assert_eq!(got.len(), 3);
    assert_eq!(
        refusal(drop(&open("B2"), &ctx(), None, None)),
        "drop needs a reason."
    );
    assert!(drop(&held_elsewhere("B2"), &ctx(), Some("x"), None).is_err());
}

#[test]
fn test_reopen_needs_a_closed_item_and_a_reason() {
    assert_eq!(
        reopen(&done("B1"), Some("not a duplicate")).unwrap(),
        vec![
            Field::State("open".into()),
            Field::Resolution(None),
            Field::SupersededBy(None),
        ]
    );
    assert_eq!(
        refusal(reopen(&open("B1"), Some("x"))),
        "B1 is already open."
    );
    assert_eq!(
        refusal(reopen(&done("B1"), None)),
        "reopen needs a reason, it is recorded."
    );
}

#[test]
fn test_an_item_depends_on_another_never_on_itself() {
    assert!(depend(&claimed("B1"), &ctx(), 7).is_ok());
    assert!(
        depend(&open("B1"), &ctx(), 7).is_ok(),
        "a second dependency"
    );
    assert_eq!(
        refusal(depend(&open("B1"), &ctx(), 1)),
        "B1 cannot depend on itself."
    );
    assert!(depend(&held_elsewhere("B1"), &ctx(), 7).is_err());
    assert!(depend(&done("B1"), &ctx(), 7).is_err());
}

#[test]
fn test_resume_needs_a_wait() {
    let wait = Wait {
        on: "condition",
        item: 7,
        id: "T4".into(),
        since: "t".into(),
    };
    assert!(resume(&open("B1"), Some(&wait)).is_ok());
    assert_eq!(refusal(resume(&open("B1"), None)), "B1 is not waiting.");
}

#[test]
fn test_ask_needs_a_note_and_an_item_held_here() {
    assert!(ask(&claimed("B1"), &ctx(), Some("needs a run on the bench")).is_ok());
    assert!(
        ask(
            &held_elsewhere("B1"),
            &ctx(),
            Some("needs a run on the bench")
        )
        .is_err()
    );
    assert_eq!(
        refusal(ask(&open("B1"), &ctx(), None)),
        "ask needs a note saying what is needed."
    );
    assert!(ask(&done("B1"), &ctx(), Some("too late")).is_err());
}

#[test]
fn test_reply_needs_the_owners_turn() {
    let row = asked("B1", Some("which way"));
    assert!(reply(&row, Some("crashes on open")).is_ok());
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
    let row = asked("Q1", None);
    let got = answer(&row, &ctx(), Some("Two"), Kind::Decision, None).unwrap();
    assert_eq!(
        got,
        [
            Field::Decision(Some("Two".into())),
            Field::DecidedAt(Some("2026-10-01T12:00:00Z".into())),
        ]
    );
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
        got[0],
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
fn test_set_tags_is_the_list_as_written_and_a_priority_word_is_only_a_label() {
    assert_eq!(set_tags("single, low"), vec!["low", "single"]);
    assert_eq!(set_tags("single"), vec!["single"]);
    assert_eq!(set_tags(" , "), Vec::<String>::new());
}

#[test]
fn test_apply_reads_back_as_the_row_after_the_update() {
    let mut row = claimed("B1");
    row.apply(&close(&row, &ctx(), Some("abc1234"), Kind::Work, &[]).unwrap());
    assert_eq!(row.state, "done");
    assert_eq!(row.resolution.as_deref(), Some("abc1234"));
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
    let plan = refusal(close(
        &open("A1"),
        &ctx(),
        Some("clean"),
        Kind::Audit,
        &["B7".to_string()],
    ));
    assert!(plan.contains("unclaim it and it comes back"), "{plan}");
    assert!(!plan.contains("release"), "{plan}");
    let own = refusal(release(&done("B1"), &ctx()));
    assert!(own.contains("unclaim needs an open item"), "{own}");
}
