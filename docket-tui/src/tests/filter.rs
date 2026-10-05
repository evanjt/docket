use super::*;

fn q(f: &Filter) -> Vec<(&'static str, String)> {
    f.query()
}

#[test]
fn test_parse_maps_fields_to_the_servers_query_with_release_as_theme() {
    let f = Filter::parse("release:7.2.0 priority:high key:B under:A7").unwrap();
    assert_eq!(
        q(&f),
        [
            ("theme", "7.2.0".to_string()),
            ("priority", "high".to_string()),
            ("key", "B".to_string()),
            ("under", "A7".to_string()),
        ]
    );
    assert_eq!(f.label(), "release:7.2.0 priority:high key:B under:A7");
}

#[test]
fn test_parse_takes_complexity_and_an_empty_line_is_no_filter() {
    let f = Filter::parse("complexity:low").unwrap();
    assert_eq!(q(&f), [("complexity", "low".to_string())]);
    assert_eq!(Filter::parse("  ").unwrap(), Filter::default());
    assert!(q(&Filter::default()).is_empty());
    assert_eq!(Filter::default().label(), "");
}

#[test]
fn test_parse_refuses_an_unknown_field_with_the_list_of_fields() {
    let e = Filter::parse("colour:red").unwrap_err();
    assert_eq!(
        e,
        "unknown filter field colour; the fields are release, priority, key, complexity, under"
    );
}

#[test]
fn test_parse_refuses_a_word_without_a_value() {
    assert!(Filter::parse("priority").is_err());
    assert!(Filter::parse("priority:").is_err());
}

#[test]
fn test_a_later_value_replaces_an_earlier_one() {
    let f = Filter::parse("key:B key:T").unwrap();
    assert_eq!(q(&f), [("key", "T".to_string())]);
}
