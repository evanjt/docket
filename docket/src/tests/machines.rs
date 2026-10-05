use super::*;

fn machine(name: &str, ssh: &str, slots: i64, runners: &[&str], note: Option<&str>) -> Machine {
    Machine {
        name: name.into(),
        ssh: ssh.into(),
        slots,
        runners: runners.iter().map(|r| (*r).to_string()).collect(),
        note: note.map(Into::into),
        updated_at: "2026-10-02T00:00:00Z".into(),
        limits: std::collections::BTreeMap::new(),
        path: None,
    }
}

const NOW: &str = "2026-10-05T12:00:00Z";

fn two() -> Vec<Machine> {
    vec![
        machine("alpha", "dev@alpha.example", 4, &["claude", "codex"], None),
        machine(
            "beta",
            "beta.example",
            16,
            &["codex"],
            Some("the build box"),
        ),
    ]
}

#[test]
fn test_machines_text_marks_the_machine_it_runs_on() {
    assert_eq!(
        machines_text(&two(), "alpha", NOW),
        "  NAME   SLOTS  RUNNERS       SSH\n\
         * alpha      4  claude,codex  dev@alpha.example\n  \
         beta      16  codex         beta.example  the build box\n\
         * is this machine\n"
    );
}

#[test]
fn test_the_same_listing_from_another_machine_marks_that_one() {
    let text = machines_text(&two(), "beta", NOW);
    assert!(text.contains("\n  alpha "), "{text}");
    assert!(text.contains("\n* beta "), "{text}");
}

#[test]
fn test_a_listing_from_an_unregistered_machine_says_so() {
    let text = machines_text(&two(), "gamma", NOW);
    assert!(!text.contains("\n* "), "{text}");
    assert!(
        text.ends_with("gamma, this machine, is not one of them\n"),
        "{text}"
    );
}

#[test]
fn test_no_machines_says_how_to_add_one() {
    assert_eq!(
        machines_text(&[], "alpha", NOW),
        "no machines: docket machine set NAME --ssh ADDRESS --slots N --runners claude,codex\n"
    );
}

#[test]
fn test_set_request_carries_only_the_fields_given() {
    let req = set_request(
        "beta",
        None,
        Some(8),
        Some("codex, claude"),
        None,
        None,
        false,
    );
    assert_eq!(req.set.name, "beta");
    assert_eq!(req.set.ssh, None);
    assert_eq!(req.set.slots, Some(8));
    assert_eq!(
        req.set.runners,
        Some(vec!["codex".to_string(), "claude".to_string()])
    );
    assert!(!req.remove);
    assert!(set_request("beta", None, None, None, None, None, true).remove);
}

#[test]
fn test_a_runner_under_a_limit_is_listed_with_its_reset_until_it_passes() {
    let mut ms = two();
    ms[0]
        .limits
        .insert("codex".into(), "2026-10-07T18:29:00Z".into());
    let text = machines_text(&ms, "alpha", NOW);
    assert!(
        text.contains("dev@alpha.example  codex limited until 2026-10-07T18:29:00Z\n"),
        "{text}"
    );
    let after = machines_text(&ms, "alpha", "2026-10-07T18:29:00Z");
    assert!(!after.contains("limited"), "{after}");
}
