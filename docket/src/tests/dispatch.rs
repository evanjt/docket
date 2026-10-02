use super::*;

fn machine(name: &str, slots: i64, runners: &[&str]) -> Machine {
    Machine {
        name: name.into(),
        ssh: format!("user@{name}"),
        slots,
        runners: runners.iter().map(|r| (*r).to_string()).collect(),
        note: None,
        updated_at: String::new(),
    }
}

fn running(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
}

#[test]
fn test_role_follows_the_kind() {
    assert_eq!(role_of("audit"), "audit");
    assert_eq!(role_of("research"), "plan");
    assert_eq!(role_of("work"), "build");
    assert_eq!(role_of("decision"), "plan");
    assert_eq!(role_of("story"), "build");
}

#[test]
fn test_branch_is_the_lead_prefix_the_id_and_the_number() {
    assert_eq!(branch_for("T14", 731), "lead/t14-731");
}

#[test]
fn test_choose_takes_the_most_free_slots_with_the_runner() {
    let ms = [
        machine("alpha", 2, &["claude", "codex"]),
        machine("beta", 4, &["claude"]),
    ];
    let busy = running(&[("alpha", 1), ("beta", 1)]);
    assert_eq!(choose(&ms, &busy, "claude", "alpha").unwrap().name, "beta");
    assert_eq!(choose(&ms, &busy, "codex", "alpha").unwrap().name, "alpha");
}

#[test]
fn test_choose_never_takes_a_full_machine_or_one_without_the_runner() {
    let ms = [
        machine("alpha", 1, &["claude"]),
        machine("beta", 2, &["codex"]),
    ];
    let busy = running(&[("alpha", 1)]);
    assert!(choose(&ms, &busy, "claude", "alpha").is_none());
    assert!(choose(&ms, &running(&[("beta", 2)]), "codex", "alpha").is_none());
}

#[test]
fn test_choose_prefers_this_machine_among_equals() {
    let ms = [
        machine("alpha", 2, &["claude"]),
        machine("beta", 2, &["claude"]),
    ];
    let idle = running(&[]);
    assert_eq!(choose(&ms, &idle, "claude", "beta").unwrap().name, "beta");
    assert_eq!(choose(&ms, &idle, "claude", "gamma").unwrap().name, "alpha");
}

#[test]
fn test_quote_keeps_plain_words_and_quotes_the_rest() {
    assert_eq!(quote("lead/t14-3"), "lead/t14-3");
    assert_eq!(quote("o/p"), "o/p");
    assert_eq!(quote("two words"), "'two words'");
    assert_eq!(quote("it's"), r"'it'\''s'");
    assert_eq!(quote(""), "''");
    assert_eq!(quote("$HOME"), "'$HOME'");
}

#[test]
fn test_remote_line_finds_docket_and_quotes_each_argument() {
    let line = remote_line(&["-p".into(), "o/p".into(), "job".into(), "a b".into()]);
    assert_eq!(
        line,
        "PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:$PATH\" docket -p o/p job 'a b'"
    );
}

#[test]
fn test_via_is_here_for_this_machine_and_ssh_for_another() {
    let a = machine("alpha", 1, &["claude"]);
    assert_eq!(Via::of(&a, "alpha"), Via::Here);
    assert_eq!(Via::of(&a, "beta"), Via::Ssh("user@alpha".into()));
    assert_eq!(Via::Here.git_url("/r"), "/r");
    assert_eq!(Via::Ssh("user@alpha".into()).git_url("/r"), "user@alpha:/r");
}

#[test]
fn test_an_ssh_url_with_a_port_keeps_its_form_for_git() {
    let via = Via::Ssh("ssh://user@host:2222".into());
    assert_eq!(via.git_url("/srv/r"), "ssh://user@host:2222/srv/r");
    assert_eq!(
        Via::Ssh("ssh://user@host:2222/".into()).git_url("/srv/r"),
        "ssh://user@host:2222/srv/r"
    );
}
