use super::*;

use crate::scratch::Scratch;

const MIGRATIONS: [&str; 10] = [
    "m20261001_000001_schema",
    "m20261002_000001_machines_and_leads",
    "m20261005_000001_owner_facts",
    "m20261005_000002_runner_limits",
    "m20261005_000003_machine_path",
    "m20261005_000004_assignments",
    "m20261006_000001_dependencies",
    "m20261006_000002_condition_waits",
    "m20261006_000003_ask_need",
    "m20261006_000004_job_reports",
];

/// The migrations to apply to stand just before the one named.
fn steps_before(name: &str) -> u32 {
    let position = Migrator::migrations()
        .iter()
        .position(|m| m.name() == name)
        .unwrap_or_else(|| panic!("no migration named {name}"));
    u32::try_from(position).unwrap()
}

/// A migration's number is its name up to the label: `m20261005_000001`.
fn number_of(name: &str) -> &str {
    name.match_indices('_')
        .nth(1)
        .map_or(name, |(end, _)| &name[..end])
}

#[test]
fn test_migration_numbers_are_unique_and_in_order() {
    let names: Vec<String> = Migrator::migrations()
        .iter()
        .map(|m| m.name().to_string())
        .collect();
    for pair in names.windows(2) {
        assert!(
            number_of(&pair[0]) < number_of(&pair[1]),
            "{} and {} share a number or are out of order",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn test_number_of_reads_the_date_and_sequence() {
    assert_eq!(
        number_of("m20261005_000001_owner_facts"),
        "m20261005_000001"
    );
}

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
    assert_eq!(migrate(&s.db).await.unwrap(), MIGRATIONS);
    assert!(migrate(&s.db).await.unwrap().is_empty());
    let tables = s
        .db
        .query_one_raw(statement(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public' AND table_name IN \
             ('projects', 'roots', 'items', 'events', 'links', 'search', 'pending_dump', 'chores', 'meta', \
             'machines', 'leads', 'owner_facts', 'assignments', 'dependencies')",
            vec![],
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index::<i64>(0)
        .unwrap();
    assert_eq!(tables, 14);
}

#[tokio::test]
async fn test_migrate_from_two_servers_at_once_applies_once() {
    let s = Scratch::bare(4).await;
    let (a, b) = tokio::join!(migrate(&s.db), migrate(&s.db));
    let mut applied = a.unwrap();
    applied.extend(b.unwrap());
    assert_eq!(applied, MIGRATIONS);
}

#[tokio::test]
async fn test_the_assignments_migration_rebuilds_each_items_attempts_from_its_events() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261005_000004_assignments")))
        .await
        .unwrap();
    s.seed(
        "INSERT INTO projects (slug, created_at, updated_at) VALUES ('garden/shed', 'c', 'u'); \
         INSERT INTO items (rid, project, key, num, title, state, turn, resolution, opened_at, updated_at) VALUES \
           (1, 'garden/shed', 'T', 1, 'Oil the hinges', 'done', NULL, 'abc1234', 'o', 'u1'); \
         INSERT INTO items (rid, project, key, num, title, state, turn, claim_branch, claim_host, claim_since, \
           claim_runner, claim_job, claim_on, opened_at, updated_at) VALUES \
           (2, 'garden/shed', 'T', 2, 'Hang the rake', 'open', 'agent', 'build/t2-1', 'bench', 's2', \
            'codex', 'build-t2-1', 'potting', 'o', 'u2'); \
         INSERT INTO items (rid, project, key, num, title, state, turn, turn_note, asked_at, opened_at, updated_at) VALUES \
           (3, 'garden/shed', 'T', 3, 'Paint the door', 'open', 'user', 'pick a colour', 'a3', 'o', 'u3'); \
         INSERT INTO events (uid, project, rid, at, host, branch, kind, note) VALUES \
           ('e1', 'garden/shed', 1, 't1', 'bench', 'build/t1-1', 'claimed', NULL), \
           ('e2', 'garden/shed', 1, 't2', 'bench', 'build/t1-1', 'released', 'merge conflict'), \
           ('e3', 'garden/shed', 1, 't3', 'bench', 'build/t1-2', 'claimed', NULL), \
           ('e4', 'garden/shed', 1, 't4', 'bench', 'build/t1-2', 'closed', 'abc1234')",
    )
    .await;
    assert_eq!(
        migrate(&s.db).await.unwrap(),
        [
            "m20261005_000004_assignments",
            "m20261006_000001_dependencies",
            "m20261006_000002_condition_waits",
            "m20261006_000003_ask_need",
            "m20261006_000004_job_reports"
        ]
    );
    let rows = s
        .db
        .query_all_raw(statement(
            "SELECT rid, kind, assignee, started_at, COALESCE(ended_at, '-'), COALESCE(outcome, '-'), \
             COALESCE(machine, '-') FROM assignments ORDER BY rid, id",
            vec![],
        ))
        .await
        .unwrap();
    let got: Vec<(i64, String, String, String, String, String, String)> = rows
        .iter()
        .map(|r| {
            (
                r.try_get_by_index(0).unwrap(),
                r.try_get_by_index(1).unwrap(),
                r.try_get_by_index(2).unwrap(),
                r.try_get_by_index(3).unwrap(),
                r.try_get_by_index(4).unwrap(),
                r.try_get_by_index(5).unwrap(),
                r.try_get_by_index(6).unwrap(),
            )
        })
        .collect();
    let row = |rid, kind: &str, who: &str, from: &str, to: &str, outcome: &str, machine: &str| {
        (
            rid,
            kind.to_string(),
            who.to_string(),
            from.to_string(),
            to.to_string(),
            outcome.to_string(),
            machine.to_string(),
        )
    };
    assert_eq!(
        got,
        [
            row(1, "claim", "agent", "t1", "t2", "conflict", "-"),
            row(1, "claim", "agent", "t3", "t4", "landed", "-"),
            row(2, "claim", "agent", "s2", "-", "-", "potting"),
            row(3, "ask", "owner", "a3", "-", "-", "-"),
        ]
    );
}

async fn pairs(s: &Scratch, text: &str) -> Vec<(String, String)> {
    s.db.query_all_raw(statement(text, vec![]))
        .await
        .unwrap()
        .iter()
        .map(|r| {
            (
                r.try_get_by_index(0).unwrap(),
                r.try_get_by_index(1).unwrap(),
            )
        })
        .collect()
}

#[tokio::test]
async fn test_each_item_wait_becomes_a_dependency_and_the_edge_closing_a_cycle_is_related() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261006_000001_dependencies")))
        .await
        .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('test/proj',
  '[{"key": "T", "kind": "work"}, {"key": "A", "kind": "audit"}]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, opened_at, updated_at) VALUES
  (1, 'test/proj', 'T', 1, 'kiln', 'open', 'agent', 'o', 'u'),
  (2, 'test/proj', 'T', 2, 'glaze', 'open', 'agent', 'o', 'u'),
  (3, 'test/proj', 'T', 3, 'clay', 'open', 'agent', 'o', 'u'),
  (4, 'test/proj', 'T', 4, 'wheel', 'open', 'agent', 'o', 'u'),
  (5, 'test/proj', 'T', 5, 'shelf', 'open', 'agent', 'o', 'u'),
  (6, 'test/proj', 'A', 1, 'studio', 'open', 'agent', 'o', 'u');
UPDATE items SET wait_on='item', wait_item=2, wait_ref='T2', wait_since='w1' WHERE rid=1;
UPDATE items SET wait_on='item', wait_item=3, wait_ref='T3', wait_since='w2' WHERE rid=2;
UPDATE items SET wait_on='item', wait_item=1, wait_ref='T1', wait_since='w3' WHERE rid=3;
UPDATE items SET wait_on='item', wait_item=6, wait_ref='A1', wait_since='w4' WHERE rid=4;
UPDATE items SET wait_on='item', wait_item=2, wait_ref='T2', wait_since='w5' WHERE rid=5;
UPDATE items SET wait_on='condition', wait_ref='everything it opened is closed', wait_since='w6' WHERE rid=6;
INSERT INTO links (rid, kind, to_rid) VALUES (4, 'opened', 6);
"#,
    )
    .await;
    migrate(&s.db).await.unwrap();
    let deps = pairs(
        &s,
        "SELECT a.id, b.id FROM dependencies d JOIN items a ON a.rid=d.rid JOIN items b ON b.rid=d.on_rid \
         ORDER BY a.id, b.id",
    )
    .await;
    let want = [("T1", "T2"), ("T2", "T3"), ("T5", "T2")];
    assert_eq!(deps, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let related = pairs(
        &s,
        "SELECT a.id, b.id FROM links l JOIN items a ON a.rid=l.rid JOIN items b ON b.rid=l.to_rid \
         WHERE l.kind='related' ORDER BY a.id, b.id",
    )
    .await;
    let want = [("A1", "T4"), ("T1", "T3"), ("T3", "T1"), ("T4", "A1")];
    assert_eq!(related, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let still = pairs(
        &s,
        "SELECT id, COALESCE(wait_ref, '') FROM items WHERE wait_on IS NOT NULL ORDER BY id",
    )
    .await;
    let want = [
        ("A1", "everything it opened is closed"),
        ("T1", "T2"),
        ("T2", "T3"),
        ("T5", "T2"),
    ];
    assert_eq!(still, want.map(|(a, b)| (a.to_string(), b.to_string())));
}

#[tokio::test]
async fn test_a_stored_condition_wait_becomes_a_task_for_the_owner_and_a_dependency() {
    let s = Scratch::bare(2).await;
    Migrator::up(
        &s.db,
        Some(steps_before("m20261006_000002_condition_waits")),
    )
    .await
    .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('test/proj',
  '[{"key": "T", "kind": "work"}, {"key": "A", "kind": "audit"}]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, theme, opened_at, updated_at) VALUES
  (1, 'test/proj', 'T', 1, 'fire', 'open', 'agent', '1.0.0', 'o', 'u'),
  (2, 'test/proj', 'T', 2, 'glaze', 'open', 'agent', NULL, 'o', 'u'),
  (3, 'test/proj', 'A', 1, 'studio', 'open', 'agent', NULL, 'o', 'u');
UPDATE items SET wait_on='condition', wait_ref='the kiln cools', wait_since='w1' WHERE rid=1;
UPDATE items SET wait_on='item', wait_item=1, wait_ref='T1', wait_since='w2' WHERE rid=2;
UPDATE items SET wait_on='condition', wait_ref='everything it opened is closed', wait_since='w3' WHERE rid=3;
INSERT INTO dependencies (rid, on_rid, created_at) VALUES (2, 1, 'w2');
"#,
    )
    .await;
    migrate(&s.db).await.unwrap();
    let waits = pairs(
        &s,
        "SELECT id, COALESCE(wait_ref, '') FROM items WHERE wait_on IS NOT NULL ORDER BY id",
    )
    .await;
    let want = [
        ("A1", "everything it opened is closed"),
        ("T1", "T3"),
        ("T2", "T1"),
    ];
    assert_eq!(waits, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let tasks = s
        .db
        .query_all_raw(statement(
            "SELECT t.id, t.title, t.turn, COALESCE(t.theme, '-'), t.turn_note, w.wait_item = t.rid, \
             w.wait_since, t.opened_at \
             FROM items t JOIN items w ON w.wait_ref = t.id WHERE t.num = 3 AND t.key = 'T'",
            vec![],
        ))
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    let t = &tasks[0];
    assert_eq!(t.try_get_by_index::<String>(0).unwrap(), "T3");
    assert_eq!(t.try_get_by_index::<String>(1).unwrap(), "the kiln cools");
    assert_eq!(t.try_get_by_index::<String>(2).unwrap(), "user");
    assert_eq!(t.try_get_by_index::<String>(3).unwrap(), "1.0.0");
    assert_eq!(t.try_get_by_index::<String>(4).unwrap(), "the kiln cools");
    assert!(t.try_get_by_index::<bool>(5).unwrap());
    assert_eq!(t.try_get_by_index::<String>(6).unwrap(), "w1");
    assert_eq!(t.try_get_by_index::<String>(7).unwrap(), "w1");
    let deps = pairs(
        &s,
        "SELECT a.id, b.id FROM dependencies d JOIN items a ON a.rid=d.rid JOIN items b ON b.rid=d.on_rid \
         ORDER BY a.id, b.id",
    )
    .await;
    let want = [("T1", "T3"), ("T2", "T1")];
    assert_eq!(deps, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let related = pairs(
        &s,
        "SELECT a.id, b.id FROM links l JOIN items a ON a.rid=l.rid JOIN items b ON b.rid=l.to_rid \
         WHERE l.kind='related' ORDER BY a.id, b.id",
    )
    .await;
    let want = [("T1", "T3"), ("T3", "T1")];
    assert_eq!(related, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let opened = pairs(
        &s,
        "SELECT i.id, e.kind FROM events e JOIN items i ON i.rid=e.rid WHERE i.id='T3'",
    )
    .await;
    assert_eq!(opened, [("T3".to_string(), "opened".to_string())]);
    let asks = pairs(
        &s,
        "SELECT i.id, a.kind FROM assignments a JOIN items i ON i.rid=a.rid WHERE i.id='T3'",
    )
    .await;
    assert_eq!(asks, [("T3".to_string(), "ask".to_string())]);
}
