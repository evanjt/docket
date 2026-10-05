use super::*;

fn private() -> Private {
    Private {
        projects: vec!["acme/widgets".into(), "acme/gizmo-api".into()],
        owners: vec!["Ada Lovelace".into()],
        machines: vec![
            ("alpha.example.org".into(), "user@203.0.113.7".into()),
            ("beta".into(), "ssh://bob@beta.lan:2222".into()),
        ],
        hosts: vec!["gamma.example.org".into()],
        keys: vec!["T".into(), "A".into(), "PK".into()],
        titles: vec![],
        themes: vec!["Lanterns".into()],
        groups: vec!["harbour-lights".into()],
        releases: vec!["2.4".into(), "saffron".into()],
    }
}

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_string()).collect()
}

#[test]
fn test_terms_take_every_part_that_names_private_work() {
    let t = terms(&private(), &strings(&["carol"]), &[]);
    for want in [
        "acme/widgets",
        "widgets",
        "acme",
        "gizmo-api",
        "Ada Lovelace",
        "Lovelace",
        "Ada",
        "alpha.example.org",
        "alpha",
        "203.0.113.7",
        "user",
        "bob",
        "beta.lan",
        "beta",
        "gamma",
        "carol",
    ] {
        assert!(t.iter().any(|x| x == want), "{want} missing from {t:?}");
    }
    assert!(!t.iter().any(|x| x == "2222" || x == "10"), "{t:?}");
}

#[test]
fn test_terms_leave_out_what_is_allowed_and_short_words() {
    let t = terms(&private(), &[], &strings(&["ACME", "user"]));
    assert!(!t.iter().any(|x| x.eq_ignore_ascii_case("acme")));
    assert!(!t.iter().any(|x| x == "user"));
    assert!(t.iter().all(|x| x.len() >= 3));
    assert!(t.iter().any(|x| x == "acme/widgets"));
}

#[test]
fn test_hits_match_whole_words_in_any_case() {
    let t = strings(&["alpha", "widgets"]);
    assert_eq!(hits(&t, "ssh alpha-1 and Widgets"), ["alpha", "widgets"]);
    assert_eq!(hits(&t, "at alpha.local"), ["alpha"]);
    assert!(hits(&t, "the alphabet of widgetsmith").is_empty());
}

#[test]
fn test_a_name_with_a_capital_is_matched_as_written() {
    let t = strings(&["ORBIT", "Ada"]);
    assert!(hits(&t, "an orbit of the orbit log").is_empty());
    assert_eq!(hits(&t, "ORBIT/webapp by Ada"), ["ORBIT", "Ada"]);
    assert!(hits(&t, "ada and orbit").is_empty());
}

#[test]
fn test_ids_finds_keys_followed_by_a_number() {
    let keys = strings(&["T", "A", "PK"]);
    assert_eq!(
        ids(&keys, "// A3 principle 2, as PK12 said (T9)"),
        ["A3", "PK12", "T9"]
    );
    assert!(ids(&keys, "// sha256 and AT7 and A and T-1").is_empty());
}

#[test]
fn test_message_ids_find_a_branch_name_in_any_case() {
    let keys = strings(&["T", "PK"]);
    assert_eq!(
        message_ids(&keys, "Merge branch 'lead/t14-123' into lead/batch"),
        ["t14"]
    );
    assert_eq!(message_ids(&keys, "Closes T9 and pk12"), ["T9", "pk12"]);
    assert!(message_ids(&keys, "Merge the job; sha256, t-1, utf8").is_empty());
    assert!(ids(&keys, "lead/t14-123").is_empty());
}

#[test]
fn test_comments_are_told_from_code() {
    assert!(is_comment("    /// The lead"));
    assert!(is_comment("# a heading"));
    assert!(is_comment("let x = 1; // see why"));
    assert!(!is_comment("let x = \"T1\";"));
}

#[test]
fn test_titles_are_found_when_long_enough_to_be_an_items_own() {
    let t = Titles::new(&strings(&[
        "The sync loses a record when two devices write at once",
        "Fix it",
    ]));
    assert_eq!(
        t.hits("// The sync loses a record when two devices write at once."),
        ["The sync loses a record when two devices write at once"]
    );
    assert!(t.hits("Fix it now").is_empty());
}

#[test]
fn test_shapes_find_private_addresses_and_tokens() {
    // Built here, so the source carries no private address or token for the check to find.
    let ten = format!("{}.0.12.4", 10);
    let home = format!("{}.168.1.20", 192);
    let inside = format!("{}.20.0.1", 172);
    let token = format!("{}_{}", "ghp", "abcdefghijklmnopqrstuvwx1234");
    assert_eq!(
        shapes(&format!("ssh to {ten} or {home}")),
        [ten.clone(), home]
    );
    assert_eq!(shapes(&format!("{inside} but not 172.32.0.1")), [inside]);
    assert_eq!(shapes(&format!("token {token}")), [token]);
    assert!(shapes("version 1.10.0.4, 8.8.8.8 and 203.0.113.7").is_empty());
}

#[test]
fn test_ssh_hosts_are_every_alias_but_patterns() {
    let config = "Host alpha alpha-vpn\n  HostName 203.0.113.7\nHost *.lan\nhost beta\nMatch all\n";
    assert_eq!(ssh_hosts(config), ["alpha", "alpha-vpn", "beta"]);
}

#[test]
fn test_terms_take_themes_groups_and_releases() {
    let t = terms(&private(), &[], &[]);
    for want in ["Lanterns", "harbour-lights", "2.4", "saffron"] {
        assert!(t.iter().any(|x| x == want), "{want} missing from {t:?}");
    }
}
