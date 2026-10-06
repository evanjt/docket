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
fn test_the_flags_decide_the_rules_and_an_absent_flag_takes_the_plan_rule() {
    assert_eq!(rules(None), Rules::default());
    assert_eq!(rules(Some("plan")), Rules::default());
    assert_eq!(
        rules(Some("backlog")),
        Rules {
            themes: Themes::Backlog,
        }
    );
}

#[test]
fn test_a_write_is_refused_naming_each_project_whose_cases_wait() {
    let text = refusal(&[
        waiting("orchard", Some(Decision::Themes)),
        waiting("cellar", None),
    ]);
    assert!(text.contains("orchard"), "{text}");
    assert!(text.contains("pollen"), "{text}");
    assert!(!text.contains("cellar"), "{text}");
    assert!(refusal(&[waiting("cellar", None)]).contains("--dry-run"));
}

#[test]
fn test_only_a_commit_answer_resolves_a_new_sha() {
    let answer = "aaaaaaaaa commit 231\nbbbbbbbbb missing\nccccccccc blob 4\nddddddddd tree 9\n";
    assert_eq!(
        not_commits(answer),
        vec!["bbbbbbbbb", "ccccccccc", "ddddddddd"]
    );
}
