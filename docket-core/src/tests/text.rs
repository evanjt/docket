use super::*;

fn paths(body: &str) -> Vec<String> {
    citations(body).into_iter().map(|c| c.path).collect()
}

#[test]
fn test_citations_keep_the_whole_extension() {
    let body = "- **Evidence.** `SectionChangeCardSlide.tsx` and `a.jsx` and `b.rs`.\n";
    assert_eq!(
        paths(body),
        vec!["SectionChangeCardSlide.tsx", "a.jsx", "b.rs"]
    );
}

#[test]
fn test_citations_are_not_property_accesses() {
    let body = "- **Evidence.** `WidgetRenderer.recordUrl` and `engine.tsBucket` and `x.python`.\n";
    assert_eq!(citations(body), vec![]);
}

#[test]
fn test_citations_skip_fix_bullets() {
    let body =
        "- **Evidence.** `src/a.rs:12` and `tests/a_test.rs`.\n- **Fix.** create `src/new.rs`.\n";
    let cites = citations(body);
    assert_eq!(paths(body), vec!["src/a.rs", "tests/a_test.rs"]);
    assert_eq!(cites[0].line, Some(12));
    assert_eq!(cites[1].kind, "cites_test");
    assert_eq!(cites[0].kind, "cites_file");
}

#[test]
fn test_citations_read_a_trailing_fragment_and_dedupe() {
    let body = "`src/a.rs:4 and more` `src/a.rs:4` `src/a.rs` `foo.rs.bak` `a.rs:x`\n";
    let cites = citations(body);
    assert_eq!(
        cites
            .iter()
            .map(|c| (c.path.as_str(), c.line))
            .collect::<Vec<_>>(),
        vec![
            ("src/a.rs", Some(4)),
            ("src/a.rs", None),
            ("foo.rs", None),
            ("a.rs", None)
        ]
    );
}

#[test]
fn test_citations_resume_after_a_fix_bullet_ends() {
    let body = "- **Fix.** create `src/new.rs`.\n\n`src/x.rs`\n## Original\n`src/gone.rs`\n";
    assert_eq!(paths(body), vec!["src/x.rs"]);
}

const PLAN: &str = "- **Source.** A conversation.\n- **Principles.**\n  1. Every loaf is weighed against a ratio set,\n     written by hand.\n  2. A shared step is corrected once.\n- **Scope.** The pantry.\n";

#[test]
fn test_principles_parse_the_numbered_list_with_continuations() {
    assert_eq!(
        principles(PLAN),
        vec![
            (
                1,
                "Every loaf is weighed against a ratio set, written by hand.".to_string()
            ),
            (2, "A shared step is corrected once.".to_string()),
        ]
    );
    assert_eq!(principles("- **Fix.**\n  1. not a principle\n"), vec![]);
}

#[test]
fn test_a_quoted_original_is_not_read_for_citations() {
    let body = format!("{PLAN}\n## Original plan\n\nTouches `src/moved/away.rs:12`.\n");
    assert_eq!(
        citations(&format!("- **Source.** `docs/plan.md`\n{body}")),
        vec![Citation {
            path: "docs/plan.md".into(),
            line: None,
            kind: "cites_file"
        }]
    );
    assert_eq!(principles(&body)[1].1, "A shared step is corrected once.");
}

#[test]
fn test_mentioned_ids_are_the_backticked_ones_sorted() {
    assert_eq!(
        mentioned_ids("see `B2` and `PK1`, then `B2` again, not B3", &["B", "PK"]),
        vec!["B2", "PK1"]
    );
}

#[test]
fn test_gates_result_opens_with_one_of_three_words() {
    assert_eq!(gates_result(Some(" passed ")).unwrap(), "passed");
    assert_eq!(
        gates_result(Some("skipped: lock held")).unwrap(),
        "skipped: lock held"
    );
    assert_eq!(
        gates_result(Some("fine")).unwrap_err().0,
        "a gates result opens with passed, failed or skipped, as \"skipped: lock held\". Got \"fine\"."
    );
    assert!(gates_result(Some("passedx")).is_err());
    assert!(gates_result(None).is_err());
}

#[test]
fn test_split_id_accepts_any_case_and_three_capitals() {
    assert_eq!(split_id(" b14 ").unwrap(), ("B".to_string(), 14));
    assert_eq!(split_id("STY1").unwrap(), ("STY".to_string(), 1));
    assert_eq!(
        split_id("ABCD1").unwrap_err().0,
        "'ABCD1' is not an id: a key of one to three capitals and a number, like B14"
    );
    assert!(split_id("B").is_err());
    assert!(split_id("").is_err());
}

#[test]
fn test_quoted_picks_quote_and_escapes() {
    assert_eq!(quoted("x"), "'x'");
    assert_eq!(quoted("it's"), "\"it's\"");
    assert_eq!(quoted("a'b\"c"), "'a\\'b\"c'");
    assert_eq!(quoted("a\\b\n"), "'a\\\\b\\n'");
    assert_eq!(quoted("\u{1}"), "'\\x01'");
}

#[test]
fn test_principle_refs_skip_a_longer_id() {
    let text = "serves A1#2 and A1#10, not BA1#3 or A12#4; A1#2 again";
    assert_eq!(principle_refs(text, "A1"), vec![2, 10]);
    assert!(principle_refs("A1# none", "A1").is_empty());
}
