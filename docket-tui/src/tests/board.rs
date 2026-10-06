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
    assert_eq!(word("T2"), "in progress");
    assert_eq!(word("T3"), "ready");
    // A package with an open member is building.
    assert_eq!(word("PK1"), "under way");
    // A plan with open tickets reads building, never blocked.
    assert_eq!(word("A1"), "under way");
    assert_eq!(word("Q1"), "waiting on owner");
    assert_eq!(word("CON1"), "dropped");
    assert_eq!(word("B1"), "blocked");
    assert_eq!(b.word_of("X9"), None);
}

#[test]
fn test_with_word_leaves_packages_out_as_the_flow_does() {
    let b = board();
    assert_eq!(ids(&b.with_word("in progress")), ["T2"]);
    assert_eq!(ids(&b.with_word("audit due")), ["A2"]);
    assert_eq!(ids(&b.with_word("ready")), ["T3"]);
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
    assert_eq!(under, ["PK1", "A1"]);
    assert_eq!(ids(&b.due_audits()), ["A2"]);
}

#[test]
fn test_children_and_owner_counts() {
    let b = board();
    assert_eq!(ids(&b.children(8)), ["PK1", "T3"]);
    assert_eq!(b.on_owner(), 1);
    assert_eq!(b.undecided(), 1);
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
fn test_plan_rows_list_each_area_with_its_open_count_and_no_concepts() {
    let rows = crate::load::plan_rows(&board(), &std::collections::BTreeSet::default());
    let headings: Vec<String> = rows.iter().filter_map(|r| r.heading.clone()).collect();
    assert!(
        headings.iter().all(|h| !h.contains("CONCEPT")),
        "{headings:?}"
    );
    let area = |name: &str| {
        rows.iter()
            .find(|r| r.area.as_deref() == Some(name))
            .unwrap()
    };
    let (lanterns, kites) = (area("lanterns"), area("kites"));
    assert_eq!(
        (lanterns.total - lanterns.done, kites.total - kites.done),
        (3, 1)
    );
    assert!(rows.iter().all(|r| r.area.is_none() || r.heading.is_some()));
    assert!(!rows.iter().any(|r| r.id == "CON1"));
}
