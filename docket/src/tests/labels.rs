use docket_core::label::Label;

use super::*;

#[test]
fn test_a_label_line_names_the_label_its_item_count_and_its_description() {
    let row = |about: Option<&str>| LabelRow {
        label: Label {
            name: "goal:kites".to_string(),
            description: about.map(str::to_string),
        },
        items: 3,
    };
    assert_eq!(
        line(&row(Some("fly in wind"))),
        "goal:kites  (3)  fly in wind"
    );
    assert_eq!(line(&row(None)), "goal:kites  (3)");
}
