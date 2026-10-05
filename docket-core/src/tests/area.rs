use super::*;

fn area(name: &str, position: i64) -> Area {
    Area {
        name: name.to_string(),
        position,
        ..Area::default()
    }
}

#[test]
fn test_two_names_that_differ_only_in_case_are_one_area() {
    let all = vec![area("lanterns", 0)];
    assert_eq!(
        check_name(&all, "Lanterns", None),
        Err(Refused("Lanterns is already an area: lanterns".to_string()))
    );
    assert_eq!(check_name(&all, "kites", None), Ok(()));
    assert_eq!(check_name(&all, "Lanterns", Some("lanterns")), Ok(()));
}

#[test]
fn test_an_area_name_is_never_empty() {
    assert!(check_name(&[], "", None).is_err());
    assert!(check_name(&[], "   ", None).is_err());
}

#[test]
fn test_a_write_naming_an_area_the_project_lacks_is_refused_with_its_areas() {
    let all = vec![area("lanterns", 0), area("paper boats", 1)];
    assert_eq!(
        resolve(&all, "Kites"),
        Err(Refused(
            "Kites is not an area here: give one of lanterns, paper boats".to_string()
        ))
    );
    assert_eq!(
        resolve(&[], "Kites"),
        Err(Refused(
            "Kites is not an area here: there are none, add one with docket areas add".to_string()
        ))
    );
    assert_eq!(resolve(&all, "LANTERNS"), Ok(Some("lanterns".to_string())));
    assert_eq!(resolve(&all, ""), Ok(None));
}

#[test]
fn test_an_area_priority_is_one_of_the_four_words_or_none() {
    assert_eq!(check_priority(""), Ok(None));
    assert_eq!(check_priority("high"), Ok(Some("high".to_string())));
    assert!(check_priority("urgent").is_err());
}

#[test]
fn test_moving_an_area_shifts_the_ones_between() {
    let all = vec![area("a", 0), area("b", 1), area("c", 2)];
    assert_eq!(
        reorder(&all, "c", 1),
        Ok(vec!["c".to_string(), "a".to_string(), "b".to_string()])
    );
    assert_eq!(
        reorder(&all, "a", 9),
        Ok(vec!["b".to_string(), "c".to_string(), "a".to_string()])
    );
    assert!(reorder(&all, "a", 0).is_err());
    assert!(reorder(&all, "z", 1).is_err());
}

#[test]
fn test_an_area_is_removed_only_while_no_item_carries_it() {
    assert_eq!(rm_refusal("kites", &[]), None);
    assert_eq!(
        rm_refusal("kites", &["T1".to_string(), "A2".to_string()]).unwrap(),
        "kites is carried by 2 items: T1, A2. Move them to another area first."
    );
}

#[test]
fn test_a_placement_reads_its_area_and_about_from_the_event_data() {
    let data =
        r#"{"derived": "its title names kites", "area": "kites", "about": "string and tails"}"#;
    assert_eq!(
        Placement::of(data),
        Some(Placement {
            area: "kites".to_string(),
            about: Some("string and tails".to_string()),
        })
    );
    let bare = r#"{"derived": "b", "area": "kites"}"#;
    assert_eq!(Placement::of(bare).map(|p| p.about), Some(None));
}

#[test]
fn test_a_decided_event_with_only_a_basis_is_no_placement() {
    assert_eq!(Placement::of(r#"{"derived": "CID1"}"#), None);
    assert_eq!(Placement::of("not json"), None);
}

#[test]
fn test_a_placement_name_is_lower_case_with_hyphens_and_never_unsorted() {
    assert_eq!(
        placement_name(" Paper  Boats "),
        Ok("paper-boats".to_string())
    );
    assert!(placement_name("  ").is_err());
    assert!(placement_name("Unsorted").is_err());
}

#[test]
fn test_open_into_refuses_a_history_area_and_lists_the_others() {
    let all = vec![
        area("kites", 0),
        Area {
            history: true,
            ..area("attic", 1)
        },
    ];
    assert_eq!(open_into(&all, "kites"), Ok(()));
    let Err(Refused(why)) = open_into(&all, "Attic") else {
        panic!("a history area takes no open item");
    };
    assert!(
        why.contains("attic") && why.contains("closed items only"),
        "{why}"
    );
    assert!(
        why.contains("kites") && !why.contains("kites, attic"),
        "{why}"
    );
}
