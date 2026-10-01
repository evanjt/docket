use super::*;

#[test]
fn test_stamp_formats_utc_seconds() {
    assert_eq!(stamp(0), "1970-01-01T00:00:00Z");
    assert_eq!(stamp(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(stamp(1_790_858_847), "2026-10-01T12:47:27Z");
    assert_eq!(stamp(4_102_444_799), "2099-12-31T23:59:59Z");
}

#[test]
fn test_now_has_the_stored_shape() {
    let n = now();
    assert_eq!(n.len(), 20);
    assert!(n.starts_with("20"));
    assert!(n.ends_with('Z'));
}
