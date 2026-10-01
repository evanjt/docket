use serde_json::json;

use super::*;

#[test]
fn test_dumps_spaces_separators_and_escapes_non_ascii() {
    assert_eq!(
        dumps(&json!(["high", "single"]), false),
        r#"["high", "single"]"#
    );
    assert_eq!(
        dumps(&json!({"runner": "codex", "gates": "passed"}), true),
        r#"{"gates": "passed", "runner": "codex"}"#
    );
    assert_eq!(
        dumps(&json!("caf\u{e9} \u{1f600}"), false),
        "\"caf\\u00e9 \\ud83d\\ude00\""
    );
    assert_eq!(dumps(&json!("a\"b\\c\n"), false), r#""a\"b\\c\n""#);
    assert_eq!(dumps(&json!([]), false), "[]");
    assert_eq!(
        dumps(&json!({"n": 3, "b": true, "z": null}), true),
        r#"{"b": true, "n": 3, "z": null}"#
    );
}

#[test]
fn test_data_of_reads_text_or_nothing() {
    assert_eq!(data_of(Some(r#"{"role": "review"}"#))["role"], "review");
    assert!(data_of(None).is_empty());
    assert!(data_of(Some("not json")).is_empty());
    assert!(data_of(Some("[1]")).is_empty());
}
