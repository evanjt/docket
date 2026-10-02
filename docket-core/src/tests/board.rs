use super::Board;
use crate::member::Tie;
use crate::rows::{ItemRow, KeySpec, Progress, ProjectRow};
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
