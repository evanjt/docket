use super::*;

#[test]
fn test_id_reads_any_case_and_leading_zeros() {
    assert_eq!(id(" b07 ").unwrap(), "B7");
    assert_eq!(id("STY12").unwrap(), "STY12");
}

#[test]
fn test_id_refuses_what_is_not_one_in_the_cli_words() {
    assert_eq!(
        id("zz").unwrap_err(),
        Fail::refused("'zz' is not an id: a key of one to three capitals and a number, like B14")
    );
}

#[test]
fn test_a_worktree_outside_its_root_takes_its_checkouts_binding() {
    let roots = vec![docket_client::roots::Root {
        path: "/work/app".into(),
        project: "team/app".into(),
        how: "bind".into(),
        bound_at: "2026-01-01T00:00:00Z".into(),
    }];
    assert_eq!(
        bound_at(&roots, "/work/app-t14", Some("/work/app")),
        Some("team/app".into())
    );
    assert_eq!(
        bound_at(&roots, "/work/app/src", Some("/work/app")),
        Some("team/app".into())
    );
    assert_eq!(bound_at(&roots, "/work/other", Some("/work/other")), None);
    assert_eq!(bound_at(&roots, "/work/other", None), None);
}

#[test]
fn test_root_at_is_the_project_root_holding_the_directory() {
    let root = |path: &str, project: &str| Root {
        path: path.into(),
        project: project.into(),
        how: "bind".into(),
        bound_at: "2026-01-01T00:00:00Z".into(),
    };
    let roots = vec![root("/w/a", "team/app"), root("/work/bb", "team/app")];
    assert_eq!(
        root_at(&roots, "team/app", "/work/bb/src", Some("/work/bb")),
        Some("/work/bb".into())
    );
    assert_eq!(
        root_at(&roots, "team/app", "/w/a", Some("/w/a")),
        Some("/w/a".into())
    );
    assert_eq!(root_at(&roots, "team/app", "/elsewhere", None), None);
}
