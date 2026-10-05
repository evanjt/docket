use super::*;
use crate::dump::DumpPage;

const PAGE: &str = include_str!("fixtures/migrate/page.json");

fn planned(rules: Rules) -> Changes {
    let page: DumpPage = serde_json::from_str(PAGE).unwrap();
    let rows = Rows::of(&page);
    plan(&rows[0], rules)
}

fn decided() -> Rules {
    Rules {
        held: Some(Held::Detach),
        areas: Some(Areas::Current),
    }
}

fn risky(c: &Changes, pick: impl Fn(&Case) -> bool) -> Vec<&Risky> {
    c.risky.iter().filter(|r| pick(&r.case)).collect()
}

fn has(c: &Changes, change: &Change) -> bool {
    c.changes.contains(change)
}

fn s(id: &str) -> String {
    id.to_string()
}

#[test]
fn test_a_plan_held_by_later_work_waits_on_the_rule_and_the_write_is_refused() {
    let c = planned(Rules::default());
    let held = risky(&c, |k| matches!(k, Case::HeldPlan { .. }));
    assert_eq!(held.len(), 1, "{:#?}", c.risky);
    assert_eq!(
        held[0].case,
        Case::HeldPlan {
            plan: s("A1"),
            release: s("0.3"),
            later: vec![(s("T2"), s("0.4"))],
        }
    );
    assert_eq!(held[0].waits, Some(Decision::Held));
    let refused = c.writable().unwrap_err().0;
    assert!(refused.contains("A1"), "{refused}");
    assert!(refused.contains("swarming"), "{refused}");
}

#[test]
fn test_each_rule_for_a_held_plan_repairs_it_its_own_way() {
    let detach = planned(decided());
    assert!(has(
        &detach,
        &Change::Origin {
            id: s("T2"),
            from: s("A1")
        }
    ));
    assert!(!has(
        &detach,
        &Change::Parent {
            id: s("T2"),
            parent: s("A1")
        }
    ));
    assert_eq!(
        risky(&detach, |k| matches!(k, Case::HeldPlan { .. }))[0].waits,
        None
    );
    detach.writable().unwrap();

    let moved = planned(Rules {
        held: Some(Held::MovePlan),
        ..decided()
    });
    assert!(has(
        &moved,
        &Change::InRelease {
            id: s("A1"),
            release: Some(s("0.4"))
        }
    ));

    let pulled = planned(Rules {
        held: Some(Held::PullChildren),
        ..decided()
    });
    assert!(has(
        &pulled,
        &Change::InRelease {
            id: s("T2"),
            release: Some(s("0.3"))
        }
    ));
    assert!(pulled.grows.contains(&s("T2")), "{:?}", pulled.grows);
}

#[test]
fn test_the_edge_that_closes_a_wait_cycle_is_kept_as_related() {
    let c = planned(decided());
    let cycles = risky(&c, |k| matches!(k, Case::Cycle { .. }));
    let found: Vec<&Case> = cycles.iter().map(|r| &r.case).collect();
    assert_eq!(
        found,
        [
            &Case::Cycle {
                id: s("T4"),
                on: s("T3"),
                path: vec![s("T3"), s("T4")],
            },
            &Case::Cycle {
                id: s("T12"),
                on: s("A2"),
                path: vec![s("A2"), s("T12")],
            },
        ]
    );
    assert!(cycles.iter().all(|r| r.action.contains("related")));
    assert!(has(
        &c,
        &Change::DependsOn {
            id: s("T3"),
            on: s("T4")
        }
    ));
    assert!(!has(
        &c,
        &Change::DependsOn {
            id: s("T4"),
            on: s("T3")
        }
    ));
    assert!(c.changes.iter().any(|ch| matches!(
        ch,
        Change::Related { id, to, .. } if id == "T4" && to == "T3"
    )));
    assert!(has(
        &c,
        &Change::Parent {
            id: s("T12"),
            parent: s("A2")
        }
    ));
}

#[test]
fn test_an_item_under_two_plans_keeps_the_earliest_and_relates_the_rest() {
    let c = planned(decided());
    let two = risky(&c, |k| matches!(k, Case::TwoPlans { .. }));
    assert_eq!(two.len(), 1, "{:#?}", c.risky);
    assert_eq!(
        two[0].case,
        Case::TwoPlans {
            id: s("T5"),
            kept: s("A1"),
            others: vec![s("A2")],
        }
    );
    assert!(has(
        &c,
        &Change::Parent {
            id: s("T5"),
            parent: s("A1")
        }
    ));
    assert!(c.changes.iter().any(|ch| matches!(
        ch,
        Change::Related { id, to, .. } if id == "T5" && to == "A2"
    )));
}

#[test]
fn test_an_area_theme_becomes_a_label_and_its_release_waits_on_the_rule() {
    let c = planned(Rules::default());
    let folded = risky(&c, |k| matches!(k, Case::FoldedTheme { .. }));
    assert_eq!(folded.len(), 1, "{:#?}", c.risky);
    assert_eq!(
        folded[0].case,
        Case::FoldedTheme {
            theme: s("swarming"),
            open: 1,
        }
    );
    assert_eq!(folded[0].waits, Some(Decision::Areas));
    assert!(has(
        &c,
        &Change::Label {
            id: s("T6"),
            label: s("area:swarming")
        }
    ));

    let current = planned(decided());
    assert!(has(
        &current,
        &Change::InRelease {
            id: s("T6"),
            release: Some(s("0.3"))
        }
    ));
    let backlog = planned(Rules {
        areas: Some(Areas::Backlog),
        ..decided()
    });
    assert!(has(
        &backlog,
        &Change::InRelease {
            id: s("T6"),
            release: None
        }
    ));
}

#[test]
fn test_a_dependency_in_a_later_release_is_pulled_in_and_counted() {
    let c = planned(decided());
    let inverted = risky(&c, |k| matches!(k, Case::Inversion { .. }));
    assert_eq!(inverted.len(), 1, "{:#?}", c.risky);
    assert_eq!(
        inverted[0].case,
        Case::Inversion {
            id: s("T9"),
            release: s("0.3"),
            on: s("T10"),
            on_release: s("0.5"),
        }
    );
    assert_eq!(c.grows, [s("T10")]);
    assert!(has(
        &c,
        &Change::InRelease {
            id: s("T10"),
            release: Some(s("0.3"))
        }
    ));
}

#[test]
fn test_the_releases_fact_comes_first_then_version_themes_in_order() {
    let c = planned(decided());
    let names: Vec<(&str, usize, bool)> = c
        .releases
        .iter()
        .map(|r| (r.name.as_str(), r.position, r.from_fact))
        .collect();
    assert_eq!(
        names,
        [("0.3", 0, true), ("0.4", 1, true), ("0.5", 2, false)]
    );
    assert!(has(
        &c,
        &Change::InRelease {
            id: s("B1"),
            release: Some(s("0.3"))
        }
    ));
}

#[test]
fn test_waits_openers_turn_tags_and_standing_kinds_map_to_the_core() {
    let c = planned(decided());
    for change in [
        Change::Parent {
            id: s("T7"),
            parent: s("A1"),
        },
        Change::Origin {
            id: s("T7"),
            from: s("Q1"),
        },
        Change::Parent {
            id: s("Q1"),
            parent: s("A1"),
        },
        Change::Assignee { id: s("Q1") },
        Change::Priority {
            id: s("B1"),
            priority: s("high"),
        },
        Change::Label {
            id: s("B1"),
            label: s("scale"),
        },
        Change::Label {
            id: s("B1"),
            label: s("group:sensors"),
        },
        Change::OwnerTask {
            id: s("T8"),
            condition: s("the guards are delivered"),
        },
        Change::GateRemoved { id: s("A1") },
        Change::Plan { id: s("STY1") },
        Change::Label {
            id: s("STY1"),
            label: s("story"),
        },
        Change::Label {
            id: s("T1"),
            label: s("con1"),
        },
        Change::BecomesLabel {
            id: s("CON1"),
            label: s("con1"),
        },
    ] {
        assert!(has(&c, &change), "missing {change:?}");
    }
    assert_eq!(
        c.labels.get("con1").map(String::as_str),
        Some("Hives stay dry")
    );
    assert!(!c.changes.iter().any(|ch| matches!(
        ch,
        Change::Label { label, .. } if label == "high"
    )));
}

#[test]
fn test_claims_past_and_present_become_assignment_rows_with_the_outcome_their_notes_name() {
    let c = planned(decided());
    let rows: Vec<&Assignment> = c
        .changes
        .iter()
        .filter_map(|ch| match ch {
            Change::Assignment(a) => Some(a),
            _ => None,
        })
        .collect();
    let of = |id: &str| -> Vec<(&str, Option<&str>)> {
        rows.iter()
            .filter(|a| a.id == id)
            .map(|a| (a.branch.as_str(), a.outcome))
            .collect()
    };
    assert_eq!(
        of("T1"),
        [("build/t1-1", Some("gate")), ("build/t1-2", Some("landed"))]
    );
    assert_eq!(of("T11"), [("build/t11-1", None)]);
}

#[test]
fn test_the_dump_format_change_is_listed_and_every_case_prints() {
    let c = planned(Rules::default());
    assert_eq!(
        risky(&c, |k| matches!(k, Case::DumpFormat { .. }))[0].case,
        Case::DumpFormat { files: 18 }
    );
    let text = c.lines().join("\n");
    for r in &c.risky {
        assert!(text.contains(&r.line()), "{} not in\n{text}", r.line());
    }
    assert!(text.contains("T3 depends on T4"), "{text}");
}

#[test]
fn test_a_project_with_no_release_sends_every_item_to_the_backlog() {
    let mut page: DumpPage = serde_json::from_str(PAGE).unwrap();
    page.projects[0].skills = serde_json::json!({});
    for i in &mut page.items {
        i.theme = None;
    }
    let c = plan(&Rows::of(&page)[0], decided());
    assert!(c.releases.is_empty());
    assert_eq!(
        risky(&c, |k| matches!(k, Case::NoRelease { .. }))[0].case,
        Case::NoRelease { open: 17 }
    );
    assert!(has(
        &c,
        &Change::InRelease {
            id: s("B1"),
            release: None
        }
    ));
}

#[test]
fn test_a_wait_on_the_later_of_two_plans_is_still_a_cycle() {
    let mut page: DumpPage = serde_json::from_str(PAGE).unwrap();
    let t12 = page.items.iter_mut().find(|i| i.id == "T12").unwrap();
    t12.opened = vec![s("A1"), s("A2")];
    let c = plan(&Rows::of(&page)[0], decided());
    assert!(
        c.risky.iter().any(|r| r.case
            == Case::Cycle {
                id: s("T12"),
                on: s("A2"),
                path: vec![s("A2"), s("T12")],
            }),
        "{:#?}",
        c.risky
    );
    assert!(has(
        &c,
        &Change::Parent {
            id: s("T12"),
            parent: s("A1")
        }
    ));
    assert!(!has(
        &c,
        &Change::DependsOn {
            id: s("T12"),
            on: s("A2")
        }
    ));
}
