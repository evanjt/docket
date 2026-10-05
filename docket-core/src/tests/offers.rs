use super::*;

fn open(id: &str) -> Item {
    let (key, num) = crate::text::split_id(id).unwrap();
    Item {
        rid: 1,
        project: "test/proj".into(),
        key,
        num,
        id: id.into(),
        title: "x".into(),
        state: "open".into(),
        turn: Some("agent".into()),
        ..Item::default()
    }
}

fn held(id: &str, branch: &str) -> Item {
    let mut row = open(id);
    row.claim_branch = Some(branch.into());
    row.claim_host = Some("box".into());
    row
}

fn verbs(offers: &[Offer]) -> Vec<&'static str> {
    offers.iter().map(|o| o.verb).collect()
}

fn find<'a>(offers: &'a [Offer], verb: &str) -> &'a Offer {
    offers.iter().find(|o| o.verb == verb).unwrap()
}

#[test]
fn test_ready_item_offers_start_ask_wait_close_drop() {
    let o = offers(&open("T1"), Kind::Work, &[]);
    assert_eq!(verbs(&o), ["start", "close", "ask", "wait", "drop"]);
    assert_eq!(find(&o, "close").needs, ["resolution"]);
    assert_eq!(find(&o, "drop").needs, ["why"]);
    assert_eq!(find(&o, "ask").needs, ["note"]);
    assert_eq!(find(&o, "wait").needs, ["until|on"]);
    assert!(o.iter().all(|x| x.branch.is_none()));
}

#[test]
fn test_claimed_item_sends_each_verb_with_the_holders_branch() {
    let o = offers(&held("T1", "audit/t1-1"), Kind::Work, &[]);
    assert_eq!(verbs(&o), ["close", "release", "ask", "wait", "drop"]);
    assert!(o.iter().all(|x| x.branch.as_deref() == Some("audit/t1-1")));
}

#[test]
fn test_closed_item_offers_reopen_with_a_reason() {
    let mut row = open("T1");
    row.state = "done".into();
    let o = offers(&row, Kind::Work, &[]);
    assert_eq!(verbs(&o), ["reopen"]);
    assert_eq!(o[0].needs, ["why"]);
}

#[test]
fn test_waiting_item_offers_resume_not_start_or_wait() {
    let mut row = open("T1");
    row.wait_on = Some("item".into());
    assert_eq!(
        verbs(&offers(&row, Kind::Work, &[])),
        ["resume", "close", "ask", "drop"]
    );
}

#[test]
fn test_parked_item_offers_reply_and_a_question_offers_answer() {
    let mut row = open("T1");
    row.turn = Some("user".into());
    let o = offers(&row, Kind::Work, &[]);
    assert_eq!(verbs(&o), ["reply", "close", "wait", "drop"]);
    assert_eq!(find(&o, "reply").needs, ["note"]);
    let mut q = open("Q1");
    q.turn = Some("user".into());
    let o = offers(&q, Kind::Decision, &[]);
    assert_eq!(verbs(&o)[0], "answer");
    assert_eq!(find(&o, "answer").needs, ["decision"]);
    assert!(!verbs(&o).contains(&"close"));
    q.decision = Some("go".into());
    assert!(verbs(&offers(&q, Kind::Decision, &[])).contains(&"close"));
}

#[test]
fn test_plan_with_open_work_is_offered_close_only_while_held() {
    let plan = open("A1");
    let pending = ["T1".to_string()];
    assert!(!verbs(&offers(&plan, Kind::Audit, &pending)).contains(&"close"));
    let held = held("A1", "audit/a1-1");
    assert!(verbs(&offers(&held, Kind::Audit, &pending)).contains(&"close"));
    assert!(verbs(&offers(&plan, Kind::Audit, &[])).contains(&"close"));
}
