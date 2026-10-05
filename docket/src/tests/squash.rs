use super::*;

#[test]
fn test_a_last_tree_that_differs_from_the_work_tips_is_refused() {
    assert!(tips_match("abc", "abc").is_ok());
    let why = tips_match("abc", "def").unwrap_err().to_string();
    assert!(why.contains("nothing written"), "{why}");
}

#[test]
fn test_submodule_paths_are_the_repos_under_the_checkout_less_its_path() {
    let repos: Vec<String> = [".", "vendor/glaze/", "./tools/wheel", "~/elsewhere", "/abs"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    assert_eq!(
        submodule_paths(&repos, "."),
        vec!["vendor/glaze", "tools/wheel"]
    );
    assert_eq!(submodule_paths(&repos, "vendor"), vec!["glaze"]);
    assert!(submodule_paths(&repos, "studio").is_empty());
}
