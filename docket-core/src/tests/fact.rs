use super::*;

fn skills(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

const MODELS_SET: &str = "high=claude:opus-x:high medium=codex:gpt-x:medium low=claude:haiku-x \
                          unrated=codex:gpt-y audit=codex:gpt-x:high plan=claude:opus-x:max \
                          lead=claude:opus-x:high";

fn model(runner: &str, name: &str, effort: Option<&str>) -> Model {
    Model {
        runner: runner.into(),
        model: name.into(),
        effort: effort.map(str::to_string),
    }
}

#[test]
fn test_gaps_name_the_mode_and_a_missing_models() {
    assert_eq!(
        gaps(&skills(&[("mode", "pause")]), &skills(&[])),
        ["mode is pause"]
    );
    assert_eq!(
        gaps(&skills(&[("mode", "drain")]), &skills(&[]))[0],
        "mode is drain"
    );
}

#[test]
fn test_gaps_empty_when_a_lead_can_dispatch() {
    assert_eq!(
        gaps(&skills(&[("models", "medium=claude:m")]), &skills(&[])).len(),
        0
    );
}

#[test]
fn test_value_tells_set_default_and_unset_apart() {
    let set = skills(&[("gates", "make test"), ("owner", "")]);
    assert_eq!(value(&set, "gates"), Value::Set("make test".into()));
    // An empty value is unset, so the default shows.
    assert_eq!(value(&set, "owner"), Value::Default("the owner"));
    assert_eq!(value(&set, "models"), Value::Default(DEFAULT_MODELS));
    assert_eq!(value(&set, "gates"), Value::Set("make test".into()));
    assert_eq!(value(&skills(&[]), "gates"), Value::Unset);
    assert_eq!(value(&set, "mode"), Value::Default("run"));
}

#[test]
fn test_every_fact_has_a_meaning_and_every_default_a_fact() {
    assert!(FACTS.iter().all(|(k, m)| !k.is_empty() && !m.is_empty()));
    assert!(DEFAULTS.iter().all(|(k, _)| meaning(k).is_some()));
    assert!(NEEDED.iter().all(|k| meaning(k).is_some()));
    assert!(RETIRED.iter().all(|k| meaning(k).is_none()));
}

#[test]
fn test_the_old_loops_facts_are_retired() {
    for key in [
        "remote",
        "mirror_exclude",
        "remote_prepare",
        "remote_setup",
        "land",
        "slice",
        "pool",
        "pool_max",
        "model_build",
        "model_review",
        "model_plan",
        "file_cap",
        "lanes",
        "packages_live",
        "ram_floor",
        "observe_cap",
        "plan_batch",
        "jobs_per_day",
        "brake_ratio",
        "loop_host",
        "poll",
        "paused_by",
        "last_tick",
    ] {
        assert!(meaning(key).is_none(), "{key}");
        assert!(RETIRED.contains(&key), "{key}");
        assert_eq!(
            refusal(check(key, "x")),
            format!("{key} was a fact of the old loop and is no longer read")
        );
    }
    for key in [
        "mode",
        "stale_claim",
        "job_timeout",
        "checkout",
        "models",
        "provision",
    ] {
        assert!(meaning(key).is_some(), "{key}");
    }
}

#[test]
fn test_known_keeps_the_facts_docket_reads_and_leaves_the_rest() {
    let stored = skills(&[("owner", "Ana"), ("pool", "a=2"), ("land", "make land")]);
    assert_eq!(known(&stored), skills(&[("owner", "Ana")]));
}

fn refusal<T: std::fmt::Debug>(r: Result<T, Refused>) -> String {
    r.unwrap_err().0
}

#[test]
fn test_check_refuses_an_unknown_key_naming_every_fact() {
    let why = refusal(check("colour", "red"));
    assert!(
        why.starts_with("colour is not a skill fact. One of: owner, worktree, merge"),
        "{why}"
    );
    assert!(
        why.ends_with("stale_claim, lead_lapse, owner_limit, failure_limit, prices, flow"),
        "{why}"
    );
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
        refusal(check("flow", "simple")),
        "flow is written by docket, not set by hand"
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
    assert_eq!(check("job_timeout", "30"), Ok(()));
    for bad in ["0", "00", "-1", "1.5", "ten", " 3"] {
        assert_eq!(
            refusal(check("stale_claim", bad)),
            format!("stale_claim is a whole number above 0, not '{bad}'")
        );
    }
}

#[test]
fn test_check_takes_a_models_value_that_reads() {
    assert_eq!(check("models", MODELS_SET), Ok(()));
    assert_eq!(check("models", "medium=claude:m"), Ok(()));
}

#[test]
fn test_check_refuses_a_models_entry_naming_it() {
    for (bad, why) in [
        ("high", "'high' is not key=runner:model[:effort]"),
        (
            "high=claude",
            "'high=claude' is not key=runner:model[:effort]",
        ),
        ("high=:m", "'high=:m' is not key=runner:model[:effort]"),
        (
            "high=claude:",
            "'high=claude:' is not key=runner:model[:effort]",
        ),
        (
            "high=claude:m:",
            "'high=claude:m:' is not key=runner:model[:effort]",
        ),
        (
            "huge=claude:m",
            "'huge=claude:m' names huge, not one of high, medium, low, unrated, audit, plan, lead",
        ),
        (
            "high=gemini:m",
            "'high=gemini:m' runs on gemini, not one of claude, codex",
        ),
        (
            "low=claude:a low=codex:b",
            "'low=codex:b' sets low a second time",
        ),
    ] {
        assert_eq!(
            refusal(check("models", bad)),
            format!("models: {why}"),
            "{bad}"
        );
    }
}

#[test]
fn test_model_for_a_build_follows_its_complexity() {
    let set = skills(&[("models", MODELS_SET)]);
    assert_eq!(
        model_for(&set, &skills(&[]), Role::Build, Some("high")),
        Some(model("claude", "opus-x", Some("high")))
    );
    assert_eq!(
        model_for(&set, &skills(&[]), Role::Build, Some("medium")),
        Some(model("codex", "gpt-x", Some("medium")))
    );
    assert_eq!(
        model_for(&set, &skills(&[]), Role::Build, Some("low")),
        Some(model("claude", "haiku-x", None))
    );
}

#[test]
fn test_model_for_unrated_falls_back_to_unrated_then_medium() {
    let set = skills(&[("models", MODELS_SET)]);
    assert_eq!(
        model_for(&set, &skills(&[]), Role::Build, None),
        Some(model("codex", "gpt-y", None))
    );
    let no_unrated = skills(&[("models", "medium=codex:gpt-x:medium high=claude:o")]);
    assert_eq!(
        model_for(&no_unrated, &skills(&[]), Role::Build, None),
        Some(model("codex", "gpt-x", Some("medium")))
    );
    assert_eq!(
        model_for(&no_unrated, &skills(&[]), Role::Build, Some("unrated")),
        Some(model("codex", "gpt-x", Some("medium")))
    );
}

#[test]
fn test_model_for_audit_and_plan_use_their_entries_then_high() {
    let set = skills(&[("models", MODELS_SET)]);
    assert_eq!(
        model_for(&set, &skills(&[]), Role::Audit, Some("low")),
        Some(model("codex", "gpt-x", Some("high")))
    );
    assert_eq!(
        model_for(&set, &skills(&[]), Role::Plan, None),
        Some(model("claude", "opus-x", Some("max")))
    );
    let builds_only = skills(&[("models", "high=claude:opus-x:high")]);
    assert_eq!(
        model_for(&builds_only, &skills(&[]), Role::Audit, None),
        Some(model("claude", "opus-x", Some("high")))
    );
}

#[test]
fn test_model_for_a_lead_uses_its_entry_then_high() {
    let set = skills(&[("models", MODELS_SET)]);
    assert_eq!(
        model_for(&set, &skills(&[]), Role::Lead, None),
        Some(model("claude", "opus-x", Some("high")))
    );
    let no_lead = skills(&[("models", "high=codex:gpt-x:xhigh low=claude:h")]);
    assert_eq!(
        model_for(&no_lead, &skills(&[]), Role::Lead, Some("low")),
        Some(model("codex", "gpt-x", Some("xhigh")))
    );
    assert_eq!(
        model_for(
            &skills(&[("models", "low=claude:h")]),
            &skills(&[]),
            Role::Lead,
            None
        ),
        None
    );
}

#[test]
fn test_model_for_is_none_without_an_entry() {
    assert_eq!(
        model_for(&skills(&[]), &skills(&[]), Role::Build, Some("high")),
        Some(model("claude", DEFAULT_MODEL, None))
    );
    let set = skills(&[("models", "high=claude:o")]);
    assert_eq!(
        model_for(&set, &skills(&[]), Role::Build, Some("low")),
        None
    );
    assert_eq!(model_for(&set, &skills(&[]), Role::Build, None), None);
}

#[test]
fn test_with_sets_and_unsets() {
    let set = skills(&[("gates", "x")]);
    assert_eq!(
        with(&set, "owner", "Ada"),
        skills(&[("gates", "x"), ("owner", "Ada")])
    );
    assert_eq!(with(&set, "gates", ""), skills(&[]));
}

#[test]
fn test_releases_are_rows_never_set_as_a_fact() {
    assert_eq!(
        refusal(check("releases", "1.0.0 1.1.0")),
        "releases are rows: docket releases add, move and ship change them"
    );
    assert_eq!(check("releases", ""), Ok(()));
}

#[test]
fn test_model_for_reads_the_owner_level_when_the_project_sets_none() {
    let owner = skills(&[("models", MODELS_SET)]);
    assert_eq!(
        model_for(&skills(&[]), &owner, Role::Build, Some("high")),
        Some(model("claude", "opus-x", Some("high")))
    );
}

#[test]
fn test_model_for_a_project_value_wins_over_the_owner_and_the_default() {
    let owner = skills(&[("models", MODELS_SET)]);
    let project = skills(&[("models", "high=codex:gpt-z")]);
    assert_eq!(
        model_for(&project, &owner, Role::Build, Some("high")),
        Some(model("codex", "gpt-z", None))
    );
    assert_eq!(model_for(&project, &owner, Role::Build, Some("low")), None);
}

#[test]
fn test_model_for_with_no_value_at_any_level_is_the_built_in_default() {
    for complexity in [Some("high"), Some("medium"), Some("low"), None] {
        assert_eq!(
            model_for(&skills(&[]), &skills(&[]), Role::Build, complexity),
            Some(model("claude", DEFAULT_MODEL, None))
        );
    }
    for role in [Role::Audit, Role::Plan, Role::Lead] {
        assert_eq!(
            model_for(&skills(&[]), &skills(&[]), role, None),
            Some(model("claude", DEFAULT_MODEL, None))
        );
    }
}

#[test]
fn test_the_built_in_models_default_reads() {
    assert_eq!(check("models", DEFAULT_MODELS), Ok(()));
}

#[test]
fn test_layer_of_names_where_each_value_came_from() {
    let project = skills(&[("mode", "drain")]);
    let owner = skills(&[("mode", "pause"), ("job_timeout", "60"), ("gates", "x")]);
    assert_eq!(
        layered(&project, &owner, "mode"),
        Some(("drain".into(), Layer::Project))
    );
    assert_eq!(
        layered(&project, &owner, "job_timeout"),
        Some(("60".into(), Layer::Owner))
    );
    assert_eq!(
        layered(&project, &owner, "stale_claim"),
        Some(("120".into(), Layer::Default))
    );
    // Only agent settings have an owner level.
    assert_eq!(layered(&project, &owner, "gates"), None);
}

#[test]
fn test_only_agent_settings_are_set_for_all_projects() {
    assert_eq!(check_owner("models", MODELS_SET), Ok(()));
    assert_eq!(check_owner("mode", ""), Ok(()));
    assert_eq!(
        refusal(check_owner("gates", "make")),
        "gates is a project fact: --all-projects takes one of models, job_timeout, stale_claim, lead_lapse, mode"
    );
    assert_eq!(
        refusal(check_owner("mode", "go")),
        "mode is one of run, drain, pause, not 'go'"
    );
}

#[test]
fn test_model_without_a_runner_takes_the_entry_on_another() {
    let set = skills(&[("models", MODELS_SET)]);
    assert_eq!(
        model_without(&set, &skills(&[]), Role::Build, Some("medium"), &["codex"]),
        Some(model("claude", "opus-x", Some("high")))
    );
    assert_eq!(
        model_without(&set, &skills(&[]), Role::Build, Some("high"), &["codex"]),
        Some(model("claude", "opus-x", Some("high")))
    );
    assert_eq!(
        model_without(&set, &skills(&[]), Role::Build, Some("low"), &[]),
        model_for(&set, &skills(&[]), Role::Build, Some("low"))
    );
    let one_runner = skills(&[("models", "high=codex:gpt-x medium=codex:gpt-y")]);
    assert_eq!(
        model_without(&one_runner, &skills(&[]), Role::Build, None, &["codex"]),
        None
    );
}

#[test]
fn test_owner_limit_defaults_to_twenty_and_is_a_count() {
    assert_eq!(default_of("owner_limit"), Some("20"));
    assert_eq!(check("owner_limit", "5"), Ok(()));
    assert!(check("owner_limit", "0").is_err());
}

#[test]
fn test_an_audit_skips_the_runner_with_the_most_assignments_under_the_plan() {
    let set = skills(&[("models", MODELS_SET)]);
    let own = skills(&[]);
    let runs =
        |c: usize, x: usize| BTreeMap::from([("claude".to_string(), c), ("codex".to_string(), x)]);
    assert_eq!(
        audit_model(&set, &own, &runs(0, 0), &[]),
        Some(model("codex", "gpt-x", Some("high")))
    );
    assert_eq!(
        audit_model(&set, &own, &runs(1, 4), &[]),
        Some(model("claude", "opus-x", Some("high")))
    );
    assert_eq!(
        audit_model(&set, &own, &runs(4, 1), &[]),
        Some(model("codex", "gpt-x", Some("high")))
    );
    let only = skills(&[("models", "high=codex:gpt-x audit=codex:gpt-x")]);
    assert_eq!(
        audit_model(&only, &own, &runs(0, 4), &[]),
        Some(model("codex", "gpt-x", None))
    );
}

#[test]
fn test_publish_names_the_published_ref_and_the_remote_ref_it_is_pushed_to() {
    let meaning = meaning("publish").unwrap();
    assert!(meaning.contains("published origin/main"), "{meaning}");
    assert!(meaning.contains("unset"), "{meaning}");
    assert_eq!(
        publish_of("published origin/main"),
        Ok(Publish {
            local: "published".into(),
            remote: "origin/main".into(),
        })
    );
    assert_eq!(
        publish(&skills(&[("publish", "out/clean  upstream/trunk")])),
        Some(Publish {
            local: "out/clean".into(),
            remote: "upstream/trunk".into(),
        })
    );
    assert_eq!(publish(&skills(&[])), None);
    assert_eq!(publish(&skills(&[("publish", "")])), None);
}

#[test]
fn test_check_refuses_a_publish_that_does_not_name_two_refs() {
    assert!(check("publish", "published origin/main").is_ok());
    assert!(check("publish", "").is_ok());
    for bad in [
        "published",
        "published origin",
        "published origin/",
        "published /main",
        "a b/c d",
        "-x origin/main",
        "pub..lished origin/main",
        "published origin/ma:in",
        "published.lock origin/main",
    ] {
        let why = refusal(check("publish", bad));
        assert!(why.starts_with("publish: "), "{bad}: {why}");
    }
}

#[test]
fn test_dispatch_refused_outside_run_at_either_level() {
    let none = BTreeMap::new();
    assert_eq!(dispatch_refused(&skills(&[]), &none), None);
    assert_eq!(dispatch_refused(&skills(&[("mode", "run")]), &none), None);
    assert_eq!(
        dispatch_refused(&skills(&[("mode", "drain")]), &none).as_deref(),
        Some("mode is drain: docket dispatch starts no job until mode is run")
    );
    assert_eq!(
        dispatch_refused(&skills(&[]), &skills(&[("mode", "pause")])).as_deref(),
        Some("mode is pause: docket dispatch starts no job until mode is run")
    );
    assert_eq!(
        dispatch_refused(&skills(&[("mode", "run")]), &skills(&[("mode", "pause")])),
        None
    );
}
