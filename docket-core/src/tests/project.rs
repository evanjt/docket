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
    assert!(matches(&p, Some("new/thing"), &[], "thing").is_empty());
}

#[test]
fn test_default_keys_hold_nine_kinds() {
    let keys = default_keys();
    let keys = keys.as_array().unwrap();
    assert_eq!(keys.len(), 9);
    assert_eq!(keys[2]["turn"], "user");
}
