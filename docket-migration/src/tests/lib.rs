use super::*;

use crate::scratch::Scratch;

#[test]
fn test_numbered_counts_each_mark_in_order() {
    assert_eq!(
        numbered("SELECT * FROM items WHERE project=? AND rid IN (?,?)"),
        "SELECT * FROM items WHERE project=$1 AND rid IN ($2,$3)"
    );
}

#[test]
fn test_numbered_leaves_marks_inside_quotes() {
    assert_eq!(
        numbered("SELECT '?', \"a?\" FROM t WHERE x=? AND y='it''s ?'"),
        "SELECT '?', \"a?\" FROM t WHERE x=$1 AND y='it''s ?'"
    );
}

#[test]
fn test_numbered_without_marks_is_unchanged() {
    assert_eq!(numbered(""), "");
    assert_eq!(numbered("SELECT 1"), "SELECT 1");
}

#[test]
fn test_postgres_url_reads_either_scheme() {
    assert_eq!(
        postgres_url("postgresql://app:pw@db-rw.default:5432/app"),
        "postgres://app:pw@db-rw.default:5432/app"
    );
    assert_eq!(postgres_url("postgres://u@h/d"), "postgres://u@h/d");
}

#[tokio::test]
async fn test_migrate_applies_once_then_nothing() {
    let s = Scratch::bare(2).await;
    assert_eq!(migrate(&s.db).await.unwrap(), ["m20261001_000001_schema"]);
    assert!(migrate(&s.db).await.unwrap().is_empty());
    let tables = s
        .db
        .query_one_raw(statement(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public' AND table_name IN \
             ('projects', 'roots', 'items', 'events', 'links', 'search', 'pending_dump', 'chores', 'meta')",
            vec![],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index::<i64>(0)
        .unwrap();
    assert_eq!(tables, 9);
}

#[tokio::test]
async fn test_migrate_from_two_servers_at_once_applies_once() {
    let s = Scratch::bare(4).await;
    let (a, b) = tokio::join!(migrate(&s.db), migrate(&s.db));
    let mut applied = a.unwrap();
    applied.extend(b.unwrap());
    assert_eq!(applied, ["m20261001_000001_schema"]);
}
