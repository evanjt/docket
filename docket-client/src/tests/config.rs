use super::*;

const FILE: &str = "# docket client\nserver = http://server.example:7878/\n\nkey = filekey\n";

#[test]
fn test_resolve_reads_the_file() {
    let c = Config::resolve(None, None, Some(FILE)).unwrap();
    assert_eq!(c.server, "http://server.example:7878");
    assert_eq!(c.key, "filekey");
}

#[test]
fn test_resolve_prefers_the_environment_value_by_value() {
    let c = Config::resolve(None, Some("envkey".into()), Some(FILE)).unwrap();
    assert_eq!(c.server, "http://server.example:7878");
    assert_eq!(c.key, "envkey");
}

#[test]
fn test_resolve_skips_empty_values() {
    let c = Config::resolve(Some(String::new()), None, Some(FILE)).unwrap();
    assert_eq!(c.server, "http://server.example:7878");
}

#[test]
fn test_resolve_names_what_is_missing() {
    let e = Config::resolve(None, None, None).unwrap_err();
    assert!(e.starts_with("no server"), "{e}");
    let e = Config::resolve(Some("http://h".into()), None, Some("server = x")).unwrap_err();
    assert!(e.starts_with("no key"), "{e}");
}

#[test]
fn test_field_skips_comments_and_unknown_names() {
    assert_eq!(field("#key = no\nother = 1", "key"), None);
    assert_eq!(field("key=a=b", "key"), Some("a=b".into()));
}
