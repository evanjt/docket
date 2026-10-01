use super::*;

#[test]
fn test_declared_files_strips_lines_and_folds_short_paths() {
    let body = "- **Touches.** `src/a.rs:12`, a.rs, src/b/c.ts:3-9, none, two words.rs\nother";
    let got: Vec<String> = declared_files(body).into_iter().collect();
    assert_eq!(got, vec!["src/a.rs", "src/b/c.ts"]);
}

#[test]
fn test_declared_files_empty_without_a_touches_line() {
    assert!(declared_files("- **Fix.** `src/a.rs`").is_empty());
}

#[test]
fn test_same_file_needs_a_directory_on_the_shorter() {
    assert!(same_file("a/b.rs", "x/a/b.rs"));
    assert!(!same_file("b.rs", "x/b.rs"));
    assert!(same_file("b.rs", "b.rs"));
}
