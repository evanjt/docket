use std::fs;

use tempfile::TempDir;

use super::*;

/// A checkout with one commit and an identity, and a bare origin it tracks.
fn checkout() -> (TempDir, TempDir) {
    let origin = tempfile::tempdir().unwrap();
    git(origin.path(), &["init", "-q", "--bare", "-b", "main"]).unwrap();
    let work = tempfile::tempdir().unwrap();
    let url = origin.path().to_str().unwrap();
    git(work.path(), &["clone", "-q", url, "."]).unwrap();
    identify(work.path());
    fs::write(work.path().join("README.md"), "dump\n").unwrap();
    git(work.path(), &["add", "README.md"]).unwrap();
    git(
        work.path(),
        &["commit", "-q", "--no-gpg-sign", "-m", "Start"],
    )
    .unwrap();
    git(work.path(), &["push", "-q", "-u", "origin", "main"]).unwrap();
    (work, origin)
}

fn identify(repo: &Path) {
    git(repo, &["config", "user.name", "Dump"]).unwrap();
    git(repo, &["config", "user.email", "dump@example.com"]).unwrap();
    git(repo, &["config", "commit.gpgsign", "false"]).unwrap();
    git(repo, &["config", "tag.gpgsign", "false"]).unwrap();
    git(repo, &["checkout", "-q", "-B", "main"]).unwrap();
}

fn put(repo: &Path, rel: &str, text: &str) {
    let path = repo.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn subjects(repo: &Path, range: &str) -> String {
    git(repo, &["log", "--format=%s", range]).unwrap()
}

#[test]
fn test_cursor_is_kept_in_local_config_and_forgotten() {
    let (work, _origin) = checkout();
    assert_eq!(cursor(work.path()).unwrap(), None);
    set_cursor(work.path(), 42).unwrap();
    assert_eq!(cursor(work.path()).unwrap(), Some(42));
    assert_eq!(git(work.path(), &["status", "--porcelain"]).unwrap(), "");
    forget_cursor(work.path()).unwrap();
    forget_cursor(work.path()).unwrap();
    assert_eq!(cursor(work.path()).unwrap(), None);
    git(work.path(), &["config", "--local", CURSOR, "many"]).unwrap();
    assert!(cursor(work.path()).is_err());
}

#[test]
fn test_commit_takes_only_the_paths_and_nothing_when_unchanged() {
    let (work, _origin) = checkout();
    put(work.path(), "o/p/items/T1.md", "one\n");
    put(work.path(), "notes.txt", "not the dump's\n");
    git(work.path(), &["add", "notes.txt"]).unwrap();
    let paths = vec!["o/p/items".to_string()];
    assert!(commit(work.path(), &paths, "Open T1").unwrap().is_some());
    assert_eq!(subjects(work.path(), "-1"), "Open T1");
    let shown = git(work.path(), &["show", "--name-only", "--format=", "HEAD"]).unwrap();
    assert_eq!(shown, "o/p/items/T1.md");
    assert_eq!(commit(work.path(), &paths, "Again").unwrap(), None);
    assert_eq!(commit(work.path(), &[], "Nothing").unwrap(), None);
}

#[test]
fn test_push_sends_commits_and_rebases_over_a_moved_origin() {
    let (work, origin) = checkout();
    assert_eq!(push(work.path()).unwrap(), Pushed::UpToDate);
    let other = tempfile::tempdir().unwrap();
    git(
        other.path(),
        &["clone", "-q", origin.path().to_str().unwrap(), "."],
    )
    .unwrap();
    identify(other.path());
    put(other.path(), "src/code.rs", "fn main() {}\n");
    git(other.path(), &["add", "src/code.rs"]).unwrap();
    git(
        other.path(),
        &["commit", "-q", "--no-gpg-sign", "-m", "Add code"],
    )
    .unwrap();
    git(other.path(), &["push", "-q", "origin", "main"]).unwrap();
    put(work.path(), "o/p/items/T1.md", "one\n");
    commit(work.path(), &["o/p/items".to_string()], "Open T1").unwrap();
    assert_eq!(push(work.path()).unwrap(), Pushed::Commits(1));
    assert_eq!(subjects(origin.path(), "-3"), "Open T1\nAdd code\nStart");
}

#[test]
fn test_push_that_fails_keeps_the_commits_for_the_next() {
    let (work, origin) = checkout();
    put(work.path(), "o/p/items/T1.md", "one\n");
    commit(work.path(), &["o/p/items".to_string()], "Open T1").unwrap();
    let gone = origin.path().with_extension("moved");
    fs::rename(origin.path(), &gone).unwrap();
    assert!(push(work.path()).is_err());
    assert_eq!(subjects(work.path(), "-1"), "Open T1");
    fs::rename(&gone, origin.path()).unwrap();
    assert_eq!(push(work.path()).unwrap(), Pushed::Commits(1));
    assert_eq!(subjects(origin.path(), "-1"), "Open T1");
}

#[test]
fn test_push_without_origin_commits_only() {
    let work = tempfile::tempdir().unwrap();
    git(work.path(), &["init", "-q"]).unwrap();
    assert_eq!(push(work.path()).unwrap(), Pushed::NoOrigin);
}
