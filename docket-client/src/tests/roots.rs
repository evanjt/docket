use super::*;

fn root(path: &str, project: &str) -> Root {
    Root {
        path: path.into(),
        project: project.into(),
        how: "bind".into(),
        bound_at: "t".into(),
    }
}

#[test]
fn test_bound_takes_the_longest_root_holding_the_path() {
    let roots = [root("/w", "a/outer"), root("/w/inner/", "a/inner")];
    assert_eq!(bound(&roots, "/w/inner/src").as_deref(), Some("a/inner"));
    assert_eq!(bound(&roots, "/w/inner").as_deref(), Some("a/inner"));
    assert_eq!(bound(&roots, "/w/innerx").as_deref(), Some("a/outer"));
    assert_eq!(bound(&roots, "/elsewhere"), None);
}

#[test]
fn test_parse_reads_four_fields_and_skips_the_rest() {
    let roots = parse("/w\ta/b\tauto\t2026-01-01T00:00:00Z\nbroken line\n/x\t\tbind\tt\n");
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].project, "a/b");
    assert_eq!(render(&roots), "/w\ta/b\tauto\t2026-01-01T00:00:00Z\n");
}

#[test]
fn test_bind_replaces_a_path_and_writes_the_file() {
    let dir = std::env::temp_dir().join(format!("docket-roots-{}", std::process::id()));
    let file = dir.join("roots");
    let mut roots = Roots::load(file.clone());
    assert!(roots.roots.is_empty());
    roots.bind(root("/w", "a/one")).unwrap();
    roots.bind(root("/w", "a/two")).unwrap();
    let again = Roots::load(file);
    assert_eq!(again.roots.len(), 1);
    assert_eq!(again.of("a/two"), ["/w"]);
    std::fs::remove_dir_all(dir).unwrap();
}
