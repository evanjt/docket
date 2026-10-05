use super::*;

use crate::pace::epoch;

const SINCE: &str = "2026-10-02T10:00:00Z";

fn at(stamp: &str) -> i64 {
    epoch(stamp).unwrap()
}

fn held(host: &str, session: &str, renewed: &str) -> Lead {
    Lead {
        project: "o/p".into(),
        host: host.into(),
        session: session.into(),
        branch: Some("main".into()),
        since: SINCE.into(),
        renewed_at: renewed.into(),
    }
}

fn me() -> Holder {
    Holder {
        host: "alpha".into(),
        session: "lead-1".into(),
    }
}

fn refusal(r: Result<Outcome, Refused>) -> String {
    r.unwrap_err().0
}

#[test]
fn test_take_with_no_lead_takes_it() {
    assert_eq!(
        decide("o/p", None, &me(), Act::Take, at(SINCE), 10),
        Ok(Outcome::Took)
    );
}

#[test]
fn test_take_by_its_holder_renews_it() {
    let lead = held("alpha", "lead-1", SINCE);
    assert_eq!(
        decide("o/p", Some(&lead), &me(), Act::Take, at(SINCE) + 60, 10),
        Ok(Outcome::Renewed)
    );
}

#[test]
fn test_take_while_another_holds_it_is_refused_naming_the_holder() {
    let lead = held("beta", "lead-7", "2026-10-02T10:05:00Z");
    assert_eq!(
        refusal(decide(
            "o/p",
            Some(&lead),
            &me(),
            Act::Take,
            at("2026-10-02T10:10:00Z"),
            10
        )),
        "o/p is led by lead-7 on beta since 2026-10-02T10:00:00Z, last renewed 2026-10-02T10:05:00Z; \
         it lapses at 2026-10-02T10:15:00Z unless renewed"
    );
}

#[test]
fn test_the_same_session_name_on_another_machine_is_another_holder() {
    let lead = held("beta", "lead-1", SINCE);
    assert!(decide("o/p", Some(&lead), &me(), Act::Take, at(SINCE), 10).is_err());
}

#[test]
fn test_take_once_the_holder_lapsed_takes_it_over() {
    let lead = held("beta", "lead-7", SINCE);
    assert_eq!(
        decide(
            "o/p",
            Some(&lead),
            &me(),
            Act::Take,
            at("2026-10-02T10:10:00Z"),
            10
        ),
        Ok(Outcome::TookOver(lead))
    );
}

#[test]
fn test_lapsed_counts_from_the_last_renewal() {
    let lead = held("beta", "lead-7", "2026-10-02T10:30:00Z");
    assert!(!lapsed(&lead, at("2026-10-02T10:39:59Z"), 10));
    assert!(lapsed(&lead, at("2026-10-02T10:40:00Z"), 10));
    assert_eq!(lapses_at(&lead, 10), Some("2026-10-02T10:40:00Z".into()));
    let unreadable = held("beta", "lead-7", "yesterday");
    assert!(lapsed(&unreadable, at(SINCE), 10));
}

#[test]
fn test_renew_by_anyone_but_the_holder_is_refused() {
    let lead = held("beta", "lead-7", SINCE);
    assert_eq!(
        refusal(decide("o/p", Some(&lead), &me(), Act::Renew, at(SINCE), 10)),
        "o/p is led by lead-7 on beta, not by lead-1 on alpha: only its holder renews it"
    );
    assert_eq!(
        refusal(decide("o/p", None, &me(), Act::Renew, at(SINCE), 10)),
        "no lead holds o/p: docket lead take"
    );
}

#[test]
fn test_renew_by_its_holder_after_the_lapse_keeps_it_while_nobody_took_it() {
    let lead = held("alpha", "lead-1", SINCE);
    assert_eq!(
        decide(
            "o/p",
            Some(&lead),
            &me(),
            Act::Renew,
            at("2026-10-02T12:00:00Z"),
            10
        ),
        Ok(Outcome::Renewed)
    );
}

#[test]
fn test_give_by_its_holder_gives_it_back_and_by_another_is_refused() {
    let mine = held("alpha", "lead-1", SINCE);
    assert_eq!(
        decide("o/p", Some(&mine), &me(), Act::Give, at(SINCE), 10),
        Ok(Outcome::Gave)
    );
    let theirs = held("beta", "lead-7", SINCE);
    assert_eq!(
        refusal(decide(
            "o/p",
            Some(&theirs),
            &me(),
            Act::Give,
            at(SINCE),
            10
        )),
        "o/p is led by lead-7 on beta, not by lead-1 on alpha: only its holder gives it back"
    );
    assert_eq!(
        refusal(decide("o/p", None, &me(), Act::Give, at(SINCE), 10)),
        "no lead holds o/p"
    );
}

#[test]
fn test_act_reads_its_words() {
    assert_eq!(Act::parse("take"), Some(Act::Take));
    assert_eq!(Act::parse("renew"), Some(Act::Renew));
    assert_eq!(Act::parse("give"), Some(Act::Give));
    assert_eq!(Act::parse("show"), None);
}

fn facts(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

const MODELS: (&str, &str) = ("models", "lead=claude:m:medium");

#[test]
fn test_start_with_no_claim_and_the_facts_set() {
    assert_eq!(should_start(None, &facts(&[MODELS]), at(SINCE)), Ok(()));
}

#[test]
fn test_start_over_a_lapsed_claim() {
    let lead = held("beta", "lead-2", SINCE);
    let now = at(SINCE) + 10 * 60;
    assert_eq!(should_start(Some(&lead), &facts(&[MODELS]), now), Ok(()));
}

#[test]
fn test_no_start_while_a_claim_is_live() {
    let lead = held("beta", "lead-2", SINCE);
    let now = at(SINCE) + 9 * 60;
    let why = should_start(Some(&lead), &facts(&[MODELS]), now).unwrap_err();
    assert_eq!(why, ["lead-2 on beta holds the lead"]);
}

#[test]
fn test_the_lapse_fact_decides_when_a_claim_is_free() {
    let lead = held("beta", "lead-2", SINCE);
    let skills = facts(&[MODELS, ("lead_lapse", "30")]);
    assert!(should_start(Some(&lead), &skills, at(SINCE) + 20 * 60).is_err());
    assert_eq!(
        should_start(Some(&lead), &skills, at(SINCE) + 30 * 60),
        Ok(())
    );
}

#[test]
fn test_no_start_in_drain_or_pause_or_without_models() {
    let drain = facts(&[MODELS, ("mode", "drain")]);
    assert_eq!(
        should_start(None, &drain, at(SINCE)).unwrap_err(),
        ["mode is drain"]
    );
    let pause = facts(&[MODELS, ("mode", "pause")]);
    assert_eq!(
        should_start(None, &pause, at(SINCE)).unwrap_err(),
        ["mode is pause"]
    );
    assert_eq!(
        should_start(None, &facts(&[]), at(SINCE)).unwrap_err(),
        ["no models"]
    );
}

#[test]
fn test_every_reason_is_named() {
    let lead = held("beta", "lead-2", SINCE);
    let skills = facts(&[("mode", "pause")]);
    let why = should_start(Some(&lead), &skills, at(SINCE)).unwrap_err();
    assert_eq!(
        why,
        [
            "lead-2 on beta holds the lead",
            "mode is pause",
            "no models"
        ]
    );
}

#[test]
fn test_one_attempt_per_lapse_window() {
    let skills = facts(&[]);
    let t = at(SINCE);
    assert!(attempt_due(None, t, &skills));
    assert!(!attempt_due(Some(t), t + 9 * 60, &skills));
    assert!(attempt_due(Some(t), t + 10 * 60, &skills));
}
