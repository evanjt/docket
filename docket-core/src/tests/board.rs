use super::{Board, GateProblem};
use crate::member::Tie;
use crate::rows::{ItemRow, KeySpec, Progress, ProjectRow};
use crate::rules::GATE;
use crate::word::Kind;

fn project() -> ProjectRow {
    let keys = [("T", Kind::Work), ("A", Kind::Audit)]
        .into_iter()
        .map(|(key, kind)| KeySpec {
            key: key.into(),
            kind,
            meaning: None,
            turn: None,
        })
        .collect();
    ProjectRow {
        slug: "o/p".into(),
        keys,
        ..ProjectRow::default()
    }
}

fn item(rid: i64, id: &str, state: &str) -> ItemRow {
    let key: String = id.chars().take_while(char::is_ascii_alphabetic).collect();
    ItemRow {
        rid,
        project: "o/p".into(),
        num: id[key.len()..].parse().unwrap(),
        key,
        id: id.into(),
        state: state.into(),
        turn: Some("agent".into()),
        ..ItemRow::default()
    }
}

fn opened(rid: i64, to: i64) -> Tie {
    Tie {
        rid,
        opened: true,
        to,
    }
}

/// A1 opened T1 and T2, both closed; T2 opened T3, whose state is given.
fn plan_with_grandchild(t3: &str) -> Board {
    let items = vec![
        item(1, "A1", "open"),
        item(2, "T1", "done"),
        item(3, "T2", "done"),
        item(4, "T3", t3),
    ];
    Board::new(
        project(),
        items,
        vec![opened(2, 1), opened(3, 1), opened(4, 3)],
    )
}

#[test]
fn test_a_plan_counts_what_it_opened_at_any_depth() {
    let b = plan_with_grandchild("open");
    let a1 = b.get("A1").unwrap();
    assert_eq!(
        b.progress(a1),
        Progress {
            done: 2,
            total: 3,
            live: 0
        }
    );
    assert!(!b.due(a1));
}

#[test]
fn test_a_plan_is_due_once_everything_under_it_is_closed() {
    let b = plan_with_grandchild("done");
    assert!(b.due(b.get("A1").unwrap()));
    assert!(!b.due(b.get("T2").unwrap()));
}

#[test]
fn test_a_claimed_plan_is_not_due() {
    let mut b = plan_with_grandchild("done");
    b.items[0].claim_branch = Some("audit/a1".into());
    assert!(!b.due(b.get("A1").unwrap()));
}

fn gated(b: &mut Board, idx: usize) {
    b.items[idx].wait_on = Some("condition".into());
    b.items[idx].wait_ref = Some(GATE.into());
}

#[test]
fn test_a_gated_plan_with_everything_closed_is_a_held_gate() {
    let mut b = plan_with_grandchild("done");
    gated(&mut b, 0);
    assert_eq!(
        b.gate_problems(),
        vec![GateProblem::HeldGate { id: "A1".into() }]
    );
}

#[test]
fn test_an_ungated_plan_with_open_members_at_depth_two_is_an_open_audit() {
    let items = vec![
        item(1, "A1", "open"),
        item(2, "T1", "done"),
        item(3, "T2", "done"),
        item(4, "T3", "open"),
        item(5, "T4", "open"),
    ];
    let ties = vec![opened(2, 1), opened(3, 1), opened(4, 3), opened(5, 3)];
    let b = Board::new(project(), items, ties);
    assert_eq!(
        b.gate_problems(),
        vec![GateProblem::OpenAudit {
            id: "A1".into(),
            n: 2
        }]
    );
}

#[test]
fn test_a_gated_plan_with_open_members_and_a_claimed_plan_are_not_problems() {
    let mut b = plan_with_grandchild("open");
    gated(&mut b, 0);
    assert!(b.gate_problems().is_empty());
    b.items[0].wait_on = None;
    b.items[0].wait_ref = None;
    b.items[0].claim_branch = Some("audit/a1".into());
    assert!(b.gate_problems().is_empty());
}

#[test]
fn test_a_plan_under_a_plan_is_reported_once_each() {
    let items = vec![
        item(1, "A1", "open"),
        item(2, "A2", "open"),
        item(3, "T1", "open"),
    ];
    let ties = vec![opened(2, 1), opened(3, 2)];
    let b = Board::new(project(), items, ties);
    assert_eq!(
        b.gate_problems(),
        vec![
            GateProblem::OpenAudit {
                id: "A1".into(),
                n: 2
            },
            GateProblem::OpenAudit {
                id: "A2".into(),
                n: 1
            },
        ]
    );
}
