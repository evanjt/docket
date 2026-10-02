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
fn test_ids_finds_keys_followed_by_a_number() {
    let keys = strings(&["T", "A", "PK"]);
    assert_eq!(
        ids(&keys, "// A7 principle 3, as PK12 said (T43)"),
        ["A7", "PK12", "T43"]
    );
    assert!(ids(&keys, "// sha256 and AT7 and A and T-1").is_empty());
}

#[test]
fn test_comments_are_told_from_code() {
    assert!(is_comment("    /// The lead"));
    assert!(is_comment("# a heading"));
    assert!(is_comment("let x = 1; // see why"));
    assert!(!is_comment("let x = \"T1\";"));
}
