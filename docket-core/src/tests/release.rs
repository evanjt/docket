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
    assert!(check_name("v2.1").is_ok());
    for bad in ["", "1.0 beta", "current", "upkeep", "7"] {
        assert!(check_name(bad).is_err(), "{bad} was taken");
    }
}

#[test]
fn test_a_new_release_takes_the_place_its_version_gives_it() {
    let all = vec![release("1.0", 0, false), release("1.2", 1, false)];
    assert_eq!(place(&all, "1.1"), Ok(1));
    assert_eq!(place(&all, "2.0"), Ok(2));
    assert_eq!(place(&all, "0.9"), Ok(0));
    assert_eq!(
        place(&all, "1.2"),
        Err(Refused("1.2 is already a release".to_string()))
    );
}

#[test]
fn test_current_is_the_first_release_not_shipped_and_an_unknown_name_is_refused() {
    let all = vec![
        release("1.0", 0, true),
        release("1.2", 2, false),
        release("1.1", 1, false),
    ];
    assert_eq!(unshipped(&all), vec!["1.1", "1.2"]);
    assert_eq!(resolve(&all, "current"), Ok(Some("1.1".to_string())));
    assert_eq!(resolve(&all, "1.2"), Ok(Some("1.2".to_string())));
    assert_eq!(resolve(&all, ""), Ok(None));
    assert_eq!(
        resolve(&all, "0.42"),
        Err(Refused(
            "0.42 is not a release here: give current or one of 1.1 1.2".to_string()
        ))
    );
    assert!(
        resolve(&all, "1.0")
            .unwrap_err()
            .0
            .starts_with("1.0 has shipped")
    );
    assert!(resolve(&[], "current").is_err());
}

#[test]
fn test_a_release_ships_only_when_nothing_in_it_is_open() {
    assert_eq!(ship_refusal("1.0", &[]), None);
    assert_eq!(
        ship_refusal("1.0", &["T1".to_string()]).unwrap(),
        "1.0 still holds 1 open: T1. Close them, or pass --move-open-to a later release."
    );
}
