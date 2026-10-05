use super::*;

#[test]
fn test_check_sha_takes_only_a_full_lowercase_sha() {
    assert!(check_sha("published", &"0a".repeat(20)).is_ok());
    assert!(check_sha("published", &"f".repeat(64)).is_ok());
    for bad in [
        "",
        "abc1234",
        &"A".repeat(40),
        &"g".repeat(40),
        &"a".repeat(41),
    ] {
        let why = check_sha("work", bad).unwrap_err().0;
        assert!(why.starts_with("the work commit is a full sha"), "{why}");
    }
}

fn made(published: &str, work: &str) -> Publication {
    Publication {
        published: published.into(),
        work: work.into(),
        ..Publication::default()
    }
}

#[test]
fn test_an_item_is_named_under_the_first_publication_whose_snapshot_contains_it() {
    let newest_first = [made("p3", "w3"), made("p2", "w2"), made("p1", "w1")];
    let in_second = |work: &str| matches!(work, "w2" | "w3");
    assert_eq!(
        first_containing(&newest_first, in_second)
            .unwrap()
            .published,
        "p2"
    );
    assert!(first_containing(&newest_first, |_| false).is_none());
}

#[test]
fn test_a_squash_is_due_for_an_uncovered_finished_plan_or_many_landings() {
    let plans = vec!["A3".to_string()];
    assert!(squash_due(&plans, 1, 30).unwrap().starts_with("A3 closed"));
    assert!(squash_due(&[], 29, 30).is_none());
    assert!(squash_due(&[], 30, 30).unwrap().starts_with("30 landings"));
    assert!(squash_due(&plans, 0, 30).is_none());
}

#[test]
fn test_the_push_ask_names_the_command() {
    assert_eq!(
        push_command("published", "origin/main"),
        "git push origin published:main"
    );
    assert_eq!(
        push_title("published", "origin/main"),
        "Push published to origin/main"
    );
}
