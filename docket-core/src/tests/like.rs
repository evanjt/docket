use super::*;

#[test]
fn test_like_wildcards_and_case() {
    assert!(like("%road%", "Roadmap"));
    assert!(like("a_c", "ABC"));
    assert!(like("%", ""));
    assert!(like("", ""));
    assert!(!like("a_c", "ac"));
    assert!(!like("abc", "abcd"));
    assert!(like("%b%d", "abcd"));
    assert!(!like("%b%e", "abcd"));
}

#[test]
fn test_contains_reads_as_a_like_pattern() {
    assert!(contains("sync-v2", "SYNC"));
    assert!(contains("sync", ""));
    assert!(!contains("sync", "syncs"));
    assert!(contains("a-b", "a_b"));
}
