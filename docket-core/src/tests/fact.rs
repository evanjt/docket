use super::*;

fn skills(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

#[test]
fn test_gaps_name_every_missing_fact_and_the_mode() {
    assert_eq!(
        gaps(&skills(&[])),
        [
            "mode is pause",
            "no land",
            "no model_build",
            "no model_review",
            "no model_plan"
        ]
    );
}

#[test]
fn test_gaps_empty_when_the_loop_can_dispatch() {
    let set = skills(&[
        ("mode", "run"),
        ("land", "make land"),
        ("model_build", "m"),
        ("model_review", "m"),
        ("model_plan", "m"),
    ]);
    assert!(gaps(&set).is_empty());
}

#[test]
fn test_gaps_put_a_brake_first() {
    let set = skills(&[("mode", "pause"), ("paused_by", "brake ratio 0.9")]);
    assert_eq!(gaps(&set)[0], "the loop paused itself: brake ratio 0.9");
    assert_eq!(gaps(&set)[1], "mode is pause");
}

#[test]
fn test_value_tells_set_default_and_unset_apart() {
    let set = skills(&[("land", "make land"), ("owner", "")]);
    assert_eq!(value(&set, "land"), Value::Set("make land".into()));
    // An empty value is unset, so the default shows.
    assert_eq!(value(&set, "owner"), Value::Default("the owner"));
    assert_eq!(value(&set, "model_build"), Value::Unset);
}

#[test]
fn test_blocks_marks_needed_facts_and_a_mode_other_than_run() {
    let set = skills(&[("land", "x")]);
    assert!(!blocks(&set, "land"));
    assert!(blocks(&set, "model_build"));
    assert!(blocks(&set, "mode"));
    assert!(!blocks(&skills(&[("mode", "run")]), "mode"));
    assert!(!blocks(&set, "traps"));
}

#[test]
fn test_every_fact_has_a_meaning_and_every_default_a_fact() {
    assert!(FACTS.iter().all(|(k, m)| !k.is_empty() && !m.is_empty()));
    assert!(DEFAULTS.iter().all(|(k, _)| meaning(k).is_some()));
    assert!(NEEDED.iter().all(|k| meaning(k).is_some()));
}

#[test]
fn test_pool_reads_pairs_and_skips_the_rest() {
    assert_eq!(
        pool("local=4 devbox=16 bad =3 x=y"),
        [("local".to_string(), 4), ("devbox".to_string(), 16)]
    );
    assert!(pool("").is_empty());
}
