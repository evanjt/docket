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
        scope: None,
        complexity: None,
        theme: None,
        rank: None,
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
    items.push(Candidate {
        scope: Some("later"),
        ..item(7, "B", Kind::Work)
    });
    items.push(item(8, "CON", Kind::Concept));
    assert_eq!(ids(next(&items, &[], &Filter::default(), 10)), vec![1]);
    let later = Filter {
        scope: Some("later"),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &later, 10)), vec![7]);
    let concepts = Filter {
        key: Some("CON"),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &concepts, 10)), vec![8]);
}

#[test]
fn test_next_orders_by_tier_then_class_then_rank_then_age() {
    let items = vec![
        Candidate {
            opened_at: "2026-01-02",
            ..item(1, "B", Kind::Work)
        },
        Candidate {
            tier: 1,
            ..item(2, "Q", Kind::Decision)
        },
        Candidate {
            rank: Some(5),
            ..item(3, "B", Kind::Work)
        },
        Candidate {
            rank: Some(1),
            opened_at: "2026-01-09",
            ..item(4, "B", Kind::Work)
        },
        item(5, "I", Kind::Research),
    ];
    assert_eq!(
        ids(next(&items, &[], &Filter::default(), 10)),
        vec![2, 4, 3, 1, 5]
    );
    assert_eq!(ids(next(&items, &[], &Filter::default(), 2)), vec![2, 4]);
    let high = Filter {
        priority: Some(1),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &high, 10)), vec![2]);
}

#[test]
fn test_next_lifts_a_ticket_to_its_open_package_tier() {
    let items = vec![
        item(1, "B", Kind::Work),
        Candidate {
            tier: 0,
            ..item(2, "PK", Kind::Package)
        },
        item(3, "B", Kind::Work),
    ];
    let rows = next(&items, &opened(3, 2), &Filter::default(), 10);
    assert_eq!(rows, vec![(3, 0), (1, 2)]);
}

#[test]
fn test_next_lists_a_package_only_when_its_members_are_all_closed() {
    let items = vec![
        item(1, "PK", Kind::Package),
        item(2, "PK", Kind::Package),
        Candidate {
            open: false,
            ..item(3, "B", Kind::Work)
        },
        item(4, "PK", Kind::Package),
        item(5, "B", Kind::Work),
    ];
    let mut ties = opened(3, 2);
    ties.extend(opened(5, 4));
    assert_eq!(ids(next(&items, &ties, &Filter::default(), 10)), vec![2, 5]);
}

#[test]
fn test_next_puts_a_fix_of_a_package_under_way_first() {
    let items = vec![
        item(1, "B", Kind::Work),
        item(2, "PK", Kind::Package),
        item(3, "B", Kind::Work),
        Candidate {
            claimed: true,
            ..item(4, "B", Kind::Work)
        },
    ];
    let mut ties = opened(3, 2);
    ties.extend(opened(4, 2));
    assert_eq!(ids(next(&items, &ties, &Filter::default(), 10)), vec![3, 1]);
}

#[test]
fn test_next_filters_by_theme_release_and_under() {
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
        theme: Some("roadmap"),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &themed, 10)), vec![1]);
    let without = Filter {
        without_theme: Some("roadmap"),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &without, 10)), vec![2, 3]);
    let release = vec!["sync".to_string()];
    let inside = Filter {
        release: Some(&release),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &inside, 10)), vec![2, 3]);
    let under: HashSet<i64> = [3].into_iter().collect();
    let below = Filter {
        under: Some(&under),
        ..Filter::default()
    };
    assert_eq!(ids(next(&items, &[], &below, 10)), vec![3]);
}

#[test]
fn test_release_of_needs_a_name_and_a_date() {
    assert_eq!(
        release_of(Some("1.0.0 2026-10-01")),
        Some(("1.0.0".to_string(), "2026-10-01".to_string()))
    );
    assert_eq!(
        release_of(Some("spring  cut 2026-10-01")),
        Some(("spring cut".to_string(), "2026-10-01".to_string()))
    );
    assert_eq!(release_of(Some("2026-10-01")), None);
    assert_eq!(release_of(Some("1.0.0 soon")), None);
    assert_eq!(release_of(None), None);
}
