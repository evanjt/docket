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
        repo_dirs(&[], std::slice::from_ref(&root), &[]),
        vec![here.join(".")]
    );
    assert!(repo_dirs(&[], &[root], &["no-such-dir-here".into()]).is_empty());
}

fn root(path: &str, project: &str) -> docket_client::roots::Root {
    docket_client::roots::Root {
        path: path.into(),
        project: project.into(),
        how: "bind".into(),
        bound_at: String::new(),
    }
}

#[test]
fn test_resolve_repo_by_slug_uses_each_machines_roots() {
    let u1 = [root("/h/u1/lib", "acme/lib"), root("/h/u1/app", "acme/app")];
    let u2 = [root("/h/u2/lib", "acme/lib"), root("/h/u2/app", "acme/app")];
    assert_eq!(
        resolve_repo(&u1, "/h/u1/app", "@acme/lib").unwrap(),
        PathBuf::from("/h/u1/lib")
    );
    assert_eq!(
        resolve_repo(&u2, "/h/u2/app", "@acme/lib").unwrap(),
        PathBuf::from("/h/u2/lib")
    );
    assert_eq!(
        resolve_repo(&u1, "/h/u1/app", "@acme/lib/crates/x").unwrap(),
        PathBuf::from("/h/u1/lib/crates/x")
    );
}

#[test]
fn test_resolve_repo_plain_path_is_under_the_filing_root() {
    let u1 = [root("/h/u1/app", "acme/app")];
    assert_eq!(
        resolve_repo(&u1, "/h/u1/app", "web").unwrap(),
        PathBuf::from("/h/u1/app/web")
    );
}

#[test]
fn test_resolve_repo_unbound_slug_names_bind() {
    let u1 = [root("/h/u1/app", "acme/app")];
    let e = resolve_repo(&u1, "/h/u1/app", "@acme/lib").unwrap_err();
    assert!(e.contains("docket bind acme/lib"), "{e}");
}
