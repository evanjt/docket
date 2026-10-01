use super::*;

fn open(kind: Kind) -> Facts<'static> {
    Facts {
        state: "open",
        kind,
        claimed: false,
        scope: None,
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
fn test_word_standing_kind_ignores_claim_and_scope() {
    let facts = Facts {
        claimed: true,
        scope: Some("later"),
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
fn test_word_scope_comes_before_wait_and_turn() {
    let facts = Facts {
        scope: Some("inbox"),
        waiting: true,
        turn: Some("user"),
        ..open(Kind::Work)
    };
    assert_eq!(word(&facts, 0), "inbox");
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
