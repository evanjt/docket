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
        conflict: false,
        complexity: None,
        theme: None,
        tier: 2,
        opened_at: "2026-01-01",
    }
}

fn opened(rid: i64, to: i64) -> Vec<Tie> {
    vec![Tie {
        rid,
        opened: true,
        to,
    }]
}

fn ids(rows: Vec<(i64, usize)>) -> Vec<i64> {
    rows.into_iter().map(|(rid, _)| rid).collect()
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
    items.push(Candidate {
        conflict: true,
        ..item(6, "B", Kind::Work)
    });
    assert_eq!(ids(next(&items, &[], &Filter::default(), 10)), vec![1]);
}

#[test]
fn test_next_never_lists_a_read_only_kind() {
    let items = vec![
        item(1, "PK", Kind::Package),
        item(2, "CON", Kind::Concept),
        item(3, "CID", Kind::Idea),
        item(4, "STY", Kind::Story),
        item(5, "B", Kind::Work),
    ];
    assert_eq!(ids(next(&items, &[], &Filter::default(), 10)), vec![5]);
    let concepts = Filter {
        key: Some("CON"),
        ..Filter::default()
    };
    assert!(next(&items, &[], &concepts, 10).is_empty());
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
        vec![4, 2, 3, 5, 6, 1]
    );
    assert_eq!(ids(next(&items, &[], &Filter::default(), 2)), vec![4, 2]);
    let high = Filter {
        priority: Some(1),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &high, 10)), vec![4, 2]);
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
    let rows = next(&items, &opened(3, 2), &Filter::default(), 10);
    assert_eq!(rows, vec![(1, 2), (3, 2)]);
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
    let mut ties = opened(3, 2);
    ties.extend(opened(8, 7));
    ties.extend(opened(9, 8));
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
fn test_next_filters_by_theme_and_under() {
    let items = vec![
        Candidate {
            theme: Some("Roadmap-2"),
            ..item(1, "B", Kind::Work)
        },
        item(2, "B", Kind::Work),
        Candidate {
            theme: Some("sync"),
            ..item(3, "B", Kind::Work)
        },
    ];
    let themed = Filter {
        theme: Some("roadmap-2"),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &themed, 10)), vec![1]);
    let prefix = Filter {
        theme: Some("roadmap"),
        ..Filter::default()
    };
    assert!(next(&items, &[], &prefix, 10).is_empty());
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
fn test_release_rank_is_the_themes_place_and_the_current_release_otherwise() {
    let r = releases(&["1.0", "1.1", "1.2"]);
    assert_eq!(release_rank(&r, Some("1.2")), 2);
    assert_eq!(release_rank(&r, Some("1.0")), 0);
    assert_eq!(release_rank(&r, None), 0);
    assert_eq!(release_rank(&r, Some("docs")), 0);
    assert_eq!(release_rank(&[], Some("1.1")), 0);
}

#[test]
fn test_next_orders_by_release_before_priority() {
    let items = vec![
        Candidate {
            theme: Some("1.2"),
            tier: 0,
            ..item(1, "B", Kind::Work)
        },
        Candidate {
            theme: Some("1.1"),
            tier: 3,
            ..item(2, "B", Kind::Work)
        },
        Candidate {
            theme: Some("docs"),
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
    assert_eq!(ids(next(&items, &[], &f, 10)), vec![4, 3, 2, 1]);
    assert_eq!(
        ids(next(&items, &[], &Filter::default(), 10)),
        vec![1, 4, 3, 2]
    );
}

#[test]
fn test_next_over_several_roles_orders_by_release_then_role() {
    let items = vec![
        Candidate {
            tier: 3,
            ..item(1, "B", Kind::Work)
        },
        Candidate {
            tier: 3,
            ..item(2, "A", Kind::Audit)
        },
        Candidate {
            open: false,
            ..item(3, "B", Kind::Work)
        },
        Candidate {
            theme: Some("1.1"),
            tier: 0,
            ..item(4, "A", Kind::Audit)
        },
        Candidate {
            theme: Some("1.1"),
            tier: 0,
            ..item(5, "B", Kind::Work)
        },
        Candidate {
            theme: Some("1.1"),
            tier: 0,
            ..item(6, "A", Kind::Audit)
        },
    ];
    let mut ties = opened(3, 2);
    ties.extend(opened(5, 6));
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
            theme: Some("1.1"),
            tier: 0,
            ..item(1, "A", Kind::Audit)
        },
        Candidate {
            theme: Some("docs"),
            ..item(2, "B", Kind::Work)
        },
        Candidate {
            theme: Some("1.0"),
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
    assert_eq!(ids(next(&items, &[], &f, 10)), vec![2, 3, 4]);
}

#[test]
fn test_bare_next_leaves_out_a_plan_with_an_open_member() {
    let items = vec![
        item(1, "A", Kind::Audit),
        item(2, "B", Kind::Work),
        item(3, "A", Kind::Audit),
    ];
    let ties = opened(2, 1);
    assert_eq!(ids(next(&items, &ties, &Filter::default(), 10)), vec![2, 3]);
}

fn owed<'a>(rid: i64, key: &'static str, kind: Kind) -> OwnerRow<'a> {
    OwnerRow {
        rid,
        key,
        kind,
        waiting: false,
        derived: false,
        need: None,
        theme: None,
        tier: 2,
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
            theme: Some("0.4.1"),
            ..owed(2, "B", Kind::Work)
        },
        OwnerRow {
            need: Some("hold"),
            theme: Some("0.4.0"),
            tier: 2,
            asked_at: "2026-01-03",
            ..owed(3, "B", Kind::Work)
        },
        OwnerRow {
            need: Some("hold"),
            theme: Some("0.4.0"),
            tier: 2,
            asked_at: "2026-01-02",
            ..owed(4, "B", Kind::Work)
        },
        OwnerRow {
            need: Some("hold"),
            theme: Some("0.4.0"),
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
            theme: Some("0.4.1"),
            ..owed(1, "B", Kind::Work)
        },
        OwnerRow {
            tier: 0,
            theme: Some("0.4.0"),
            ..owed(2, "B", Kind::Work)
        },
        OwnerRow {
            waiting: true,
            ..owed(3, "PL", Kind::Audit)
        },
        OwnerRow {
            theme: Some("ci"),
            ..owed(4, "B", Kind::Work)
        },
        owed(5, "Q", Kind::Decision),
    ];
    let q = owner_queue(&rows, &OwnerFilter::of(&releases(&["0.4.0", "0.4.1"])));
    assert_eq!(owner_ids(&q), vec![5, 2, 4, 1]);
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
