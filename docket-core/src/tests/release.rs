use super::*;

fn release(name: &str, position: i64, shipped: bool) -> Release {
    Release {
        name: name.to_string(),
        position,
        shipped_at: shipped.then(|| "2026-01-01T00:00:00Z".to_string()),
        ..Release::default()
    }
}

#[test]
fn test_a_release_name_is_one_version_word_and_never_the_alias() {
    assert!(check_name("1.0.0").is_ok());
    assert!(check_name("2.0.0-rc.1").is_ok());
    for bad in [
        "", "1.0 beta", "current", "upkeep", "7", "1.0", "v1.0.0", "01.0.0", "alpha", "1.1",
    ] {
        assert!(check_name(bad).is_err(), "{bad} was taken");
    }
}

#[test]
fn test_a_name_that_is_no_semantic_version_is_refused_with_an_example() {
    assert_eq!(
        check_name("1.1"),
        Err(Refused(
            "1.1 is not a semantic version: give MAJOR.MINOR.PATCH, as 1.1.0".to_string()
        ))
    );
}

#[test]
fn test_two_names_of_equal_precedence_are_one_release() {
    let all = vec![release("1.0.0+a", 0, false)];
    assert_eq!(
        place(&all, "1.0.0+b"),
        Err(Refused("1.0.0+b is already a release".to_string()))
    );
}

#[test]
fn test_a_prerelease_sorts_before_its_release() {
    let all = vec![release("1.0.0", 0, false)];
    assert_eq!(place(&all, "1.0.0-rc.1"), Ok(0));
}

#[test]
fn test_a_new_release_takes_the_place_its_version_gives_it() {
    let all = vec![release("1.0.0", 0, false), release("1.2.0", 1, false)];
    assert_eq!(place(&all, "1.1.0"), Ok(1));
    assert_eq!(place(&all, "2.0.0"), Ok(2));
    assert_eq!(place(&all, "0.9.0"), Ok(0));
    assert_eq!(
        place(&all, "1.2.0"),
        Err(Refused("1.2.0 is already a release".to_string()))
    );
}

#[test]
fn test_current_is_the_first_release_not_shipped_and_an_unknown_name_is_refused() {
    let all = vec![
        release("1.0.0", 0, true),
        release("1.2.0", 2, false),
        release("1.1.0", 1, false),
    ];
    assert_eq!(unshipped(&all), vec!["1.1.0", "1.2.0"]);
    assert_eq!(resolve(&all, "current"), Ok(Some("1.1.0".to_string())));
    assert_eq!(resolve(&all, "1.2.0"), Ok(Some("1.2.0".to_string())));
    assert_eq!(resolve(&all, ""), Ok(None));
    assert_eq!(
        resolve(&all, "0.42.0"),
        Err(Refused(
            "0.42.0 is not a release here: give current or one of 1.1.0 1.2.0".to_string()
        ))
    );
    assert!(
        resolve(&all, "1.0.0")
            .unwrap_err()
            .0
            .starts_with("1.0.0 has shipped")
    );
    assert!(resolve(&[], "current").is_err());
}

#[test]
fn test_a_release_ships_only_when_nothing_in_it_is_open() {
    assert_eq!(ship_refusal("1.0.0", &[]), None);
    assert_eq!(
        ship_refusal("1.0.0", &["T1".to_string()]).unwrap(),
        "1.0.0 still holds 1 open: T1. Close them, or pass --move-open-to a later release."
    );
}
