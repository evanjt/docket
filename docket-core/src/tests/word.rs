use super::*;

fn open(kind: Kind) -> Facts<'static> {
    Facts {
        state: "open",
        kind,
        claimed: false,
        waiting: false,
        turn: Some("agent"),
    }
}

#[test]
fn test_word_closed_is_its_state() {
    let facts = Facts {
        state: "dropped",
        turn: None,
        ..open(Kind::Work)
    };
    assert_eq!(word(&facts, 0), "dropped");
}

#[test]
fn test_word_standing_kind_ignores_claim() {
    let facts = Facts {
        claimed: true,
        ..open(Kind::Concept)
    };
    assert_eq!(word(&facts, 0), "standing");
}

#[test]
fn test_word_claimed_ticket_builds_and_claimed_package_checks() {
    assert_eq!(
        word(
            &Facts {
                claimed: true,
                ..open(Kind::Work)
            },
            0
        ),
        "building"
    );
    assert_eq!(
        word(
            &Facts {
                claimed: true,
                ..open(Kind::Package)
            },
            0
        ),
        "checking"
    );
}

#[test]
fn test_word_wait_comes_before_turn() {
    let facts = Facts {
        waiting: true,
        turn: Some("user"),
        ..open(Kind::Work)
    };
    assert_eq!(word(&facts, 0), "blocked");
    assert_eq!(
        word(
            &Facts {
                turn: Some("user"),
                ..open(Kind::Decision)
            },
            0
        ),
        "parked"
    );
}

#[test]
fn test_word_package_with_open_members_builds() {
    assert_eq!(word(&open(Kind::Package), 2), "building");
    assert_eq!(word(&open(Kind::Package), 0), "ready");
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
            Word::Building,
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
    let building = Standing {
        open_members: 1,
        closed_members: 2,
        ..live()
    };
    assert_eq!(status(&building), Word::Building);
    let closed = Standing {
        open_members: 0,
        closed_members: 3,
        ..building
    };
    assert_eq!(status(&closed).as_str(), "audit due");
}
