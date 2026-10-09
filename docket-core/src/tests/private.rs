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
    assert_eq!(hits(&t, "the alphabet of widgetsmith").len(), 0);
}

#[test]
fn test_a_name_with_a_capital_is_matched_as_written() {
    let t = strings(&["ORBIT", "Ada"]);
    assert_eq!(hits(&t, "an orbit of the orbit log").len(), 0);
    assert_eq!(hits(&t, "ORBIT/webapp by Ada"), ["ORBIT", "Ada"]);
    assert_eq!(hits(&t, "ada and orbit").len(), 0);
}

#[test]
fn test_ids_finds_keys_followed_by_a_number() {
    let keys = strings(&["T", "A", "PK"]);
    assert_eq!(
        ids(&keys, "// A3 principle 2, as PK12 said (T9)"),
        ["A3", "PK12", "T9"]
    );
    assert_eq!(ids(&keys, "// sha256 and AT7 and A and T-1").len(), 0);
}

#[test]
fn test_message_ids_find_a_branch_name_in_any_case() {
    let keys = strings(&["T", "PK"]);
    assert_eq!(
        message_ids(&keys, "Merge branch 'lead/t14-123' into lead/batch"),
        ["t14"]
    );
    assert_eq!(message_ids(&keys, "Closes T9 and pk12"), ["T9", "pk12"]);
    assert_eq!(
        message_ids(&keys, "Merge the job; sha256, t-1, utf8").len(),
        0
    );
    assert_eq!(ids(&keys, "lead/t14-123").len(), 0);
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
    assert_eq!(t.hits("Fix it now").len(), 0);
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
    assert_eq!(shapes("version 1.10.0.4, 8.8.8.8 and 203.0.113.7").len(), 0);
}

#[test]
fn test_ssh_hosts_are_every_alias_but_patterns() {
    let config = "Host alpha alpha-vpn\n  HostName 203.0.113.7\nHost *.lan\nhost beta\nMatch all\n";
    assert_eq!(ssh_hosts(config), ["alpha", "alpha-vpn", "beta"]);
}

#[test]
fn test_theme_group_and_release_names_are_not_terms() {
    let sent = r#"{"projects":[],"owners":[],"machines":[],"hosts":[],"keys":[],
        "themes":["Lanterns"],"groups":["harbour-lights"],"releases":["2.4","saffron"]}"#;
    let p: Private = serde_json::from_str(sent).unwrap();
    assert_eq!(terms(&p, &[], &[]).len(), 0);
}

#[test]
fn test_agent_in_a_message_is_a_tool_name_a_session_link_or_an_attribution_trailer() {
    let link = format!("https://{}.ai/code/session_01abc", "claude");
    let trailer = format!("{}-by: Some Tool <tool@example.com>", "Co-authored");
    for line in [
        "Add shelf sorting, generated with Claude",
        "Ask Codex to sort the shelf",
        &link,
        &trailer,
        "Session-Id: 4f2",
    ] {
        assert!(!agent_in_message(&[], line).is_empty(), "{line}");
    }
    assert_eq!(agent_in_message(&[], "Add shelf sorting").len(), 0);
    assert_eq!(agent_in_message(&[], "Sort the claudette shelf").len(), 0);
}

#[test]
fn test_a_public_name_allows_a_tool_name_but_not_a_link() {
    let allowed = ["claude".to_string()];
    assert_eq!(
        agent_in_message(&allowed, "Parse the Claude config").len(),
        0
    );
    let link = format!("https://{}.ai/code/session_01abc", "claude");
    assert_ne!(agent_in_message(&allowed, &link).len(), 0);
}

#[test]
fn test_agent_in_an_added_line_is_a_session_link_or_a_trailer_not_a_tool_name() {
    let link = format!("see https://{}.ai/code/session_01abc", "claude");
    assert_ne!(agent_in_line(&link).len(), 0);
    assert_eq!(agent_in_line("Claude is a name in prose").len(), 0);
}

#[test]
fn test_an_agent_instructions_path_is_a_file_or_a_directory_of_one() {
    for p in [
        "AGENTS.md",
        "docs/CLAUDE.md",
        ".claude/settings.json",
        "a/.claude/x",
    ] {
        assert!(is_agent_path(p), "{p}");
    }
    assert!(!is_agent_path("docs/agents.md.txt"));
    assert!(!is_agent_path("src/claude_client.rs"));
}
