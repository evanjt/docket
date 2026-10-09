use std::collections::{BTreeMap, HashSet};

use super::*;

fn label(name: &str, about: Option<&str>) -> Label {
    Label {
        name: name.to_string(),
        description: about.map(str::to_string),
    }
}

#[test]
fn test_a_label_name_is_trimmed_and_never_empty() {
    assert_eq!(
        check_name("  goal:lanterns "),
        Ok("goal:lanterns".to_string())
    );
    assert!(check_name("   ").is_err());
}

#[test]
fn test_a_label_is_found_ignoring_case() {
    let all = vec![label("Goal:Kites", None)];
    assert!(find(&all, "goal:kites").is_some());
    assert!(find(&all, "goal:boats").is_none());
}

#[test]
fn test_a_label_line_carries_its_description_when_it_has_one() {
    assert_eq!(
        line(&label("goal:kites", Some("fly in wind"))),
        "goal:kites: fly in wind"
    );
    assert_eq!(line(&label("goal:kites", Some("  "))), "goal:kites");
    assert_eq!(line(&label("goal:kites", None)), "goal:kites");
}

#[test]
fn test_a_group_is_the_label_named_for_it() {
    assert_eq!(of_group(" sweep "), "group:sweep");
    assert_eq!(group_of(&["ci", "group:sweep"]), Some("sweep"));
    assert_eq!(group_of(&["ci", "group:"]), None);
    assert_eq!(group_of::<&str>(&[]), None);
}

#[test]
fn test_a_repository_is_a_path_under_the_root_kept_as_its_label() {
    assert_eq!(of_repo(" ./kites/ "), Ok(Some("repo:kites".to_string())));
    assert_eq!(
        of_repo("toys/kites"),
        Ok(Some("repo:toys/kites".to_string()))
    );
    assert_eq!(of_repo("."), Ok(Some("repo:.".to_string())));
    assert_eq!(of_repo("  "), Ok(None));
    assert!(of_repo("/srv/kites").is_err());
    assert!(of_repo("~/kites").is_err());
    assert!(of_repo("../kites").is_err());
    assert!(of_repo("toys/../kites").is_err());
}

#[test]
fn test_a_set_of_labels_names_every_repository_once_in_order() {
    assert_eq!(
        repos_of(&["ci", "repo:kites", "repo:lanterns", "repo:kites"]),
        ["kites", "lanterns"]
    );
    assert_eq!(repos_of(&["ci", "repo:"]).len(), 0);
    assert_eq!(repos_of::<&str>(&[]).len(), 0);
    assert!(is_repo("repo:kites"));
    assert!(!is_repo("group:kites"));
}

#[test]
fn test_an_item_is_in_the_repository_its_nearest_label_names_else_the_default() {
    let nearest = BTreeMap::from([
        (1, vec!["kites".to_string()]),
        (2, vec!["lanterns".to_string()]),
        (4, vec!["kites".to_string(), "lanterns".to_string()]),
    ]);
    let all = [1, 2, 3, 4];
    let sorted = |s: HashSet<i64>| {
        let mut v: Vec<i64> = s.into_iter().collect();
        v.sort_unstable();
        v
    };
    assert_eq!(sorted(in_repo(&all, &nearest, "lanterns/", ".")), [2, 4]);
    assert_eq!(
        sorted(in_repo(&all, &nearest, "./kites", "kites")),
        [1, 3, 4]
    );
    assert_eq!(sorted(in_repo(&all, &nearest, ".", ".")), [3]);
}

#[test]
fn test_repo_path_names_another_project_by_slug() {
    assert_eq!(
        repo_path("@acme/lib").unwrap().as_deref(),
        Some("@acme/lib")
    );
    assert_eq!(
        repo_path("@acme/lib/./crates/x/").unwrap().as_deref(),
        Some("@acme/lib/crates/x")
    );
    assert!(repo_path("@acme/lib/../x").is_err());
    assert!(repo_path("@").is_err());
    assert_eq!(project_named("@acme/lib/crates"), Some("acme/lib"));
    assert_eq!(project_named("web"), None);
}
