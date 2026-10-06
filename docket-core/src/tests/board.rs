use super::{Board, PlanCount};
use crate::member::{Edge, Tie};
use crate::rows::{ItemRow, ProjectRow};
use crate::word::ItemType;

/// A project whose stored key list is empty: every item is read by its type.
fn project() -> ProjectRow {
    ProjectRow {
        slug: "o/p".into(),
        ..ProjectRow::default()
    }
}

fn item(rid: i64, id: &str, state: &str) -> ItemRow {
    let key: String = id.chars().take_while(char::is_ascii_alphabetic).collect();
    let item_type = ItemType::filed_under(&key).unwrap_or_default().as_str();
    ItemRow {
        rid,
        project: "o/p".into(),
        num: id[key.len()..].parse().unwrap(),
        key,
        id: id.into(),
        state: state.into(),
        turn: Some("agent".into()),
        item_type: item_type.into(),
        ..ItemRow::default()
    }
}

fn parent(rid: i64, to: i64) -> Tie {
    Tie {
        rid,
        edge: Edge::Parent,
        to,
    }
}

/// A1 is the parent of T1 and of plan A2, both closed; A2 is the parent of T3, whose state is given.
fn plan_with_grandchild(t3: &str) -> Board {
    let items = vec![
        item(1, "A1", "open"),
        item(2, "T1", "done"),
        item(3, "A2", "done"),
        item(4, "T3", t3),
    ];
    Board::new(
        project(),
        items,
        vec![parent(2, 1), parent(3, 1), parent(4, 3)],
    )
}

#[test]
fn test_a_plan_counts_everything_under_it_at_any_depth() {
    let b = plan_with_grandchild("open");
    let a1 = b.get("A1").unwrap();
    let g = b.held_progress(a1);
    assert_eq!((g.done, g.dropped, g.total, g.live), (2, 0, 3, 0));
    assert!(!b.due(a1));
}

#[test]
fn test_a_plan_is_due_once_everything_under_it_is_closed() {
    let b = plan_with_grandchild("done");
    assert!(b.due(b.get("A1").unwrap()));
    assert!(!b.due(b.get("A2").unwrap()));
}

#[test]
fn test_a_claimed_plan_is_not_due() {
    let mut b = plan_with_grandchild("done");
    b.items[0].claim_branch = Some("audit/a1".into());
    assert!(!b.due(b.get("A1").unwrap()));
}

#[test]
fn test_a_plan_with_open_members_reads_under_way_without_a_wait() {
    let b = plan_with_grandchild("open");
    let a1 = b.get("A1").unwrap();
    assert!(a1.wait_on.is_none());
    assert_eq!(b.word(a1), "under way");
    let done = plan_with_grandchild("done");
    assert_eq!(done.word(done.get("A1").unwrap()), "audit due");
}

#[test]
fn test_the_plan_count_takes_open_and_audit_due_from_the_word() {
    let open = plan_with_grandchild("open");
    assert_eq!(
        open.plan_count(),
        PlanCount {
            open: 1,
            audit_due: 0
        }
    );
    let closed = plan_with_grandchild("done");
    assert_eq!(
        closed.plan_count(),
        PlanCount {
            open: 0,
            audit_due: 1
        }
    );
}

#[test]
fn test_a_plan_whose_tickets_are_all_closed_is_not_listed_as_under_way() {
    let closed = plan_with_grandchild("done");
    assert!(closed.plans_under_way().is_empty());
    let open = plan_with_grandchild("open");
    assert_eq!(open.plans_under_way().len(), 1);
}

#[test]
fn test_a_dropped_member_is_counted_apart_by_the_board_as_by_the_metrics() {
    let b = plan_with_grandchild("dropped");
    let g = b.held_progress(b.get("A1").unwrap());
    assert_eq!((g.done, g.dropped, g.total, g.counted), (2, 1, 3, 2));
    assert_eq!(
        g,
        crate::metrics::progress(&b.held_subjects(b.get("A1").unwrap()))
    );
}
