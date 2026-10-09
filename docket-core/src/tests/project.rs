use super::*;

fn projects() -> Vec<(String, Vec<String>)> {
    vec![
        ("acme/web".into(), vec!["git@host:acme/web.git".into()]),
        ("acme/api".into(), vec![]),
        ("other/web".into(), vec![]),
    ]
}

#[test]
fn test_slug_from_url_ssh_https_and_bare() {
    assert_eq!(
        slug_from_url("git@github.com:acme/web.git").as_deref(),
        Some("acme/web")
    );
    assert_eq!(
        slug_from_url("https://github.com/acme/web/").as_deref(),
        Some("acme/web")
    );
    assert_eq!(slug_from_url("web").as_deref(), None);
}

#[test]
fn test_matches_by_slug_remote_or_last_component() {
    let p = projects();
    assert_eq!(matches(&p, Some("acme/api"), &[], "x"), vec!["acme/api"]);
    let urls = vec!["git@host:acme/web.git".to_string()];
    assert_eq!(matches(&p, None, &urls, "elsewhere"), vec!["acme/web"]);
    assert_eq!(matches(&p, None, &[], "web"), vec!["acme/web", "other/web"]);
    assert_eq!(matches(&p, Some("new/thing"), &[], "thing").len(), 0);
}

#[test]
fn test_filing_under_any_key_but_the_five_is_refused_and_each_type_key_files() {
    assert_eq!(require_fileable("B"), Ok(ItemType::Bug));
    assert_eq!(require_fileable("A"), Ok(ItemType::Plan));
    for retired in ["STY", "PK", "ZQ", "FIX"] {
        let refused = require_fileable(retired).unwrap_err();
        assert_eq!(
            refused.0,
            format!("{retired} is not one of docket's keys. File under T, B, Q, I, A.")
        );
    }
}

#[test]
fn test_slug_valid_takes_one_or_two_plain_words() {
    for ok in ["shed", "garden/shed", "garden-1/shed_2.0", "_x/y"] {
        assert!(slug_valid(ok).is_ok(), "{ok}");
    }
    for bad in [
        "",
        "/shed",
        "garden/",
        "garden/shed/door",
        ".git/shed",
        "garden/.shed",
        "garden shed",
        "@garden/shed",
    ] {
        let err = slug_valid(bad).unwrap_err().0;
        assert!(err.contains("OWNER/NAME"), "{bad}: {err}");
    }
}
