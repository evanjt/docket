use super::*;

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn test_a_job_naming_only_another_projects_repository_also_places_the_checkout() {
    assert_eq!(
        repos_to_place(names(&["@acme/lib"]), "."),
        names(&[".", "@acme/lib"])
    );
    assert_eq!(
        repos_to_place(names(&["@acme/lib"]), "app"),
        names(&["app", "@acme/lib"])
    );
}

#[test]
fn test_a_job_naming_the_checkout_or_a_repository_of_its_own_project_places_what_it_names() {
    assert_eq!(repos_to_place(names(&["."]), "."), names(&["."]));
    assert_eq!(
        repos_to_place(names(&["toys/kites"]), "."),
        names(&["toys/kites"])
    );
    assert_eq!(
        repos_to_place(names(&["@acme/lib", "."]), "."),
        names(&["@acme/lib", "."])
    );
    assert_eq!(repos_to_place(Vec::new(), "."), Vec::<String>::new());
}
