use super::*;

#[test]
fn test_expand_relative_absolute_and_home() {
    assert_eq!(expand("/r", "sub"), PathBuf::from("/r/sub"));
    assert_eq!(expand("/r", "/abs"), PathBuf::from("/abs"));
    assert!(!expand("/r", "~/x").starts_with("/r"));
}

#[test]
fn test_repo_dirs_defaults_to_the_root_and_skips_missing() {
    let here = std::env::temp_dir();
    let root = here.to_string_lossy().into_owned();
    assert_eq!(
        repo_dirs(std::slice::from_ref(&root), &[]),
        vec![here.join(".")]
    );
    assert!(repo_dirs(&[root], &["no-such-dir-here".into()]).is_empty());
}
