use super::*;

fn alpha() -> Machine {
    Machine {
        name: "alpha".into(),
        ssh: "dev@alpha.example".into(),
        slots: 4,
        runners: vec!["claude".into(), "codex".into()],
        note: None,
        updated_at: "2026-10-02T00:00:00Z".into(),
    }
}

fn set(name: &str) -> Set {
    Set {
        name: name.into(),
        ..Set::default()
    }
}

fn refusal(r: Result<Machine, Refused>) -> String {
    r.unwrap_err().0
}

#[test]
fn test_a_new_machine_needs_its_address_slots_and_runners() {
    assert_eq!(
        refusal(merged(None, &set("beta"), "t")),
        "a new machine needs --ssh, --slots and --runners; beta has no ssh, slots, runners"
    );
    let partial = Set {
        ssh: Some("beta.example".into()),
        ..set("beta")
    };
    assert_eq!(
        refusal(merged(None, &partial, "t")),
        "a new machine needs --ssh, --slots and --runners; beta has no slots, runners"
    );
}

#[test]
fn test_a_new_machine_with_everything_is_stored_as_given() {
    let full = Set {
        ssh: Some("beta.example".into()),
        slots: Some(2),
        runners: Some(vec!["codex".into()]),
        note: Some("the spare box".into()),
        ..set("beta")
    };
    assert_eq!(
        merged(None, &full, "2026-10-02T01:00:00Z").unwrap(),
        Machine {
            name: "beta".into(),
            ssh: "beta.example".into(),
            slots: 2,
            runners: vec!["codex".into()],
            note: Some("the spare box".into()),
            updated_at: "2026-10-02T01:00:00Z".into(),
        }
    );
}

#[test]
fn test_a_set_on_a_known_machine_changes_only_what_it_names() {
    let slots = Set {
        slots: Some(8),
        ..set("alpha")
    };
    let out = merged(Some(&alpha()), &slots, "2026-10-02T02:00:00Z").unwrap();
    assert_eq!(out.slots, 8);
    assert_eq!(out.ssh, "dev@alpha.example");
    assert_eq!(out.runners, ["claude", "codex"]);
    assert_eq!(out.updated_at, "2026-10-02T02:00:00Z");
}

#[test]
fn test_an_empty_note_clears_it() {
    let mut had = alpha();
    had.note = Some("old".into());
    let clear = Set {
        note: Some(String::new()),
        ..set("alpha")
    };
    assert_eq!(merged(Some(&had), &clear, "t").unwrap().note, None);
}

#[test]
fn test_check_refuses_each_value_a_machine_cannot_hold() {
    let with = |f: fn(&mut Machine)| {
        let mut m = alpha();
        f(&mut m);
        check(&m).unwrap_err().0
    };
    assert_eq!(with(|m| m.name = String::new()), "a machine needs a name");
    assert_eq!(
        with(|m| m.name = "al pha".into()),
        "a machine's name is one word, the host its key names, not 'al pha'"
    );
    assert_eq!(
        with(|m| m.ssh = " ".into()),
        "alpha needs an ssh address the other machines reach it at"
    );
    assert_eq!(with(|m| m.slots = 0), "alpha's slots are 1 to 64, not 0");
    assert_eq!(with(|m| m.slots = 65), "alpha's slots are 1 to 64, not 65");
    assert_eq!(
        with(|m| m.runners = vec![]),
        "alpha needs at least one runner: claude, codex"
    );
    assert_eq!(
        with(|m| m.runners = vec!["claude".into(), "remote".into()]),
        "alpha's runners are claude, codex, not 'remote'"
    );
    assert_eq!(
        with(|m| m.runners = vec!["codex".into(), "codex".into()]),
        "alpha names codex twice"
    );
    assert_eq!(check(&alpha()), Ok(()));
}

#[test]
fn test_runners_read_from_a_comma_list() {
    assert_eq!(runners_of("claude, codex"), ["claude", "codex"]);
    assert_eq!(runners_of("codex"), ["codex"]);
    assert!(runners_of(" , ").is_empty());
}
