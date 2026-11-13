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
fn test_a_squash_is_due_for_an_uncovered_finished_plan_or_an_old_landing() {
    const HOUR: i64 = 3600;
    let now = 1_000_000;
    let plans = vec!["A3".to_string()];
    let old = squash_due(&[], Some(now - 25 * HOUR), now).unwrap();
    assert!(old.contains("25 hours"), "{old}");
    assert!(squash_due(&[], Some(now - HOUR / 2), now).is_none());
    assert!(squash_due(&[], Some(now - 24 * HOUR + 1), now).is_none());
    assert!(squash_due(&[], Some(now - 24 * HOUR), now).is_some());
    let closed = squash_due(&plans, Some(now - 60), now).unwrap();
    assert!(closed.starts_with("A3 closed"));
    assert!(squash_due(&plans, None, now).is_none());
    assert!(squash_due(&[], None, now).is_none());
}

#[test]
fn test_the_push_ask_names_the_command() {
    assert_eq!(
        push_command("shelf", "upstream/trunk"),
        "git push upstream shelf:trunk"
    );
    assert_eq!(
        push_title("shelf", "upstream/trunk"),
        "Push shelf to upstream/trunk"
    );
}

const NAMES: (&str, &str) = ("shelf", "upstream/trunk");

#[test]
fn test_a_local_ref_equal_to_or_inside_the_remote_with_no_publication_starts_on_the_remote() {
    assert_eq!(
        start_on(Some("a"), Some("a"), true, None, NAMES).unwrap(),
        StartOn::Remote
    );
    assert_eq!(
        start_on(Some("a"), Some("b"), true, None, NAMES).unwrap(),
        StartOn::Remote
    );
}

#[test]
fn test_a_local_ref_with_an_unrecorded_commit_past_the_remote_is_refused() {
    let why = start_on(Some("c"), Some("b"), false, None, NAMES)
        .unwrap_err()
        .0;
    assert!(why.contains("no recorded publication names"), "{why}");
    assert!(start_on(Some("c"), None, false, None, NAMES).is_err());
}

#[test]
fn test_a_local_ref_named_by_the_last_publication_starts_on_it() {
    let last = made("c", "w9");
    assert_eq!(
        start_on(Some("c"), Some("b"), false, Some(&last), NAMES).unwrap(),
        StartOn::Local { work: "w9".into() }
    );
}

#[test]
fn test_with_no_local_ref_the_start_is_the_remote_or_a_refusal() {
    assert_eq!(
        start_on(None, Some("b"), false, None, NAMES).unwrap(),
        StartOn::Remote
    );
    assert!(start_on(None, None, false, None, NAMES).is_err());
}
