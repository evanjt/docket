use super::*;

#[test]
fn test_fts_query_quotes_each_word() {
    assert_eq!(
        fts_query("  sync  \"ok\" "),
        Some(r#""sync" """ok""""#.to_string())
    );
    assert_eq!(fts_query("   "), None);
}

#[test]
fn test_any_of_joins_with_or() {
    let terms = ["a".to_string(), "b c".to_string()];
    assert_eq!(any_of(&terms), r#""a" OR "b c""#);
}

#[test]
fn test_is_id_shape() {
    assert!(is_id("B14"));
    assert!(is_id("STY2"));
    assert!(!is_id("ABCD1"));
    assert!(!is_id("B"));
    assert!(!is_id("14"));
    assert!(!is_id("b14"));
}

#[test]
fn test_similar_terms_from_title_files_and_symbols() {
    let paths = ["src/docket/cli.py".to_string(), "a/b/c.rs".to_string()];
    let body = "Calls `model.word` and `cli::next_rows`, cites `B14` and `_x`; `bad-one` stays out";
    let terms = similar_terms("The sync of the Readings' queue: ok an", &paths, body);
    assert_eq!(
        terms,
        vec![
            "sync",
            "Readings'",
            "queue",
            "cli.py",
            "c.rs",
            "word",
            "next_rows"
        ]
    );
}

#[test]
fn test_similar_terms_dedupes_without_case_and_caps_at_thirty() {
    let title = (1..=40)
        .map(|i| format!("word{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    let paths = ["Word1".to_string()];
    let terms = similar_terms(&title, &paths, "`WORD2`");
    assert_eq!(terms.len(), 30);
    assert_eq!(terms[0], "word1");
    assert!(!terms.contains(&"Word1".to_string()));
}
