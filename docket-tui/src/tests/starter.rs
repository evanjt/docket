use std::cell::RefCell;

use docket_core::lead::Lead;

use super::*;
use crate::tests_fixture::Fixture;

const T0: i64 = 1_790_000_000;

fn bound(slug: &str) -> Vec<String> {
    vec![format!("/work/{slug}")]
}

fn claim(renewed: i64) -> Lead {
    Lead {
        project: "o/q".into(),
        host: "beta".into(),
        session: "lead-2".into(),
        branch: None,
        since: String::new(),
        renewed_at: docket_core::clock::stamp(u64::try_from(renewed).unwrap()),
    }
}

/// Ticks once and returns the slugs a launch was made for.
fn tick_once(
    source: &Fixture,
    starts: &mut Starts,
    now: i64,
    result: &Result<String, String>,
) -> Vec<String> {
    let launched = RefCell::new(Vec::new());
    tick(source, &bound, starts, now, &|start: &Start| {
        launched.borrow_mut().push(start.slug.clone());
        result.clone()
    });
    launched.into_inner()
}

#[test]
fn test_no_claim_and_mode_run_starts_one_lead_and_a_second_refresh_none() {
    let source = Fixture::default();
    let mut starts = Starts::default();
    let ok = Ok("started lead-o-q".to_string());
    assert_eq!(tick_once(&source, &mut starts, T0, &ok), ["o/q"]);
    assert!(tick_once(&source, &mut starts, T0 + 5, &ok).is_empty());
    assert!(tick_once(&source, &mut starts, T0 + 9 * 60, &ok).is_empty());
    assert_eq!(starts.of("o/q").unwrap().outcome, Some(ok));
}

#[test]
fn test_a_failed_start_is_not_retried_within_the_window() {
    let source = Fixture::default();
    let mut starts = Starts::default();
    let bad = Err("no checkout".to_string());
    assert_eq!(tick_once(&source, &mut starts, T0, &bad), ["o/q"]);
    assert!(tick_once(&source, &mut starts, T0 + 60, &bad).is_empty());
    assert_eq!(tick_once(&source, &mut starts, T0 + 600, &bad), ["o/q"]);
}

#[test]
fn test_a_live_claim_starts_nothing_and_the_reason_is_kept() {
    let source = Fixture::default();
    *source.lead.lock().unwrap() = Some(claim(T0 - 60));
    let mut starts = Starts::default();
    assert!(tick_once(&source, &mut starts, T0, &Ok(String::new())).is_empty());
    assert_eq!(
        starts.of("o/q").unwrap().why,
        ["lead-2 on beta holds the lead"]
    );
}

#[test]
fn test_a_lapsed_claim_is_started_over() {
    let source = Fixture::default();
    *source.lead.lock().unwrap() = Some(claim(T0 - 11 * 60));
    let mut starts = Starts::default();
    assert_eq!(
        tick_once(&source, &mut starts, T0, &Ok(String::new())),
        ["o/q"]
    );
}

#[test]
fn test_a_project_without_models_or_not_bound_here_starts_nothing() {
    let source = Fixture::default();
    let mut starts = Starts::default();
    assert!(
        tick_once(&source, &mut starts, T0, &Ok(String::new()))
            .iter()
            .all(|s| s != "o/p")
    );
    assert_eq!(starts.of("o/p").unwrap().why, ["no models"]);
    let none = |_: &str| Vec::new();
    tick(&source, &none, &mut Starts::default(), T0, &|_: &Start| {
        panic!("started a lead in a project with no root here")
    });
}

fn set(slot: &std::sync::Mutex<Vec<(String, String)>>, key: &str, value: &str) {
    slot.lock().unwrap().push((key.into(), value.into()));
}

#[test]
fn test_an_owner_level_pause_stops_a_project_that_leaves_mode_unset() {
    let source = Fixture::default();
    set(&source.facts, "models", "medium=claude:m lead=claude:m");
    set(&source.owner, "mode", "pause");
    let mut starts = Starts::default();
    let launched = tick_once(&source, &mut starts, T0, &Ok(String::new()));
    assert_eq!(launched, ["o/q"]);
    assert_eq!(starts.of("o/p").unwrap().why, ["mode is pause"]);
}

#[test]
fn test_an_owner_level_models_lets_a_project_without_models_start_on_its_lead_entry() {
    let source = Fixture::default();
    set(
        &source.owner,
        "models",
        "medium=claude:m lead=claude:owner-lead",
    );
    let launched = RefCell::new(Vec::new());
    tick(
        &source,
        &bound,
        &mut Starts::default(),
        T0,
        &|start: &Start| {
            launched
                .borrow_mut()
                .push((start.slug.clone(), start.model.model.clone()));
            Ok(String::new())
        },
    );
    assert!(
        launched
            .into_inner()
            .contains(&("o/p".to_string(), "owner-lead".to_string()))
    );
}
