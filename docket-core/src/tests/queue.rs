use super::*;

fn item(rid: i64, key: &'static str, kind: Kind) -> Candidate<'static> {
    Candidate {
        rid,
        key,
        kind,
        open: true,
        turn: Some("agent"),
        claimed: false,
        waiting: false,
        decided: true,
        complexity: None,
        release: None,
        tier: 2,
        area_tier: 2,
        area: None,
        opened_at: "2026-01-01",
    }
}

fn parent(rid: i64, to: i64) -> Vec<Tie> {
    vec![Tie {
        rid,
        edge: crate::member::Edge::Parent,
        to,
    }]
}

fn ids(rows: Vec<Ranked>) -> Vec<i64> {
    rows.into_iter().map(|r| r.rid).collect()
}

#[test]
fn test_next_skips_what_nobody_can_take() {
    let mut items = vec![item(1, "B", Kind::Work)];
    items.push(Candidate {
        open: false,
        ..item(2, "B", Kind::Work)
    });
    items.push(Candidate {
        turn: Some("user"),
        ..item(3, "B", Kind::Work)
    });
    items.push(Candidate {
        claimed: true,
        ..item(4, "B", Kind::Work)
    });
    items.push(Candidate {
        waiting: true,
        ..item(5, "B", Kind::Work)
    });
    assert_eq!(ids(next(&items, &[], &Filter::default(), 10)), vec![1]);
}

#[test]
fn test_next_never_lists_a_retired_plan_kind() {
    let items = vec![
        item(1, "PK", Kind::Package),
        item(4, "STY", Kind::Story),
        item(5, "B", Kind::Work),
    ];
    assert_eq!(ids(next(&items, &[], &Filter::default(), 10)), vec![5]);
    let packages = Filter {
        key: Some("PK"),
        ..Filter::default()
    };
    assert_eq!(next(&items, &[], &packages, 10).len(), 0);
}

#[test]
fn test_next_orders_by_priority_then_age() {
    let items = vec![
        Candidate {
            opened_at: "2026-01-02",
            ..item(1, "B", Kind::Work)
        },
        Candidate {
            tier: 1,
            opened_at: "2026-01-09",
            ..item(2, "Q", Kind::Decision)
        },
        Candidate {
            opened_at: "2026-01-01",
            ..item(3, "I", Kind::Research)
        },
        Candidate {
            tier: 1,
            opened_at: "2026-01-03",
            ..item(4, "B", Kind::Work)
        },
        item(5, "A", Kind::Audit),
        item(6, "B", Kind::Work),
    ];
    assert_eq!(
        ids(next(&items, &[], &Filter::default(), 10)),
        vec![2, 4, 3, 5, 6, 1]
    );
    assert_eq!(ids(next(&items, &[], &Filter::default(), 2)), vec![2, 4]);
    let high = Filter {
        priority: Some(1),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &high, 10)), vec![2, 4]);
}

#[test]
fn test_a_ticket_keeps_its_own_tier_inside_an_urgent_plan() {
    let items = vec![
        item(1, "B", Kind::Work),
        Candidate {
            tier: 0,
            waiting: true,
            ..item(2, "A", Kind::Audit)
        },
        item(3, "B", Kind::Work),
    ];
    let rows = next(&items, &parent(3, 2), &Filter::default(), 10);
    assert_eq!(
        rows.iter().map(|r| (r.rid, r.tier)).collect::<Vec<_>>(),
        vec![(1, 2), (3, 2)]
    );
}

#[test]
fn test_next_by_role() {
    let items = vec![
        item(1, "A", Kind::Audit),
        item(2, "A", Kind::Audit),
        Candidate {
            open: false,
            ..item(3, "B", Kind::Work)
        },
        item(4, "B", Kind::Work),
        item(5, "I", Kind::Research),
        item(6, "Q", Kind::Decision),
        item(7, "A", Kind::Audit),
        Candidate {
            open: false,
            ..item(8, "Q", Kind::Decision)
        },
        item(9, "B", Kind::Work),
    ];
    let mut ties = parent(3, 2);
    ties.extend(parent(8, 7));
    ties.extend(parent(9, 8));
    let by = |role| {
        let f = Filter {
            roles: &[role],
            ..Filter::default()
        };
        ids(next(&items, &ties, &f, 10))
    };
    assert_eq!(by(Role::Audit), vec![2]);
    assert_eq!(by(Role::Work), vec![4, 9]);
    assert_eq!(by(Role::Plan), vec![1, 5, 6]);
    assert_eq!(Role::parse("audit"), Some(Role::Audit));
    assert_eq!(Role::parse("review"), None);
}

#[test]
fn test_next_filters_by_label_and_under() {
    let items = vec![
        item(1, "B", Kind::Work),
        item(2, "B", Kind::Work),
        item(3, "B", Kind::Work),
    ];
    let carrying: HashSet<i64> = [1].into_iter().collect();
    let labelled = Filter {
        labelled: Some(&carrying),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &labelled, 10)), vec![1]);
    let none = HashSet::new();
    let unlabelled = Filter {
        labelled: Some(&none),
        ..Filter::default()
    };
    assert_eq!(next(&items, &[], &unlabelled, 10).len(), 0);
    let under: HashSet<i64> = [3].into_iter().collect();
    let below = Filter {
        under: Some(&under),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &below, 10)), vec![3]);
}

fn releases(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_string()).collect()
}

#[test]
fn test_release_rank_is_the_position_with_the_backlog_last_and_an_unlisted_release_none() {
    let r = releases(&["1.0", "1.1", "1.2"]);
    assert_eq!(release_rank(&r, Some("1.2")), Some(2));
    assert_eq!(release_rank(&r, Some("1.0")), Some(0));
    assert_eq!(release_rank(&r, None), Some(3));
    assert_eq!(release_rank(&r, Some("docs")), None);
    assert_eq!(release_rank(&[], Some("1.1")), None);
    assert_eq!(release_rank(&[], None), Some(0));
}

#[test]
fn test_an_item_in_an_unlisted_release_is_refused_not_ranked_as_current_and_the_backlog_ranks_last()
{
    let items = vec![
        item(1, "B", Kind::Work),
        Candidate {
            release: Some("0.9"),
            tier: 0,
            ..item(2, "B", Kind::Work)
        },
        Candidate {
            release: Some("1.1"),
            tier: 3,
            ..item(3, "B", Kind::Work)
        },
        Candidate {
            release: Some("1.0"),
            ..item(4, "B", Kind::Work)
        },
    ];
    let r = releases(&["1.0", "1.1"]);
    let f = Filter {
        releases: &r,
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &f, 10)), vec![4, 3, 1]);
    let current = Filter {
        current_release_only: true,
        ..f
    };
    assert_eq!(ids(next(&items, &[], &current, 10)), vec![4]);
}

#[test]
fn test_next_orders_by_release_before_priority() {
    let items = vec![
        Candidate {
            release: Some("1.2"),
            tier: 0,
            ..item(1, "B", Kind::Work)
        },
        Candidate {
            release: Some("1.1"),
            tier: 3,
            ..item(2, "B", Kind::Work)
        },
        Candidate {
            release: None,
            tier: 2,
            ..item(3, "B", Kind::Work)
        },
        Candidate {
            tier: 1,
            ..item(4, "B", Kind::Work)
        },
    ];
    let r = releases(&["1.0", "1.1", "1.2"]);
    let f = Filter {
        releases: &r,
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &f, 10)), vec![2, 1, 4, 3]);
    assert_eq!(ids(next(&items, &[], &Filter::default(), 10)), vec![4, 3]);
}

#[test]
fn test_next_over_several_roles_orders_by_release_then_role() {
    let items = vec![
        Candidate {
            release: Some("1.0"),
            tier: 3,
            ..item(1, "B", Kind::Work)
        },
        Candidate {
            release: Some("1.0"),
            tier: 3,
            ..item(2, "A", Kind::Audit)
        },
        Candidate {
            open: false,
            ..item(3, "B", Kind::Work)
        },
        Candidate {
            release: Some("1.1"),
            tier: 0,
            ..item(4, "A", Kind::Audit)
        },
        Candidate {
            release: Some("1.1"),
            tier: 0,
            ..item(5, "B", Kind::Work)
        },
        Candidate {
            release: Some("1.1"),
            tier: 0,
            ..item(6, "A", Kind::Audit)
        },
    ];
    let mut ties = parent(3, 2);
    ties.extend(parent(5, 6));
    let r = releases(&["1.0", "1.1"]);
    let roles = [Role::Audit, Role::Plan, Role::Work];
    let f = Filter {
        roles: &roles,
        releases: &r,
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &ties, &f, 10)), vec![2, 1, 4, 5]);
}

#[test]
fn test_next_current_release_only_drops_later_releases() {
    let items = vec![
        Candidate {
            release: Some("1.1"),
            tier: 0,
            ..item(1, "A", Kind::Audit)
        },
        Candidate {
            release: None,
            ..item(2, "B", Kind::Work)
        },
        Candidate {
            release: Some("1.0"),
            ..item(3, "B", Kind::Work)
        },
        item(4, "B", Kind::Work),
    ];
    let r = releases(&["1.0", "1.1"]);
    let f = Filter {
        releases: &r,
        current_release_only: true,
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &f, 10)), vec![3]);
}

#[test]
fn test_bare_next_leaves_out_a_plan_with_an_open_member() {
    let items = vec![
        item(1, "A", Kind::Audit),
        item(2, "B", Kind::Work),
        item(3, "A", Kind::Audit),
    ];
    let ties = parent(2, 1);
    assert_eq!(ids(next(&items, &ties, &Filter::default(), 10)), vec![3, 2]);
}

fn owed<'a>(rid: i64, key: &'static str, kind: Kind) -> OwnerRow<'a> {
    OwnerRow {
        rid,
        key,
        kind,
        waiting: false,
        derived: false,
        need: None,
        release: None,
        tier: 2,
        area: None,
        asked_at: "2026-01-01",
    }
}

fn owner_ids(q: &OwnerQueue) -> Vec<i64> {
    q.rows.iter().map(|(rid, _)| *rid).collect()
}

#[test]
fn test_owner_queue_groups_by_need_then_release_priority_and_age() {
    let rows = vec![
        OwnerRow {
            need: Some("act"),
            ..owed(1, "B", Kind::Work)
        },
        OwnerRow {
            need: Some("hold"),
            release: Some("0.4.1"),
            ..owed(2, "B", Kind::Work)
        },
        OwnerRow {
            need: Some("hold"),
            release: Some("0.4.0"),
            tier: 2,
            asked_at: "2026-01-03",
            ..owed(3, "B", Kind::Work)
        },
        OwnerRow {
            need: Some("hold"),
            release: Some("0.4.0"),
            tier: 2,
            asked_at: "2026-01-02",
            ..owed(4, "B", Kind::Work)
        },
        OwnerRow {
            need: Some("hold"),
            release: Some("0.4.0"),
            tier: 0,
            asked_at: "2026-01-09",
            ..owed(5, "B", Kind::Work)
        },
        owed(6, "B", Kind::Work),
        OwnerRow {
            need: Some("judge"),
            ..owed(7, "B", Kind::Work)
        },
    ];
    let q = owner_queue(&rows, &OwnerFilter::of(&releases(&["0.4.0", "0.4.1"])));
    assert_eq!(owner_ids(&q), vec![5, 4, 3, 2, 1, 7, 6]);
    let groups: Vec<&str> = q.rows.iter().map(|(_, g)| *g).collect();
    assert_eq!(
        groups,
        vec!["hold", "hold", "hold", "hold", "act", "judge", "other"]
    );
}

#[test]
fn test_owner_queue_lists_derived_answers_first() {
    let rows = vec![
        OwnerRow {
            need: Some("hold"),
            ..owed(1, "B", Kind::Work)
        },
        OwnerRow {
            derived: true,
            tier: 4,
            ..owed(2, "Q", Kind::Decision)
        },
        owed(3, "Q", Kind::Decision),
    ];
    let q = owner_queue(&rows, &OwnerFilter::of(&releases(&["0.4.0", "0.4.1"])));
    assert_eq!(owner_ids(&q), vec![2, 3, 1]);
    assert_eq!(q.rows[0].1, "derived");
    assert_eq!(q.rows[1].1, "question");
}

#[test]
fn test_owner_queue_puts_the_question_then_the_critical_current_item_first_and_drops_waiting() {
    let rows = vec![
        OwnerRow {
            release: Some("0.4.1"),
            ..owed(1, "B", Kind::Work)
        },
        OwnerRow {
            tier: 0,
            release: Some("0.4.0"),
            ..owed(2, "B", Kind::Work)
        },
        OwnerRow {
            waiting: true,
            ..owed(3, "PL", Kind::Audit)
        },
        OwnerRow {
            ..owed(4, "B", Kind::Work)
        },
        owed(5, "Q", Kind::Decision),
    ];
    let q = owner_queue(&rows, &OwnerFilter::of(&releases(&["0.4.0", "0.4.1"])));
    assert_eq!(owner_ids(&q), vec![5, 2, 1, 4]);
    assert_eq!(q.waiting, 1);
}

#[test]
fn test_owner_queue_narrows_by_key_priority_and_count() {
    let rows = vec![
        OwnerRow {
            tier: 0,
            ..owed(1, "B", Kind::Work)
        },
        owed(2, "B", Kind::Work),
        owed(3, "T", Kind::Work),
    ];
    let releases = releases(&["0.4.0", "0.4.1"]);
    let keyed = OwnerFilter {
        key: Some("B"),
        priority: Some(0),
        ..OwnerFilter::of(&releases)
    };
    assert_eq!(owner_ids(&owner_queue(&rows, &keyed)), vec![1]);
    let two = OwnerFilter {
        limit: Some(2),
        ..OwnerFilter::of(&releases)
    };
    assert_eq!(owner_ids(&owner_queue(&rows, &two)), vec![1, 2]);
}

#[test]
fn test_the_owner_limit_refuses_the_ask_past_it() {
    assert!(owner_limit_refusal(1, 2).is_none());
    let why = owner_limit_refusal(2, 2).expect("refused at the limit");
    assert!(why.contains("owner_limit"), "{why}");
    assert!(why.contains('2'), "{why}");
}

#[test]
fn test_a_current_release_ticket_ranks_ahead_of_a_later_release_plan() {
    let items = vec![
        Candidate {
            release: Some("1.1"),
            tier: 0,
            ..item(1, "I", Kind::Research)
        },
        Candidate {
            release: Some("1.0"),
            tier: 3,
            ..item(2, "B", Kind::Work)
        },
    ];
    let r = releases(&["1.0", "1.1"]);
    let f = Filter {
        releases: &r,
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &f, 10)), vec![2, 1]);
}

#[test]
fn test_an_audit_due_plan_ranks_ahead_of_a_ticket_of_equal_priority() {
    let items = vec![
        item(1, "B", Kind::Work),
        item(2, "A", Kind::Audit),
        Candidate {
            open: false,
            ..item(3, "B", Kind::Work)
        },
        item(4, "A", Kind::Audit),
    ];
    let mut ties = parent(3, 2);
    ties.extend(parent(1, 4));
    let rows = next(&items, &ties, &Filter::default(), 10);
    assert_eq!(ids(rows.clone()), vec![2, 1]);
    assert_eq!(rows[0].role, Role::Audit);
    assert_eq!(rows[1].role, Role::Work);
}

#[test]
fn test_a_plan_with_nothing_under_it_is_a_plan_row() {
    let items = vec![item(1, "A", Kind::Audit), item(2, "Q", Kind::Decision)];
    let rows = next(&items, &[], &Filter::default(), 10);
    assert_eq!(
        rows.iter().map(|r| r.role).collect::<Vec<_>>(),
        vec![Role::Plan, Role::Plan]
    );
}

#[test]
fn test_an_undecided_question_is_never_offered() {
    let items = vec![
        Candidate {
            decided: false,
            ..item(1, "Q", Kind::Decision)
        },
        item(2, "Q", Kind::Decision),
    ];
    assert_eq!(ids(next(&items, &[], &Filter::default(), 10)), vec![2]);
}

#[test]
fn test_an_item_that_unblocks_three_ranks_ahead_of_one_that_unblocks_none() {
    let items = vec![
        item(1, "B", Kind::Work),
        item(2, "B", Kind::Work),
        Candidate {
            waiting: true,
            ..item(3, "B", Kind::Work)
        },
        Candidate {
            waiting: true,
            ..item(4, "B", Kind::Work)
        },
        Candidate {
            waiting: true,
            ..item(5, "B", Kind::Work)
        },
    ];
    let deps = [(3, 2), (4, 3), (5, 2)];
    let f = Filter {
        dependencies: &deps,
        ..Filter::default()
    };
    let rows = next(&items, &[], &f, 10);
    assert_eq!(ids(rows.clone()), vec![2, 1]);
    assert_eq!(rows[0].unblocks, 3);
    assert_eq!(rows[1].unblocks, 0);
}

#[test]
fn test_next_narrows_to_one_item_when_it_is_ready() {
    let items = vec![
        item(1, "B", Kind::Work),
        item(2, "B", Kind::Work),
        Candidate {
            claimed: true,
            ..item(3, "B", Kind::Work)
        },
    ];
    let one = |rid| Filter {
        rid: Some(rid),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &one(2), 10)), vec![2]);
    assert_eq!(next(&items, &[], &one(3), 10).len(), 0);
}

#[test]
fn test_next_orders_equal_priority_by_area_then_ignores_area_for_priority() {
    let releases = vec!["alder".to_string(), "birch".to_string()];
    let filter = Filter {
        releases: &releases,
        ..Filter::default()
    };
    let at = |rid, key, tier, area_tier, stage| Candidate {
        release: stage,
        tier,
        area_tier,
        ..item(rid, key, Kind::Work)
    };
    let items = vec![
        at(1, "T1", 1, 2, Some("alder")),
        at(2, "T2", 1, 1, Some("alder")),
        at(3, "T3", 2, 1, Some("alder")),
        at(4, "T4", 1, 2, Some("alder")),
        at(5, "T5", 0, 0, Some("birch")),
    ];
    assert_eq!(ids(next(&items, &[], &filter, 10)), vec![2, 1, 4, 3, 5]);
}

#[test]
fn test_next_filters_by_the_items_release() {
    let items = vec![
        Candidate {
            release: Some("3.4.0"),
            ..item(1, "B", Kind::Work)
        },
        Candidate {
            release: Some("5.6.0"),
            ..item(2, "B", Kind::Work)
        },
    ];
    let r = releases(&["3.4.0", "5.6.0"]);
    let f = Filter {
        releases: &r,
        release: Some("3.4.0"),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &f, 10)), vec![1]);
}

#[test]
fn test_next_narrows_to_one_area_ignoring_case() {
    let items = vec![
        Candidate {
            area: Some("lanterns"),
            ..item(1, "T", Kind::Work)
        },
        Candidate {
            area: Some("kites"),
            ..item(2, "T", Kind::Work)
        },
        item(3, "T", Kind::Work),
    ];
    let kites = Filter {
        area: Some("Kites"),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &kites, 10)), vec![2]);
}

#[test]
fn test_owner_queue_narrows_to_one_area_ignoring_case() {
    let rows = vec![
        OwnerRow {
            area: Some("lanterns"),
            ..owed(1, "Q", Kind::Decision)
        },
        OwnerRow {
            area: Some("kites"),
            ..owed(2, "Q", Kind::Decision)
        },
    ];
    let releases = releases(&["9.9.9"]);
    let kites = OwnerFilter {
        area: Some("KITES"),
        ..OwnerFilter::of(&releases)
    };
    assert_eq!(owner_ids(&owner_queue(&rows, &kites)), vec![2]);
}
