use super::*;

#[test]
fn test_parse_reads_hosts_and_roles() {
    let keys = Keys::parse("# keys\n\nbuildbox agent abc\nlaptop owner xyz\n").unwrap();
    assert_eq!(
        keys.caller("abc"),
        Some(&Caller {
            host: "devbox".into(),
            owner: false
        })
    );
    assert_eq!(
        keys.caller("xyz"),
        Some(&Caller {
            host: "laptop".into(),
            owner: true
        })
    );
}

#[test]
fn test_parse_refuses_unknown_role_and_short_line() {
    assert!(Keys::parse("devbox admin abc").is_err());
    assert!(Keys::parse("devbox abc").is_err());
}

#[test]
fn test_caller_unknown_or_prefix_key_is_none() {
    let keys = Keys::parse("devbox agent abcdef").unwrap();
    assert_eq!(keys.caller("abc"), None);
    assert_eq!(keys.caller(""), None);
    assert_eq!(Keys::default().caller("abcdef"), None);
}
