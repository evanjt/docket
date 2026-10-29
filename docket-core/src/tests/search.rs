use super::*;

#[test]
fn test_fts_query_requires_each_word_and_the_last_as_a_prefix() {
    assert_eq!(
        fts_query("  garden  lant "),
        Some("garden & lant:*".to_string())
    );
    assert_eq!(fts_query("lant"), Some("lant:*".to_string()));
    assert_eq!(fts_query("   "), None);
}

#[test]
fn test_fts_query_reads_separators_as_the_index_does() {
    assert_eq!(
        fts_query("src/a.rs:4 (x|y)"),
        Some("src <-> a.rs <-> 4 & x <-> y:*".to_string())
    );
    assert_eq!(fts_query("& ! ( ) -"), None);
}

#[test]
fn test_id_of_reads_one_id_in_any_case() {
    assert_eq!(id_of(" t14 "), Some("T14".to_string()));
    assert_eq!(id_of("STY2"), Some("STY2".to_string()));
    assert_eq!(id_of("t14 lamp"), None);
    assert_eq!(id_of("lamp"), None);
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
