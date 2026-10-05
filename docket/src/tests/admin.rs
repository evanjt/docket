use docket_core::migrate::{Case, Changes, Decision, Risky};

use super::*;

fn waiting(project: &str, waits: Option<Decision>) -> Changes {
    Changes {
        project: project.to_string(),
        risky: vec![Risky {
            case: Case::FoldedTheme {
                theme: "pollen".to_string(),
                open: 2,
            },
            action: "folded".to_string(),
            waits,
        }],
        ..Changes::default()
    }
}

#[test]
fn test_the_flags_decide_the_rules_and_an_absent_flag_leaves_one_open() {
    assert_eq!(rules(None, None), Rules::default());
    assert_eq!(
        rules(Some("pull-children"), Some("backlog")),
        Rules {
            held: Some(Held::PullChildren),
            areas: Some(Areas::Backlog),
        }
    );
}

#[test]
fn test_a_write_is_refused_naming_each_project_whose_cases_wait() {
    let text = refusal(&[
        waiting("orchard", Some(Decision::Areas)),
        waiting("cellar", None),
    ]);
    assert!(text.contains("orchard"), "{text}");
    assert!(text.contains("pollen"), "{text}");
    assert!(!text.contains("cellar"), "{text}");
    assert!(refusal(&[waiting("cellar", None)]).contains("--dry-run"));
}
