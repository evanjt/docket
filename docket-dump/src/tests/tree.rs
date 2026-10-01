use std::os::unix::fs::symlink;

use super::*;

#[test]
fn test_write_creates_directories_and_reports_change() {
    let dir = tempfile::tempdir().unwrap();
    assert!(write(dir.path(), "o/p/items/T1.md", "one\n").unwrap());
    assert!(!write(dir.path(), "o/p/items/T1.md", "one\n").unwrap());
    assert!(write(dir.path(), "o/p/items/T1.md", "two\n").unwrap());
    assert_eq!(
        read(dir.path(), "o/p/items/T1.md").as_deref(),
        Some("two\n")
    );
    let left: Vec<_> = fs::read_dir(dir.path().join("o/p/items"))
        .unwrap()
        .collect();
    assert_eq!(left.len(), 1);
}

#[test]
fn test_write_refuses_a_symlink() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("target"), "x").unwrap();
    symlink(dir.path().join("target"), dir.path().join("link.md")).unwrap();
    assert!(write(dir.path(), "link.md", "y").is_err());
    assert_eq!(read(dir.path(), "target").as_deref(), Some("x"));
}

#[test]
fn test_projects_finds_project_files_and_skips_code_and_hidden_trees() {
    let dir = tempfile::tempdir().unwrap();
    for rel in [
        "org/a/project.json",
        "org/a/nested/project.json",
        "solo/project.json",
        "src/x/project.json",
        "template/project.json",
        ".git/y/project.json",
    ] {
        write(dir.path(), rel, "{}\n").unwrap();
    }
    write(dir.path(), "README.md", "x").unwrap();
    assert_eq!(projects(dir.path()).unwrap(), vec!["org/a", "solo"]);
}

#[test]
fn test_items_lists_ids_sorted_as_text() {
    let dir = tempfile::tempdir().unwrap();
    for rel in ["o/p/items/T10.md", "o/p/items/T9.md", "o/p/items/notes.txt"] {
        write(dir.path(), rel, "x").unwrap();
    }
    assert_eq!(items(dir.path(), "o/p").unwrap(), vec!["T10", "T9"]);
    assert!(items(dir.path(), "o/none").unwrap().is_empty());
}
