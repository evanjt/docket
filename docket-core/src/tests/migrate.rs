use super::*;

const PAGE: &str = include_str!("fixtures/migrate/page.json");

fn planned(rules: Rules) -> Changes {
    let page: OldPage = serde_json::from_str(PAGE).unwrap();
    let rows = Rows::of(&page);
    plan(&rows[0], rules)
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

/// Nothing stops the write but the items no plan or concept places.
fn writable_but_for_areas(c: &Changes) -> bool {
    !c.risky
        .iter()
        .any(|r| r.case.blocks() || r.waits.is_some_and(|d| d != Decision::Unplaced))
}

#[test]
fn test_a_plan_held_by_later_work_pulls_them_into_its_release_without_a_rule_given() {
    let c = planned(Rules::default());
    let held = risky(&c, |k| matches!(k, Case::HeldPlan { .. }));
    assert_eq!(held.len(), 1, "{:#?}", c.risky);
    assert_eq!(
        held[0].case,
        Case::HeldPlan {
            plan: s("A1"),
            release: s("0.3.0"),
            later: vec![(s("T2"), s("0.3.1"))],
        }
    );
    assert_eq!(held[0].waits, None);
    assert!(has(
        &c,
        &Change::InRelease {
            id: s("T2"),
            release: Some(s("0.3.0"))
        }
    ));
    assert!(c.grows.contains(&s("T2")), "{:?}", c.grows);
    assert!(writable_but_for_areas(&c), "{:#?}", c.risky);
}

#[test]
fn test_the_edge_that_closes_a_wait_cycle_is_kept_as_related() {
    let c = planned(Rules::default());
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
    let c = planned(Rules::default());
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
fn test_an_area_theme_keeps_its_label_and_needs_no_decision() {
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
    assert_eq!(folded[0].waits, None);
    assert_eq!(
        folded[0].action,
        "0 to their plan's release, 1 to the backlog"
    );
    assert!(writable_but_for_areas(&c), "{:#?}", c.risky);
    assert!(has(
        &c,
        &Change::Label {
            id: s("T6"),
            label: s("area:swarming")
        }
    ));
    assert!(has(
        &c,
        &Change::InRelease {
            id: s("T6"),
            release: None
        }
    ));
}

#[test]
fn test_a_dependency_in_a_later_release_is_pulled_in_and_counted() {
    let c = planned(Rules::default());
    let inverted = risky(&c, |k| matches!(k, Case::Inversion { .. }));
    assert_eq!(inverted.len(), 1, "{:#?}", c.risky);
    assert_eq!(
        inverted[0].case,
        Case::Inversion {
            id: s("T9"),
            release: s("0.3.0"),
            on: s("T10"),
            on_release: s("0.3.2"),
        }
    );
    assert_eq!(c.grows, [s("T2"), s("T10")]);
    assert!(has(
        &c,
        &Change::InRelease {
            id: s("T10"),
            release: Some(s("0.3.0"))
        }
    ));
}

#[test]
fn test_the_releases_fact_comes_first_then_version_themes_in_order() {
    let c = planned(Rules::default());
    let names: Vec<(&str, usize, bool)> = c
        .releases
        .iter()
        .map(|r| (r.name.as_str(), r.position, r.from_fact))
        .collect();
    assert_eq!(
        names,
        [("0.3.0", 0, true), ("0.3.1", 1, true), ("0.3.2", 2, false)]
    );
    assert!(has(
        &c,
        &Change::InRelease {
            id: s("B1"),
            release: Some(s("0.3.0"))
        }
    ));
}

#[test]
fn test_waits_openers_turn_tags_and_concepts_map_to_the_core() {
    let c = planned(Rules::default());
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
        Change::InArea {
            id: s("T1"),
            area: s("hives-stay-dry"),
        },
        Change::BecomesArea {
            id: s("CON1"),
            area: s("hives-stay-dry"),
        },
    ] {
        assert!(has(&c, &change), "missing {change:?}");
    }
    assert_eq!(c.areas[0].description, "Hives stay dry");
    assert!(!c.labels.contains_key("con1"), "{:?}", c.labels);
    assert!(!c.changes.iter().any(|ch| matches!(
        ch,
        Change::Label { label, .. } if label == "high"
    )));
}

#[test]
fn test_claims_past_and_present_become_assignment_rows_with_the_outcome_their_notes_name() {
    let c = planned(Rules::default());
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
    let mut page: OldPage = serde_json::from_str(PAGE).unwrap();
    page.projects[0].skills = serde_json::json!({});
    for i in &mut page.items {
        i.theme = None;
    }
    let c = plan(&Rows::of(&page)[0], Rules::default());
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
    let mut page: OldPage = serde_json::from_str(PAGE).unwrap();
    let t12 = page.items.iter_mut().find(|i| i.id == "T12").unwrap();
    t12.opened = vec![s("A1"), s("A2")];
    let c = plan(&Rows::of(&page)[0], Rules::default());
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

#[test]
fn test_an_area_themed_item_takes_its_plans_release_and_the_backlog_without_one() {
    let project = OldProject {
        slug: s("o/p"),
        keys: serde_json::json!([{"key": "T", "kind": "work"}, {"key": "A", "kind": "audit"}]),
        skills: serde_json::json!({"releases": "0.3.0 0.3.1"}),
        ..OldProject::default()
    };
    let item = |id: &str, theme: &str, opened: &[&str]| OldItem {
        project: s("o/p"),
        id: s(id),
        title: s(id),
        state: s("open"),
        theme: Some(s(theme)),
        opened: opened.iter().map(|o| s(o)).collect(),
        ..OldItem::default()
    };
    let items = [
        item("A1", "0.3.1", &[]),
        item("A2", "tooling", &["A1"]),
        item("T1", "tooling", &["A2"]),
        item("T2", "tooling", &[]),
        item("A3", "tooling", &[]),
        item("T3", "tooling", &["A3"]),
    ];
    let rows = Rows {
        project: &project,
        items: items.iter().collect(),
        events: Vec::new(),
    };
    let c = plan(&rows, Rules::default());
    for (id, release) in [
        ("A2", Some(s("0.3.1"))),
        ("T1", Some(s("0.3.1"))),
        ("T2", None),
        ("A3", None),
        ("T3", None),
    ] {
        assert!(
            has(
                &c,
                &Change::InRelease {
                    id: s(id),
                    release: release.clone()
                }
            ),
            "{id} not in {release:?}"
        );
    }
    assert!(has(
        &c,
        &Change::Label {
            id: s("T1"),
            label: s("area:tooling")
        }
    ));
}

#[test]
fn test_a_parent_already_set_is_kept_and_held_like_an_opened_one() {
    let mut page: OldPage = serde_json::from_str(PAGE).unwrap();
    let t2 = page.items.iter_mut().find(|i| i.id == "T2").unwrap();
    t2.opened.clear();
    t2.parent = Some(s("A1"));
    let c = plan(&Rows::of(&page)[0], Rules::default());
    assert!(has(
        &c,
        &Change::Parent {
            id: s("T2"),
            parent: s("A1")
        }
    ));
    let held = risky(&c, |k| matches!(k, Case::HeldPlan { .. }));
    assert_eq!(held.len(), 1, "{:#?}", c.risky);
}

fn placed_in(items: &[OldItem]) -> Changes {
    let project = OldProject {
        slug: s("o/p"),
        keys: serde_json::json!([{"key": "T", "kind": "work"}, {"key": "A", "kind": "audit"}]),
        skills: serde_json::json!({"releases": "0.3.0 0.8.0"}),
        ..OldProject::default()
    };
    let rows = Rows {
        project: &project,
        items: items.iter().collect(),
        events: Vec::new(),
    };
    plan(&rows, Rules::default())
}

fn open_item(id: &str, release: &str, opened: &[&str], waits_on: Option<&str>) -> OldItem {
    OldItem {
        project: s("o/p"),
        id: s(id),
        title: s(id),
        state: s("open"),
        theme: Some(s(release)),
        opened: opened.iter().map(|o| s(o)).collect(),
        wait_on: waits_on.map(|_| s("item")),
        wait_ref: waits_on.map(s),
        ..OldItem::default()
    }
}

fn assert_in(c: &Changes, id: &str, release: &str) {
    assert!(
        has(
            c,
            &Change::InRelease {
                id: s(id),
                release: Some(s(release))
            }
        ),
        "{id} not in {release}: {:#?}",
        c.changes
    );
}

#[test]
fn test_a_sub_plan_pulled_into_its_parents_release_takes_its_children_with_it() {
    let c = placed_in(&[
        open_item("A1", "0.3.0", &[], None),
        open_item("A2", "0.8.0", &["A1"], None),
        open_item("T1", "0.8.0", &["A2"], None),
    ]);
    assert_in(&c, "A2", "0.3.0");
    assert_in(&c, "T1", "0.3.0");
    assert_eq!(c.grows, [s("A2"), s("T1")]);
    let held = risky(&c, |k| matches!(k, Case::HeldPlan { .. }));
    assert_eq!(held.len(), 2, "{:#?}", c.risky);
}

#[test]
fn test_a_plan_pulled_in_as_a_dependency_takes_its_children_with_it() {
    let c = placed_in(&[
        open_item("T1", "0.3.0", &[], Some("A1")),
        open_item("A1", "0.8.0", &[], None),
        open_item("T2", "0.8.0", &["A1"], None),
    ]);
    assert_in(&c, "A1", "0.3.0");
    assert_in(&c, "T2", "0.3.0");
    assert_eq!(c.grows, [s("A1"), s("T2")]);
}

fn plan_with(fact: &str, themes: &[&str]) -> Changes {
    let project = OldProject {
        slug: s("o/p"),
        keys: serde_json::json!([{"key": "T", "kind": "work"}]),
        skills: serde_json::json!({ "releases": fact }),
        ..OldProject::default()
    };
    let items: Vec<OldItem> = themes
        .iter()
        .enumerate()
        .map(|(n, t)| OldItem {
            project: s("o/p"),
            id: format!("T{}", n + 1),
            title: s("item"),
            state: s("open"),
            theme: Some(s(t)),
            ..OldItem::default()
        })
        .collect();
    let rows = Rows {
        project: &project,
        items: items.iter().collect(),
        events: Vec::new(),
    };
    plan(&rows, Rules::default())
}

#[test]
fn test_a_releases_fact_out_of_version_order_is_risky_and_refuses_the_write() {
    let c = plan_with("5.0.0 4.0.0", &[]);
    let found = risky(&c, |k| matches!(k, Case::ReleaseOrder { .. }));
    assert_eq!(found.len(), 1);
    assert!(c.writable().is_err());
}

#[test]
fn test_a_releases_fact_name_that_is_no_semantic_version_is_risky() {
    let c = plan_with("4.0", &[]);
    let found = risky(&c, |k| matches!(k, Case::ReleaseName { .. }));
    assert_eq!(found.len(), 1);
    assert!(found[0].line().contains("4.0"));
    assert!(c.writable().is_err());
}

#[test]
fn test_a_version_shaped_theme_that_is_no_semantic_version_is_a_label_not_a_release() {
    let c = plan_with("4.0.0", &["v3.0"]);
    assert_eq!(c.releases.len(), 1);
    assert!(has(
        &c,
        &Change::Label {
            id: s("T1"),
            label: s("area:v3.0")
        }
    ));
    assert!(writable_but_for_areas(&c), "{:#?}", c.risky);
}

#[test]
fn test_a_semantic_theme_is_placed_in_version_order_among_the_facts_names() {
    let c = plan_with("4.0.0 5.0.0", &["4.5.0"]);
    let names: Vec<(&str, usize)> = c
        .releases
        .iter()
        .map(|r| (r.name.as_str(), r.position))
        .collect();
    assert_eq!(names, [("4.0.0", 0), ("4.5.0", 1), ("5.0.0", 2)]);
    assert!(writable_but_for_areas(&c), "{:#?}", c.risky);
}

fn crafts() -> (OldProject, Vec<OldItem>, Vec<EventDump>) {
    let project = OldProject {
        slug: s("o/crafts"),
        keys: serde_json::json!([
            {"key": "T", "kind": "work"},
            {"key": "A", "kind": "audit"},
            {"key": "CON", "kind": "concept"},
            {"key": "CID", "kind": "idea"},
        ]),
        skills: serde_json::json!({"releases": "0.1.0"}),
        ..OldProject::default()
    };
    let item = |id: &str, title: &str, related: &[&str], parent: Option<&str>| OldItem {
        project: s("o/crafts"),
        id: s(id),
        title: s(title),
        state: s("open"),
        related: related.iter().map(|r| s(r)).collect(),
        parent: parent.map(s),
        opened_at: s("2026-02-01T00:00:00Z"),
        ..OldItem::default()
    };
    let mut lanterns = item("CON1", "Lanterns: paper and wire", &["A1"], None);
    lanterns.body = s("Lamps for the night market.");
    let mut glow = item("CID1", "Every stall glows", &[], None);
    glow.body = s("Each stall lights its own lamp.\n");
    let items = vec![
        lanterns,
        item("CON2", "Kites", &["T1"], None),
        item("A1", "Light the market", &["CON1"], None),
        item("T1", "Fold the frames", &["CON2"], Some("A1")),
        item("T2", "Tie the tails", &["CON1", "CON2"], None),
        item("T3", "Sweep the stalls", &["CID1"], None),
        glow,
        done_item("T4", "Count the coins"),
        item("T5", "Hang the bunting", &[], None),
    ];
    let link = |at: &str, on: &str, to: &str| EventDump {
        project: s("o/crafts"),
        uid: format!("{at}-{on}"),
        at: s(at),
        host: s("h"),
        kind: s("edited"),
        note: Some(format!("link related {to}")),
        item: Some(s(on)),
        ..EventDump::default()
    };
    let events = vec![
        link("2026-02-04T00:00:00Z", "T2", "CON1"),
        link("2026-02-03T00:00:00Z", "CON2", "T2"),
        placed(
            "o/crafts",
            "2026-02-05T00:00:00Z",
            "T3",
            r#"{"derived": "the stalls are where the kites are sold", "area": "kites"}"#,
        ),
    ];
    (project, items, events)
}

fn done_item(id: &str, title: &str) -> OldItem {
    OldItem {
        project: s("o/crafts"),
        id: s(id),
        title: s(title),
        state: s("done"),
        opened_at: s("2026-02-01T00:00:00Z"),
        ..OldItem::default()
    }
}

fn placed(project: &str, at: &str, on: &str, data: &str) -> EventDump {
    EventDump {
        project: s(project),
        uid: format!("{at}-{on}-decided"),
        at: s(at),
        host: s("h"),
        kind: s("decided"),
        item: Some(s(on)),
        data: Some(s(data)),
        ..EventDump::default()
    }
}

fn planned_crafts() -> Changes {
    let (project, items, events) = crafts();
    let rows = Rows {
        project: &project,
        items: items.iter().collect(),
        events: events.iter().collect(),
    };
    plan(&rows, Rules::default())
}

fn area_of(c: &Changes, id: &str) -> Option<String> {
    c.changes.iter().find_map(|ch| match ch {
        Change::InArea { id: i, area } if i == id => Some(area.clone()),
        _ => None,
    })
}

#[test]
fn test_each_concept_becomes_an_area_named_by_its_title_before_the_colon() {
    let c = planned_crafts();
    assert_eq!(
        c.areas,
        [
            Area {
                name: s("lanterns"),
                description: s("Lanterns: paper and wire\n\nLamps for the night market."),
                position: 0,
                concept: Some(s("CON1")),
                history: false,
            },
            Area {
                name: s("kites"),
                description: s("Kites"),
                position: 1,
                concept: Some(s("CON2")),
                history: false,
            },
            Area {
                name: s("unsorted"),
                description: s("closed items no area claimed at the migration"),
                position: 2,
                concept: None,
                history: true,
            },
        ]
    );
    for (id, area) in [("CON1", "lanterns"), ("CON2", "kites")] {
        assert!(has(
            &c,
            &Change::BecomesArea {
                id: s(id),
                area: s(area)
            }
        ));
    }
    assert!(!c.changes.iter().any(|ch| matches!(
        ch,
        Change::BecomesLabel { id, .. } | Change::Label { id, .. } if id.starts_with("CON")
    )));
    assert!(!c.labels.contains_key("con1"), "{:?}", c.labels);
    let text = c.lines().join("\n");
    assert!(
        text.contains("area lanterns at 0, from CON1, 3 items"),
        "{text}"
    );
    assert!(
        text.contains("CON1 is dropped: became area lanterns"),
        "{text}"
    );
}

#[test]
fn test_an_item_takes_its_plans_area_over_its_own_concept_and_says_so() {
    let c = planned_crafts();
    assert_eq!(area_of(&c, "A1").as_deref(), Some("lanterns"));
    assert_eq!(area_of(&c, "T1").as_deref(), Some("lanterns"));
    let found = risky(&c, |k| matches!(k, Case::PlanArea { .. }));
    assert_eq!(found.len(), 1, "{:#?}", c.risky);
    assert_eq!(
        found[0].case,
        Case::PlanArea {
            id: s("T1"),
            plan: s("A1"),
            area: s("lanterns"),
            own: vec![s("kites")],
        }
    );
    assert_eq!(found[0].waits, None);
}

#[test]
fn test_an_item_under_no_plan_tied_to_several_concepts_takes_the_oldest_link() {
    let c = planned_crafts();
    assert_eq!(area_of(&c, "T2").as_deref(), Some("kites"));
    let found = risky(&c, |k| matches!(k, Case::SeveralConcepts { .. }));
    assert_eq!(found.len(), 1, "{:#?}", c.risky);
    assert_eq!(
        found[0].case,
        Case::SeveralConcepts {
            id: s("T2"),
            area: s("kites"),
            gave_up: vec![s("lanterns")],
        }
    );
    assert_eq!(found[0].waits, None);
}

#[test]
fn test_an_open_item_nothing_places_waits_on_an_agents_placement() {
    let c = planned_crafts();
    assert_eq!(area_of(&c, "T5"), None);
    assert_eq!(area_of(&c, "CID1").as_deref(), Some("unsorted"));
    assert!(has(
        &c,
        &Change::BecomesLabel {
            id: s("CID1"),
            label: s("goal:every-stall-glows")
        }
    ));
    assert_eq!(
        c.labels.get("goal:every-stall-glows").map(String::as_str),
        Some("Each stall lights its own lamp.")
    );
    assert!(has(
        &c,
        &Change::Label {
            id: s("T3"),
            label: s("goal:every-stall-glows")
        }
    ));
    let found = risky(&c, |k| matches!(k, Case::Unplaced { .. }));
    assert_eq!(found.len(), 1, "{:#?}", c.risky);
    assert_eq!(
        found[0].case,
        Case::Unplaced {
            open: vec![s("T5")]
        }
    );
    assert_eq!(found[0].waits, Some(Decision::Unplaced));
    assert_eq!(
        Decision::Unplaced.what(),
        "an agent's placement (docket decide ID --area NAME)"
    );
    let refused = c.writable().unwrap_err().0;
    assert!(refused.contains(Decision::Unplaced.what()), "{refused}");
    assert!(refused.contains("T5"), "{refused}");
    assert!(!refused.contains("T3"), "{refused}");
}

#[test]
fn test_an_agents_placement_places_an_open_item_as_written() {
    let c = planned_crafts();
    assert_eq!(area_of(&c, "T3").as_deref(), Some("kites"));
    assert!(has(
        &c,
        &Change::Placed {
            id: s("T3"),
            area: s("kites"),
            basis: s("the stalls are where the kites are sold"),
        }
    ));
    let text = c.lines().join("\n");
    assert!(
        text.contains("T3 placed in kites (derived: the stalls are where the kites are sold)"),
        "{text}"
    );
    assert!(
        c.areas
            .iter()
            .all(|a| a.name != "kites" || a.concept.is_some())
    );
}

#[test]
fn test_a_closed_item_nothing_places_goes_to_unsorted_marked_history_and_last() {
    let c = planned_crafts();
    assert_eq!(area_of(&c, "T4").as_deref(), Some("unsorted"));
    assert_eq!(area_of(&c, "T5"), None);
    let last = c.areas.last().unwrap();
    assert_eq!(
        last,
        &Area {
            name: s("unsorted"),
            description: s("closed items no area claimed at the migration"),
            position: 2,
            concept: None,
            history: true,
        }
    );
    assert!(c.areas[..c.areas.len() - 1].iter().all(|a| !a.history));
    let text = c.lines().join("\n");
    assert!(
        text.contains("areas: 2 from concepts, 0 from placements"),
        "{text}"
    );
    assert!(
        text.contains(
            "items: 1 by plan, 2 by concept, 1 by placement, 2 closed to unsorted, 1 open unplaced"
        ),
        "{text}"
    );
}

#[test]
fn test_a_placement_on_an_item_a_plan_or_concept_places_is_ignored_with_a_line() {
    let (project, items, mut events) = crafts();
    events.push(placed(
        "o/crafts",
        "2026-02-06T00:00:00Z",
        "T1",
        r#"{"derived": "frames are kite work", "area": "kites"}"#,
    ));
    let rows = Rows {
        project: &project,
        items: items.iter().collect(),
        events: events.iter().collect(),
    };
    let c = plan(&rows, Rules::default());
    assert_eq!(area_of(&c, "T1").as_deref(), Some("lanterns"));
    let found = risky(&c, |k| matches!(k, Case::PlacementIgnored { .. }));
    assert_eq!(found.len(), 1, "{:#?}", c.risky);
    assert_eq!(
        found[0].case,
        Case::PlacementIgnored {
            id: s("T1"),
            placed: s("kites"),
            area: s("lanterns"),
        }
    );
    assert_eq!(found[0].waits, None);
}

fn spans() -> Changes {
    let project = OldProject {
        slug: s("o/spans"),
        keys: serde_json::json!([{"key": "T", "kind": "work"}, {"key": "A", "kind": "audit"}]),
        skills: serde_json::json!({"releases": "0.1.0"}),
        ..OldProject::default()
    };
    let item = |id: &str, opened: &[&str]| OldItem {
        project: s("o/spans"),
        id: s(id),
        title: s(id),
        state: s("open"),
        opened: opened.iter().map(|o| s(o)).collect(),
        ..OldItem::default()
    };
    let items = [item("A1", &[]), item("T1", &["A1"])];
    let events = [
        placed(
            "o/spans",
            "2026-03-01T00:00:00Z",
            "A1",
            r#"{"derived": "an early guess", "area": "ropes"}"#,
        ),
        placed(
            "o/spans",
            "2026-03-02T00:00:00Z",
            "A1",
            r#"{"derived": "the plan builds crossings", "area": "bridges", "about": "rope and plank crossings"}"#,
        ),
        placed(
            "o/spans",
            "2026-03-03T00:00:00Z",
            "A1",
            r#"{"derived": "a decision with no area"}"#,
        ),
    ];
    let rows = Rows {
        project: &project,
        items: items.iter().collect(),
        events: events.iter().collect(),
    };
    plan(&rows, Rules::default())
}

#[test]
fn test_a_project_with_no_concept_takes_its_areas_from_placements_and_a_plan_passes_its_down() {
    let c = spans();
    assert_eq!(
        c.areas,
        [Area {
            name: s("bridges"),
            description: s("rope and plank crossings"),
            position: 0,
            concept: None,
            history: false,
        }]
    );
    assert_eq!(area_of(&c, "A1").as_deref(), Some("bridges"));
    assert_eq!(area_of(&c, "T1").as_deref(), Some("bridges"));
    assert!(c.writable().is_ok(), "{:#?}", c.risky);
    let text = c.lines().join("\n");
    assert!(
        text.contains("area bridges at 0, from a placement, 2 items"),
        "{text}"
    );
    assert!(
        text.contains("A1 placed in bridges (derived: the plan builds crossings)"),
        "{text}"
    );
    assert!(
        text.contains("areas: 0 from concepts, 1 from placements"),
        "{text}"
    );
    assert!(
        text.contains(
            "items: 1 by plan, 0 by concept, 1 by placement, 0 closed to unsorted, 0 open unplaced"
        ),
        "{text}"
    );
}

#[test]
fn test_a_concept_named_like_an_area_the_project_has_joins_it_and_new_ones_follow() {
    let (mut project, items, events) = crafts();
    project.areas = vec![crate::area::Area {
        name: s("Kites"),
        position: 0,
        ..crate::area::Area::default()
    }];
    let rows = Rows {
        project: &project,
        items: items.iter().collect(),
        events: events.iter().collect(),
    };
    let c = plan(&rows, Rules::default());
    let made: Vec<(&str, usize)> = c
        .areas
        .iter()
        .map(|a| (a.name.as_str(), a.position))
        .collect();
    assert_eq!(made, [("lanterns", 1), ("unsorted", 2)]);
    assert_eq!(area_of(&c, "T2").as_deref(), Some("Kites"));
}

#[test]
fn test_items_under_a_projects_own_work_key_become_tasks_labelled_with_the_key() {
    let project = OldProject {
        slug: s("o/kilns"),
        keys: serde_json::json!([
            {"key": "T", "kind": "work"},
            {"key": "ZQ", "kind": "work", "meaning": "loose ends"},
            {"key": "ZD", "kind": "decision", "meaning": "forks"},
        ]),
        skills: serde_json::json!({"releases": "0.1.0"}),
        ..OldProject::default()
    };
    let item = |id: &str, state: &str| OldItem {
        project: s("o/kilns"),
        id: s(id),
        title: s(id),
        state: s(state),
        opened_at: s("2026-02-01T00:00:00Z"),
        ..OldItem::default()
    };
    let items = [
        item("T1", "open"),
        item("ZQ1", "open"),
        item("ZQ2", "done"),
        item("ZD1", "open"),
    ];
    let rows = Rows {
        project: &project,
        items: items.iter().collect(),
        events: Vec::new(),
    };
    let c = plan(&rows, Rules::default());
    for (id, kind) in [("ZQ1", "task"), ("ZQ2", "task"), ("ZD1", "question")] {
        assert!(
            has(
                &c,
                &Change::Typed {
                    id: s(id),
                    kind: kind.into()
                }
            ),
            "{id}"
        );
    }
    assert!(
        !c.changes
            .iter()
            .any(|ch| matches!(ch, Change::Typed { id, .. } if id == "T1"))
    );
    assert!(has(
        &c,
        &Change::Label {
            id: s("ZQ2"),
            label: s("key:zq")
        }
    ));
    assert_eq!(
        c.labels.get("key:zq").map(String::as_str),
        Some("loose ends")
    );
    let own = risky(&c, |k| matches!(k, Case::OwnKey { key, .. } if key == "ZQ"));
    assert_eq!(own.len(), 1);
    assert_eq!(
        own[0].case,
        Case::OwnKey {
            key: s("ZQ"),
            meaning: s("loose ends"),
            items: 2,
            open: 1
        }
    );
    assert!(own[0].line().contains("keeps its ids"));
}

#[test]
fn test_an_open_item_with_a_stored_area_and_no_other_tie_is_placed_in_it() {
    let (project, mut items, events) = crafts();
    items.iter_mut().find(|i| i.id == "T5").unwrap().area = Some(s("kites"));
    let rows = Rows {
        project: &project,
        items: items.iter().collect(),
        events: events.iter().collect(),
    };
    let c = plan(&rows, Rules::default());
    assert_eq!(area_of(&c, "T5").as_deref(), Some("kites"));
    assert_eq!(c.placed.unplaced, 0);
    assert!(risky(&c, |k| matches!(k, Case::Unplaced { .. })).is_empty());
    assert!(c.writable().is_ok(), "{:?}", c.writable());
}

#[test]
fn test_a_dump_page_reads_the_open_attempt_as_the_turn_and_each_key_by_its_items_type() {
    use crate::dump::{DumpPage, ItemDump, ProjectDump};
    let attempts = |rows: serde_json::Value| Some(serde_json::from_value(rows).unwrap());
    let page = DumpPage {
        projects: vec![ProjectDump {
            slug: s("tide/pool"),
            ..ProjectDump::default()
        }],
        items: vec![
            ItemDump {
                project: s("tide/pool"),
                id: s("Q1"),
                state: s("open"),
                item_type: s("question"),
                assignments: attempts(serde_json::json!([
                    {"assignee": "agent", "kind": "claim", "started_at": "c1", "ended_at": "c2",
                     "host": "reef", "branch": "build/q1-1"},
                    {"assignee": "owner", "kind": "ask", "started_at": "a1", "host": "",
                     "note": "which net"},
                ])),
                ..ItemDump::default()
            },
            ItemDump {
                project: s("tide/pool"),
                id: s("T2"),
                state: s("open"),
                item_type: s("task"),
                assignments: attempts(serde_json::json!([
                    {"assignee": "agent", "kind": "claim", "started_at": "c3", "host": "reef",
                     "branch": "build/t2-1"},
                ])),
                ..ItemDump::default()
            },
        ],
        ..DumpPage::default()
    };
    let old = OldPage::of(&page);
    let q1 = &old.items[0];
    assert_eq!(
        (
            q1.turn.as_deref(),
            q1.turn_note.as_deref(),
            q1.asked_at.as_deref()
        ),
        (Some("user"), Some("which net"), Some("a1"))
    );
    assert_eq!(q1.claim_branch, None);
    let t2 = &old.items[1];
    assert_eq!(
        (
            t2.turn.as_deref(),
            t2.claim_branch.as_deref(),
            t2.claim_host.as_deref()
        ),
        (Some("agent"), Some("build/t2-1"), Some("reef"))
    );
    let keys: Vec<KeySpec> = serde_json::from_value(old.projects[0].keys.clone()).unwrap();
    let kinds: Vec<(&str, Kind)> = keys.iter().map(|k| (k.key.as_str(), k.kind)).collect();
    assert_eq!(kinds, [("Q", Kind::Decision), ("T", Kind::Work)]);
}
