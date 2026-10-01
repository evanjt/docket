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

fn refusal(r: Result<(), Refused>) -> String {
    r.unwrap_err().0
}

#[test]
fn test_check_refuses_an_unknown_key_naming_every_fact() {
    let why = refusal(check("colour", "red"));
    assert!(
        why.starts_with("colour is not a skill fact. One of: owner, worktree, merge"),
        "{why}"
    );
    assert!(why.ends_with("paused_by, last_tick"), "{why}");
}

#[test]
fn test_check_lets_an_empty_value_unset_any_fact() {
    for (key, _) in FACTS {
        assert_eq!(check(key, ""), Ok(()), "{key}");
    }
}

#[test]
fn test_check_refuses_facts_docket_writes() {
    assert_eq!(
        refusal(check("paused_by", "me")),
        "paused_by is written by docket, not set by hand"
    );
}

#[test]
fn test_check_holds_mode_to_its_choices() {
    assert_eq!(check("mode", "drain"), Ok(()));
    assert_eq!(
        refusal(check("mode", "go")),
        "mode is one of run, drain, pause, not 'go'"
    );
}

#[test]
fn test_check_counts_are_whole_numbers_above_zero() {
    assert_eq!(check("poll", "30"), Ok(()));
    for bad in ["0", "00", "-1", "1.5", "ten", " 3"] {
        assert_eq!(
            refusal(check("poll", bad)),
            format!("poll is a whole number above 0, not '{bad}'")
        );
    }
}

#[test]
fn test_check_models_take_a_model_and_an_optional_effort() {
    assert_eq!(check("model_build", "gpt-5.4"), Ok(()));
    assert_eq!(check("model_build", "gpt-5.4 medium"), Ok(()));
    assert_eq!(
        refusal(check("model_plan", "a b c")),
        "model_plan is a model and an optional effort, as \"gpt-5.4 medium\", not 'a b c'"
    );
}

#[test]
fn test_check_brake_ratio_is_a_number() {
    assert_eq!(check("brake_ratio", "0.25"), Ok(()));
    assert_eq!(
        refusal(check("brake_ratio", "half")),
        "brake_ratio is a number, as 0.5, not 'half'"
    );
}

#[test]
fn test_pool_of_refuses_a_pair_that_does_not_read() {
    assert_eq!(
        pool_of("local=4 devbox=16").unwrap(),
        [("local".to_string(), 4), ("devbox".to_string(), 16)]
    );
    assert!(pool_of("").unwrap().is_empty());
    for bad in ["local", "=3", "local=x", "local=4 bad"] {
        assert_eq!(
            refusal(pool_of(bad).map(|_| ())),
            format!(
                "pool is host=slots pairs separated by spaces, as \"local=4 devbox=16\", not '{bad}'"
            )
        );
    }
}

#[test]
fn test_ceiling_refuses_a_pool_above_pool_max() {
    let set = skills(&[("pool_max", "6")]);
    assert_eq!(ceiling(&set, "pool", "local=2 devbox=4"), Ok(()));
    assert_eq!(
        refusal(ceiling(&set, "pool", "local=3 devbox=4")),
        "the pool would hold 7 slots, above pool_max 6, the owner's ceiling"
    );
}

#[test]
fn test_ceiling_refuses_a_pool_max_below_the_stored_pool() {
    let set = skills(&[("pool", "local=4 devbox=4")]);
    assert_eq!(ceiling(&set, "pool_max", "8"), Ok(()));
    assert_eq!(
        refusal(ceiling(&set, "pool_max", "5")),
        "the pool would hold 8 slots, above pool_max 5, the owner's ceiling"
    );
    // With no pool stored the default is not counted.
    assert_eq!(ceiling(&skills(&[]), "pool_max", "1"), Ok(()));
}

#[test]
fn test_ceiling_lets_a_pool_be_unset() {
    let set = skills(&[("pool_max", "1"), ("pool", "local=1")]);
    assert_eq!(ceiling(&set, "pool", ""), Ok(()));
}

#[test]
fn test_owner_only_names_pool_max_alone() {
    assert_eq!(
        owner_only("pool_max").as_deref(),
        Some("pool_max is the owner's ceiling on the pool, and a fleet job does not set it")
    );
    assert_eq!(owner_only("pool"), None);
}

#[test]
fn test_with_sets_and_unsets() {
    let set = skills(&[("land", "x")]);
    assert_eq!(
        with(&set, "owner", "Ada"),
        skills(&[("land", "x"), ("owner", "Ada")])
    );
    assert_eq!(with(&set, "land", ""), skills(&[]));
}
