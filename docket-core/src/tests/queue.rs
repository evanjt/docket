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
            role: Some(role),
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
