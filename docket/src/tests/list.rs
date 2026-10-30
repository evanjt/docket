use docket_core::pace::epoch;

use docket_core::assignment::{Land, Stage};

use super::*;

const NOW: &str = "2026-10-29T12:00:00Z";

fn row(id: &str, word: &str) -> Row {
    let (key, num) = id.split_at(1);
    Row {
        id: id.into(),
        key: key.into(),
        num: num.parse().unwrap(),
        word: word.into(),
        priority: "normal".into(),
        ..Row::default()
    }
}

fn claimed(id: &str, word: &str, since: &str, ended: Option<&str>) -> Row {
    let mut r = row(id, word);
    r.claim_branch = Some(format!("lead/{id}"));
    r.claim_since = Some(since.into());
    r.claim_ended_at = ended.map(str::to_string);
    r
}

fn ids(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|r| r.id.as_str()).collect()
}

#[test]
fn test_wip_groups_put_each_claim_under_its_word_with_its_minutes() {
    let now = epoch(NOW).unwrap();
    let rows = vec![
        claimed(
            "T1",
            "job failed",
            "2026-10-29T08:00:00Z",
            Some("2026-10-29T11:30:00Z"),
        ),
        claimed("T2", "in progress", "2026-10-29T11:15:00Z", None),
        claimed(
            "T3",
            "to land",
            "2026-10-29T08:00:00Z",
            Some("2026-10-29T11:00:00Z"),
        ),
    ];
    let found = wip_groups(&rows, now);
    let shape: Vec<String> = found
        .iter()
        .flat_map(|(w, rows)| {
            rows.iter()
                .map(move |(r, m)| format!("{w}/{}/{}", r.id, m.unwrap_or(-1)))
        })
        .collect();
    assert_eq!(
        shape,
        ["in progress/T2/45", "to land/T3/60", "job failed/T1/30"]
    );
}

#[test]
fn test_a_wip_row_words_the_minutes_by_what_they_count_from() {
    let r = claimed("T2", "in progress", "2026-10-29T11:15:00Z", None);
    assert!(wip_row("in progress", &r, Some(45), None, 0).starts_with("T2"));
    assert!(wip_row("in progress", &r, Some(45), None, 0).contains("45 min since the claim"));
    assert!(wip_row("to land", &r, Some(60), None, 0).contains("60 min since the job ended"));
}

fn landing(stage: Stage, at: &str) -> Land {
    Land {
        stage,
        at: at.into(),
        note: None,
        batch: None,
    }
}

#[test]
fn test_a_to_land_wip_row_names_its_landing_stage_and_age() {
    let now = epoch(NOW).unwrap();
    let mut r = claimed(
        "T3",
        "to land",
        "2026-10-29T08:00:00Z",
        Some("2026-10-29T11:00:00Z"),
    );
    r.claim_land = Some(landing(Stage::GateFailed, "2026-10-29T11:48:00Z"));
    let text = wip_row("to land", &r, Some(60), None, now);
    let first = text.lines().next().unwrap();
    assert!(first.contains("60 min since the job ended"), "{text}");
    assert!(first.contains("gate failed 12m ago"), "{text}");
}

#[test]
fn test_a_list_row_names_the_landing_stage_of_a_to_land_claim() {
    let now = epoch(NOW).unwrap();
    let mut r = claimed("T3", "to land", "2026-10-29T08:00:00Z", None);
    r.claim_land = Some(landing(Stage::Conflict, "2026-10-29T11:48:00Z"));
    assert!(list_row(&r, now).contains("conflict 12m ago"));
    r.claim_land = None;
    assert!(!list_row(&r, now).contains("ago"));
}

#[test]
fn test_list_groups_in_flow_order_and_orders_rows_by_the_sort() {
    let mut a = row("T9", "ready");
    a.priority = "low".into();
    a.updated_at = "2026-10-01T00:00:00Z".into();
    let mut b = row("T2", "ready");
    b.priority = "high".into();
    b.updated_at = "2026-10-02T00:00:00Z".into();
    let mut plan = row("A1", "under way");
    plan.item_type = "plan".into();
    let rows = vec![
        row("T5", "job failed"),
        a,
        row("T7", "to land"),
        plan,
        b,
        row("T6", "blocked"),
    ];
    let by = |sort| groups(rows.clone(), None, sort);
    let names: Vec<String> = by(ListSort::Queue).into_iter().map(|(w, _)| w).collect();
    assert_eq!(
        names,
        ["ready", "to land", "job failed", "blocked", "plans"]
    );
    assert_eq!(ids(&by(ListSort::Queue)[0].1), ["T9", "T2"]);
    assert_eq!(ids(&by(ListSort::Id)[0].1), ["T2", "T9"]);
    assert_eq!(ids(&by(ListSort::Priority)[0].1), ["T2", "T9"]);
    assert_eq!(ids(&by(ListSort::Updated)[0].1), ["T2", "T9"]);
}

#[test]
fn test_list_word_keeps_one_group() {
    let rows = vec![row("T1", "ready"), row("T2", "to land")];
    let found = groups(rows, Some("to land"), ListSort::Queue);
    assert_eq!(found.len(), 1);
    assert_eq!(ids(&found[0].1), ["T2"]);
}

#[test]
fn test_a_wip_head_colours_its_word_only_for_a_terminal() {
    assert_eq!(wip_head("to land", 3, false), "to land (3)");
    assert_eq!(wip_head("to land", 3, true), "\x1b[1;92mto land\x1b[0m (3)");
    assert_eq!(
        wip_head("in progress", 1, true),
        "\x1b[1;33min progress\x1b[0m (1)"
    );
    assert_eq!(
        wip_head("job failed", 2, true),
        "\x1b[1;31mjob failed\x1b[0m (2)"
    );
}
