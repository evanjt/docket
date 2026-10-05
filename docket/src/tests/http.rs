use super::*;

#[test]
fn test_an_unreadable_answer_says_the_write_may_have_landed() {
    let text = failed("http://h", &Error::Unreadable("new: missing field".into())).to_string();
    assert!(!text.contains("cannot reach"), "{text}");
    assert!(text.contains("may have landed"), "{text}");
    assert!(text.contains("update the client"), "{text}");
}

#[test]
fn test_a_network_failure_still_says_cannot_reach() {
    let text = failed("http://h", &Error::Failed("refused".into())).to_string();
    assert!(text.contains("cannot reach the server"), "{text}");
}
