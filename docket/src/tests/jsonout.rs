use super::*;

#[test]
fn test_float_repr_fixed_and_exponent() {
    assert_eq!(float_repr(1.0), "1.0");
    assert_eq!(float_repr(-5.25), "-5.25");
    assert_eq!(float_repr(0.0001), "0.0001");
    assert_eq!(float_repr(0.00001), "1e-05");
    assert_eq!(float_repr(-1.5e-7), "-1.5e-07");
    assert_eq!(float_repr(1e16), "1e+16");
    assert_eq!(float_repr(123_456.789), "123456.789");
    assert_eq!(float_repr(-0.000_002_345_6), "-2.3456e-06");
}

#[test]
fn test_dumps_indent_nests_and_keeps_order() {
    let v = Json::Dict(vec![
        ("b".into(), Json::List(vec![Json::Int(1), Json::Null])),
        ("a".into(), Json::Dict(vec![])),
        ("c".into(), Json::List(vec![])),
    ]);
    assert_eq!(
        dumps_indent(&v),
        "{\n  \"b\": [\n    1,\n    null\n  ],\n  \"a\": {},\n  \"c\": []\n}"
    );
    assert_eq!(dumps_line(&v), "{\"b\": [1, null], \"a\": {}, \"c\": []}");
}

#[test]
fn test_dumps_escapes_controls_and_keeps_unicode() {
    let v = Json::str("é\"\\\n\u{1}\u{7f}");
    assert_eq!(dumps_line(&v), "\"é\\\"\\\\\\n\\u0001\u{7f}\"");
}

#[test]
fn test_cut_counts_characters() {
    assert_eq!(cut("héllo", 2), "hé");
    assert_eq!(cut("ab", 5), "ab");
    assert_eq!(or_none(None), "None");
}
