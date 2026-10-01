use super::*;

#[test]
fn test_ready_counts_the_migrations_applied() {
    assert_eq!(ready(&[]), "database ready, no migrations applied");
    assert_eq!(
        ready(&["m1".to_string()]),
        "database ready, 1 migration applied (m1)"
    );
    assert_eq!(
        ready(&["m1".to_string(), "m2".to_string()]),
        "database ready, 2 migrations applied (m1, m2)"
    );
}
