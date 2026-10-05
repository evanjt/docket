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
