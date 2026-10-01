use super::*;

#[test]
fn test_with_database_replaces_the_name() {
    assert_eq!(
        with_database("postgres://u:p@127.0.0.1:5432/postgres", "t1"),
        "postgres://u:p@127.0.0.1:5432/t1"
    );
}

#[test]
fn test_with_database_keeps_the_query() {
    assert_eq!(
        with_database("postgres://u@db/app?sslmode=disable", "t2"),
        "postgres://u@db/t2?sslmode=disable"
    );
}

#[test]
fn test_with_database_adds_a_name_when_none_is_given() {
    assert_eq!(
        with_database("postgres://db:5432", "t3"),
        "postgres://db:5432/t3"
    );
}

#[tokio::test]
async fn test_scratch_database_is_dropped_with_its_guard() {
    let s = Scratch::bare(1).await;
    let name = s.name.clone();
    let admin = s.admin.clone();
    drop(s);
    let root = Database::connect(&admin).await.unwrap();
    let left = root
        .query_one_raw(crate::statement(
            "SELECT COUNT(*) FROM pg_database WHERE datname=?",
            vec![name.into()],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index::<i64>(0)
        .unwrap();
    assert_eq!(left, 0);
}
