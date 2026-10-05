use crate::tests_fixture::Fixture;

fn board() -> docket_core::board::Board {
    Fixture::default().board_now()
}

fn ids(items: &[&docket_core::rows::ItemRow]) -> Vec<String> {
    items.iter().map(|i| i.id.clone()).collect()
}

#[test]
fn test_word_follows_the_server_rule() {
    let b = board();
    let word = |id| b.word_of(id).unwrap();
    assert_eq!(word("T1"), "done");
    assert_eq!(word("T2"), "building");
    assert_eq!(word("T3"), "ready");
    // A package with an open member is building.
    assert_eq!(word("PK1"), "building");
    assert_eq!(word("Q1"), "parked");
    assert_eq!(word("CON1"), "standing");
    assert_eq!(word("B1"), "blocked");
    assert_eq!(b.word_of("X9"), None);
}

#[test]
fn test_with_word_leaves_packages_out_as_the_flow_does() {
    let b = board();
    assert_eq!(ids(&b.with_word("building")), ["T2"]);
    assert_eq!(ids(&b.with_word("ready")), ["A2", "T3"]);
}

#[test]
fn test_progress_by_kind() {
    let b = board();
    let of = |id| {
        let g = b.progress(b.get(id).unwrap());
        (g.done, g.total, g.live)
    };
    // PK1 holds T1 (done) and T2 (claimed).
    assert_eq!(of("PK1"), (1, 2, 1));
    // A1 holds PK1 and T3, and PK1 holds T1 and T2, at any depth.
    assert_eq!(of("A1"), (1, 4, 1));
    // CON1 is related to T3 alone.
    assert_eq!(of("CON1"), (0, 1, 0));
    assert_eq!(of("T3"), (0, 0, 0));
}

#[test]
fn test_plans_under_way_and_due_audits() {
    let b = board();
    let under: Vec<String> = b
        .plans_under_way()
        .iter()
        .map(|(p, _)| p.id.clone())
        .collect();
    assert_eq!(under, ["A1", "A2"]);
    assert_eq!(ids(&b.due_audits()), ["A2"]);
}

#[test]
fn test_children_tied_and_owner_counts() {
    let b = board();
    assert_eq!(ids(&b.children(8)), ["PK1", "T3"]);
    assert_eq!(ids(&b.tied(6)), ["T3"]);
    assert_eq!(b.on_owner(), 1);
    assert_eq!(b.undecided(), 1);
    assert!(b.is_standing(6));
    assert!(!b.is_standing(3));
}

#[test]
fn test_empty_board_has_nothing_under_way() {
    let b = docket_core::board::Board::default();
    assert!(b.plans_under_way().is_empty());
    assert!(b.due_audits().is_empty());
    assert!(b.with_word("ready").is_empty());
    assert_eq!(b.on_owner(), 0);
}

#[test]
fn test_net_counts_the_tickets_against_their_opens_and_the_idle_research() {
    let mut b = board();
    let current = docket_core::release::Release {
        name: "1.0".into(),
        ..Default::default()
    };
    b.project.releases = docket_core::release::Listed::new(vec![(1, current)]);
    for i in &mut b.items {
        i.release_id = Some(1);
    }
    let event = |seq: i64, rid: i64, at: &str, kind: &str| docket_core::rows::EventRow {
        seq,
        project: "o/p".into(),
        rid: Some(rid),
        at: at.into(),
        host: "devbox".into(),
        branch: None,
        kind: kind.into(),
        note: None,
    };
    let recent = [
        event(5, 5, "2026-10-01T10:04:00Z", "closed"),
        event(4, 7, "2026-10-01T10:03:00Z", "opened"),
        event(3, 3, "2026-10-01T10:02:00Z", "opened"),
        event(2, 1, "2026-10-01T10:01:00Z", "closed"),
        event(1, 1, "2026-10-01T08:00:00Z", "opened"),
    ];
    let since = docket_core::pace::epoch("2026-10-01T10:00:00Z").unwrap();
    assert_eq!(
        crate::load::net_of(&b, &recent, since),
        docket_core::pace::Net {
            closed: 1,
            opened: 2,
            idle: 1
        }
    );
}
