use super::*;

fn open(claimed: bool, waiting: bool, turn: Option<&str>) -> Standing<'_> {
    Standing::of("open", claimed, waiting, turn)
}

#[test]
fn test_word_closed_is_its_state() {
    let row = Standing::of("dropped", false, false, None);
    assert_eq!(row.word(), "dropped");
}

#[test]
fn test_word_open_item_is_ready_or_in_progress() {
    assert_eq!(open(true, false, None).word(), "in progress");
    assert_eq!(open(false, false, None).word(), "ready");
}

#[test]
fn test_word_claimed_item_is_in_progress() {
    assert_eq!(open(true, false, None).word(), "in progress");
}

#[test]
fn test_word_open_assignment_comes_before_dependency_and_turn() {
    assert_eq!(open(true, true, Some("user")).word(), "in progress");
}

#[test]
fn test_word_dependency_comes_before_turn() {
    assert_eq!(open(false, true, Some("user")).word(), "blocked");
    assert_eq!(open(false, false, Some("user")).word(), "waiting on owner");
}

#[test]
fn test_word_plan_reads_its_members() {
    let row = |open_members, closed_members| {
        open(false, false, Some("agent")).holding(Kind::Audit, open_members, closed_members)
    };
    assert_eq!(row(2, 1).word(), "under way");
    assert_eq!(row(0, 3).word(), "audit due");
    assert_eq!(row(0, 0).word(), "ready");
}

#[test]
fn test_word_package_reads_open_members_only() {
    let row = |open_members, closed_members| {
        open(false, false, Some("agent")).holding(Kind::Package, open_members, closed_members)
    };
    assert_eq!(row(2, 1).word(), "under way");
    assert_eq!(row(0, 3).word(), "ready");
}

#[test]
fn test_priority_first_tier_named_or_normal() {
    assert_eq!(priority(&["single".into(), "high".into()]), "high");
    assert_eq!(priority(&["low".into(), "critical".into()]), "critical");
    assert_eq!(priority(&[]), "normal");
}

fn live() -> Standing<'static> {
    Standing {
        state: "open",
        released: true,
        ..Standing::default()
    }
}

#[test]
fn test_status_each_rule_in_order() {
    let table = [
        (
            Standing {
                state: "done",
                open_assignment: true,
                ..live()
            },
            Word::Done,
        ),
        (
            Standing {
                state: "dropped",
                ..live()
            },
            Word::Dropped,
        ),
        (
            Standing {
                open_assignment: true,
                unsatisfied_dependency: true,
                ..live()
            },
            Word::InProgress,
        ),
        (
            Standing {
                unsatisfied_dependency: true,
                owner_turn: true,
                ..live()
            },
            Word::Blocked,
        ),
        (
            Standing {
                owner_turn: true,
                released: false,
                ..live()
            },
            Word::WaitingOnOwner,
        ),
        (
            Standing {
                released: false,
                open_members: 1,
                ..live()
            },
            Word::Parked,
        ),
        (
            Standing {
                open_members: 2,
                closed_members: 1,
                ..live()
            },
            Word::UnderWay,
        ),
        (
            Standing {
                closed_members: 3,
                ..live()
            },
            Word::AuditDue,
        ),
        (live(), Word::Ready),
    ];
    for (facts, want) in table {
        assert_eq!(status(&facts), want, "{facts:?}");
    }
}

#[test]
fn test_status_plan_reads_audit_due_when_its_last_member_closes() {
    let under_way = Standing {
        open_members: 1,
        closed_members: 2,
        ..live()
    };
    assert_eq!(status(&under_way), Word::UnderWay);
    let closed = Standing {
        open_members: 0,
        closed_members: 3,
        ..under_way
    };
    assert_eq!(status(&closed).as_str(), "audit due");
}

#[test]
fn test_each_type_files_under_its_key_and_a_retired_key_files_nothing() {
    for t in ItemType::ALL {
        assert_eq!(ItemType::filed_under(t.key()), Some(t));
        assert_eq!(ItemType::parse(t.as_str()), Some(t));
    }
    for retired in ["STY", "PK", "CON", "CID", "X"] {
        assert_eq!(ItemType::filed_under(retired), None);
    }
}

#[test]
fn test_a_stored_item_takes_its_type_from_its_key_else_its_kind() {
    assert_eq!(ItemType::of_stored("B", Kind::Work), ItemType::Bug);
    assert_eq!(ItemType::of_stored("PK", Kind::Package), ItemType::Plan);
    assert_eq!(ItemType::of_stored("PK", Kind::Audit), ItemType::Plan);
    assert_eq!(ItemType::of_stored("STY", Kind::Story), ItemType::Plan);
    assert_eq!(ItemType::of_stored("ZZ", Kind::Work), ItemType::Task);
    assert_eq!(
        ItemType::of_stored("ZZ", Kind::Decision),
        ItemType::Question
    );
    assert_eq!(
        ItemType::of_stored("ZZ", Kind::Research),
        ItemType::Investigation
    );
}
