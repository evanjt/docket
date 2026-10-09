use super::*;

use crate::scratch::Scratch;

const MIGRATIONS: [&str; 29] = [
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
    "m20261006_000005_releases",
    "m20261006_000006_parents",
    "m20261006_000007_assignee",
    "m20261006_000008_areas",
    "m20261006_000009_job_times",
    "m20261006_000010_plan_gates",
    "m20261006_000011_area_places",
    "m20261006_162014_item_types",
    "m20261006_163830_labels",
    "m20261006_164713_area_placements",
    "m20261006_174229_publications",
    "m20261006_202103_column_labels",
    "m20261006_210713_open_assignments",
    "m20261006_213453_drop_old_columns",
    "m20261006_220000_remapped_events",
    "m20261006_233349_items_area_not_null",
    "m20261006_235900_outcome_ended",
    "m20261007_073521_events_close_index",
    "m20261009_073000_project_rename",
];

/// A project with one row in every table that references its slug, as the tree stands just before
/// the migration that lets the slug change.
const ONE_ROW_EACH: &str = "\
    INSERT INTO projects (slug, created_at, updated_at) VALUES ('garden/shed', 'c', 'u'); \
    INSERT INTO roots (host, path, project, bound_at, how) VALUES ('bench', '/srv/shed', 'garden/shed', 'b', 'bind'); \
    INSERT INTO areas (id, project, name, position) VALUES (1, 'garden/shed', 'tools', 1); \
    INSERT INTO items (rid, project, key, num, title, state, opened_at, updated_at, area_id) VALUES \
      (1, 'garden/shed', 'T', 1, 'Oil the hinges', 'open', 'o', 'u', 1); \
    INSERT INTO events (uid, project, rid, at, host, kind) VALUES ('e1', 'garden/shed', 1, 't1', 'bench', 'opened'); \
    INSERT INTO leads (project, host, session, since, renewed_at) VALUES ('garden/shed', 'bench', 's', 'a', 'r'); \
    INSERT INTO releases (project, name, position) VALUES ('garden/shed', '1.0.0', 1); \
    INSERT INTO labels (project, name) VALUES ('garden/shed', 'slow-path'); \
    INSERT INTO publications (project, published_sha, work_sha, created_at) VALUES \
      ('garden/shed', repeat('a', 40), repeat('b', 40), 'c')";

const RENAME: &str = "UPDATE projects SET slug='other/name' WHERE slug='garden/shed'";

async fn rows_named(s: &Scratch, table: &str, slug: &str) -> i64 {
    s.db.query_one_raw(statement(
        &format!("SELECT COUNT(*) FROM {table} WHERE project=?"),
        vec![slug.into()],
    ))
    .await
    .unwrap()
    .unwrap()
    .try_get_by_index::<i64>(0)
    .unwrap()
}

#[tokio::test]
async fn test_the_rename_migration_lets_every_referencing_row_follow_the_slug() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261009_073000_project_rename")))
        .await
        .unwrap();
    s.seed(ONE_ROW_EACH).await;
    assert!(
        s.db.execute_unprepared(RENAME).await.is_err(),
        "the plain foreign keys refuse the change before the migration"
    );
    assert!(
        s.db.execute_unprepared(
            "INSERT INTO events (uid, project, at, host, kind, note) VALUES \
                 ('e2', 'garden/shed', 't2', 'bench', 'renamed', 'from garden/shed')"
        )
        .await
        .is_err(),
        "renamed is not an event kind before the migration"
    );
    Migrator::up(&s.db, Some(1)).await.unwrap();
    s.db.execute_unprepared(RENAME).await.unwrap();
    for table in [
        "roots",
        "items",
        "events",
        "leads",
        "releases",
        "areas",
        "labels",
        "publications",
    ] {
        assert_eq!(rows_named(&s, table, "other/name").await, 1, "{table}");
        assert_eq!(rows_named(&s, table, "garden/shed").await, 0, "{table}");
    }
    s.db.execute_unprepared(
        "INSERT INTO events (uid, project, at, host, kind, note) VALUES \
         ('e2', 'other/name', 't2', 'bench', 'renamed', 'from garden/shed')",
    )
    .await
    .unwrap();
}

/// The migrations to apply to stand just before the one named.
fn steps_before(name: &str) -> u32 {
    let position = Migrator::migrations()
        .iter()
        .position(|m| m.name() == name)
        .unwrap_or_else(|| panic!("no migration named {name}"));
    u32::try_from(position).unwrap()
}

/// Every migration still to apply up to the drop of the columns the core replaced, so a test of an
/// earlier one reads what those columns held.
async fn migrate_before_the_drop(s: &Scratch) {
    let applied = Migrator::get_applied_migrations(&s.db).await.unwrap().len();
    let left = steps_before("m20261006_213453_drop_old_columns") - u32::try_from(applied).unwrap();
    Migrator::up(&s.db, Some(left)).await.unwrap();
}

/// Every migration still to apply up to the one that makes an item's area required, named as applied,
/// so a test of an earlier one can seed and read items that hold no area.
async fn migrate_before_area_required(s: &Scratch) -> Vec<String> {
    let applied = Migrator::get_applied_migrations(&s.db).await.unwrap().len();
    let stop = steps_before("m20261006_233349_items_area_not_null") as usize;
    let all: Vec<String> = Migrator::migrations()
        .iter()
        .map(|m| m.name().to_string())
        .collect();
    Migrator::up(&s.db, Some(u32::try_from(stop - applied).unwrap()))
        .await
        .unwrap();
    all[applied..stop].to_vec()
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
    assert_eq!(
        migrate(&s.db).await.unwrap(),
        [] as [std::string::String; 0]
    );
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
        migrate_before_area_required(&s).await,
        MIGRATIONS[MIGRATIONS
            .iter()
            .position(|m| *m == "m20261005_000004_assignments")
            .unwrap()
            ..steps_before("m20261006_233349_items_area_not_null") as usize]
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
    migrate_before_the_drop(&s).await;
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
    let want = [("T1", "T2"), ("T2", "T3"), ("T5", "T2")];
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
    migrate_before_the_drop(&s).await;
    let waits = pairs(
        &s,
        "SELECT id, COALESCE(wait_ref, '') FROM items WHERE wait_on IS NOT NULL ORDER BY id",
    )
    .await;
    let want = [("T1", "T3"), ("T2", "T1")];
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

#[tokio::test]
async fn test_a_releases_fact_of_three_names_becomes_three_rows_in_order() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261006_000005_releases")))
        .await
        .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, skills, created_at, updated_at) VALUES ('test/proj',
  '[{"key": "T", "kind": "work"}, {"key": "A", "kind": "audit"}]',
  '{"releases": "0.3.0 0.2.0 1.0.0", "owner": "the potter"}', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, theme, opened_at, updated_at) VALUES
  (1, 'test/proj', 'T', 1, 'kiln', 'open', 'agent', '0.2.0', 'o', 'u'),
  (2, 'test/proj', 'T', 2, 'glaze', 'open', 'agent', NULL, 'o', 'u'),
  (3, 'test/proj', 'T', 3, 'shelf', 'open', 'agent', 'tooling', 'o', 'u'),
  (4, 'test/proj', 'T', 4, 'wheel', 'open', 'agent', '2.0.0', 'o', 'u');
"#,
    )
    .await;
    migrate_before_the_drop(&s).await;
    let rows = pairs(
        &s,
        "SELECT name, position::text FROM releases WHERE project='test/proj' ORDER BY position",
    )
    .await;
    let want = [
        ("0.3.0", "0"),
        ("0.2.0", "1"),
        ("1.0.0", "2"),
        ("2.0.0", "3"),
    ];
    assert_eq!(rows, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let placed = pairs(
        &s,
        "SELECT i.id, COALESCE(r.name, '-') || ' ' || COALESCE(i.theme, '-') FROM items i \
         LEFT JOIN releases r ON r.id=i.release_id ORDER BY i.id",
    )
    .await;
    let want = [
        ("T1", "0.2.0 0.2.0"),
        ("T2", "0.3.0 -"),
        ("T3", "- tooling"),
        ("T4", "2.0.0 2.0.0"),
    ];
    assert_eq!(placed, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let facts = pairs(&s, "SELECT slug, skills::text FROM projects ORDER BY slug").await;
    assert_eq!(
        facts,
        [(
            "test/proj".to_string(),
            r#"{"owner": "the potter"}"#.to_string()
        )]
    );
}

#[tokio::test]
async fn test_an_opened_link_to_a_plan_becomes_the_parent_and_the_rest_origins() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261006_000006_parents")))
        .await
        .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('test/proj',
  '[{"key": "T", "kind": "work"}, {"key": "Q", "kind": "decision"}, {"key": "A", "kind": "audit"}]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, opened_at, updated_at) VALUES
  (1, 'test/proj', 'A', 1, 'orchard', 'open', 'agent', 'o1', 'u'),
  (2, 'test/proj', 'A', 2, 'hedge', 'open', 'agent', 'o2', 'u'),
  (3, 'test/proj', 'Q', 1, 'which pears', 'open', 'user', 'o3', 'u'),
  (4, 'test/proj', 'T', 1, 'graft', 'open', 'agent', 'o4', 'u'),
  (5, 'test/proj', 'T', 2, 'prune', 'open', 'agent', 'o5', 'u'),
  (6, 'test/proj', 'T', 3, 'mulch', 'open', 'agent', 'o6', 'u'),
  (7, 'test/proj', 'T', 4, 'water', 'open', 'agent', 'o7', 'u');
INSERT INTO links (rid, kind, to_rid) VALUES
  (3, 'opened', 1), (4, 'opened', 3), (5, 'opened', 1), (5, 'opened', 2), (7, 'opened', 6);
"#,
    )
    .await;
    migrate_before_area_required(&s).await;
    let parents = pairs(
        &s,
        "SELECT a.id, b.id FROM items a JOIN items b ON b.rid=a.parent_rid ORDER BY a.id",
    )
    .await;
    let want = [("Q1", "A1"), ("T1", "A1"), ("T2", "A1")];
    assert_eq!(parents, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let links = pairs(
        &s,
        "SELECT a.id || ' ' || l.kind, b.id FROM links l JOIN items a ON a.rid=l.rid \
         JOIN items b ON b.rid=l.to_rid ORDER BY 1, 2",
    )
    .await;
    let want = [
        ("A2 related", "T2"),
        ("T1 origin", "Q1"),
        ("T2 related", "A2"),
        ("T4 origin", "T3"),
    ];
    assert_eq!(links, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let refused =
        s.db.execute_raw(statement(
            "INSERT INTO links (rid, kind, to_rid) VALUES (6, 'opened', 1)",
            vec![],
        ))
        .await;
    assert!(refused.is_err());
    let pending: Vec<i64> =
        s.db.query_all_raw(statement(
            "SELECT rid FROM pending_dump ORDER BY rid",
            vec![],
        ))
        .await
        .unwrap()
        .iter()
        .map(|r| r.try_get_by_index(0).unwrap())
        .collect();
    assert_eq!(pending, [1, 2, 3, 4, 5, 6, 7]);
}

#[tokio::test]
async fn test_each_concept_becomes_an_area_and_each_item_takes_its_plans_else_its_own() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261006_000011_area_places")))
        .await
        .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('test/proj',
  '[{"key": "T", "kind": "work"}, {"key": "A", "kind": "audit"}, {"key": "CON", "kind": "concept"}]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, body, state, turn, opened_at, updated_at) VALUES
  (1, 'test/proj', 'CON', 1, 'Lanterns: paper and wire', 'Lamps for the night market.', 'open', 'agent', 'o1', 'u'),
  (2, 'test/proj', 'CON', 2, 'Kites', '', 'open', 'agent', 'o1', 'u'),
  (3, 'test/proj', 'A', 1, 'light the market', '', 'open', 'agent', 'o1', 'u'),
  (4, 'test/proj', 'T', 1, 'fold the frames', '', 'open', 'agent', 'o1', 'u'),
  (5, 'test/proj', 'T', 2, 'tie the tails', '', 'open', 'agent', 'o1', 'u'),
  (6, 'test/proj', 'T', 3, 'sweep the stalls', '', 'open', 'agent', 'o1', 'u');
UPDATE items SET parent_rid=3 WHERE rid=4;
INSERT INTO links (rid, kind, to_rid) VALUES
  (1, 'related', 3), (3, 'related', 1), (2, 'related', 4), (4, 'related', 2),
  (5, 'related', 1), (1, 'related', 5), (5, 'related', 2), (2, 'related', 5);
INSERT INTO events (uid, project, rid, at, host, kind, note) VALUES
  ('e1', 'test/proj', 5, 'o3', 'h', 'edited', 'link related CON1'),
  ('e2', 'test/proj', 2, 'o2', 'h', 'edited', 'link related T2');
"#,
    )
    .await;
    migrate_before_area_required(&s).await;
    let areas = pairs(
        &s,
        "SELECT name, position::text || ' ' || description FROM areas ORDER BY position",
    )
    .await;
    let want = [
        (
            "lanterns",
            "0 Lanterns: paper and wire\n\nLamps for the night market.",
        ),
        ("kites", "1 Kites"),
    ];
    assert_eq!(areas, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let placed = pairs(
        &s,
        "SELECT i.id, COALESCE(a.name, '-') FROM items i LEFT JOIN areas a ON a.id=i.area_id \
         ORDER BY i.id",
    )
    .await;
    let want = [
        ("A1", "lanterns"),
        ("CON1", "lanterns"),
        ("CON2", "kites"),
        ("T1", "lanterns"),
        ("T2", "kites"),
        ("T3", "-"),
    ];
    assert_eq!(placed, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let dropped = pairs(
        &s,
        "SELECT id, state || ' ' || resolution FROM items WHERE key='CON' ORDER BY id",
    )
    .await;
    let want = [
        ("CON1", "dropped became area lanterns"),
        ("CON2", "dropped became area kites"),
    ];
    assert_eq!(dropped, want.map(|(a, b)| (a.to_string(), b.to_string())));
}

#[tokio::test]
async fn test_a_placement_places_what_no_concept_does_and_closed_leftovers_go_to_unsorted() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261006_000011_area_places")))
        .await
        .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES
  ('test/crafts', '[{"key": "T", "kind": "work"}, {"key": "CON", "kind": "concept"}]', 'c', 'u'),
  ('test/spans', '[{"key": "T", "kind": "work"}, {"key": "A", "kind": "audit"}]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, body, state, turn, opened_at, updated_at) VALUES
  (1, 'test/crafts', 'CON', 1, 'Kites', '', 'open', 'agent', 'o1', 'u'),
  (2, 'test/crafts', 'T', 1, 'fold the frames', '', 'open', 'agent', 'o1', 'u'),
  (3, 'test/crafts', 'T', 3, 'sweep the stalls', '', 'open', 'agent', 'o1', 'u'),
  (4, 'test/crafts', 'T', 4, 'count the coins', '', 'open', 'agent', 'o1', 'u'),
  (5, 'test/crafts', 'T', 5, 'hang the bunting', '', 'open', 'agent', 'o1', 'u'),
  (6, 'test/spans', 'A', 1, 'build the crossings', '', 'open', 'agent', 'o1', 'u'),
  (7, 'test/spans', 'T', 1, 'lay the planks', '', 'open', 'agent', 'o1', 'u');
UPDATE items SET parent_rid=6 WHERE rid=7;
UPDATE items SET state='done', turn=NULL, resolution='counted' WHERE rid=4;
INSERT INTO links (rid, kind, to_rid) VALUES (1, 'related', 2), (2, 'related', 1);
INSERT INTO events (uid, project, rid, at, host, kind, note, data) VALUES
  ('d1', 'test/crafts', 3, 'o2', 'h', 'decided', 'area kites',
   '{"derived": "the stalls sell kites", "area": "kites"}'),
  ('d2', 'test/spans', 6, 'o2', 'h', 'decided', 'area bridges',
   '{"derived": "the plan builds crossings", "area": "bridges", "about": "rope and plank crossings"}');
"#,
    )
    .await;
    migrate_before_area_required(&s).await;
    let areas = pairs(
        &s,
        "SELECT project || ' ' || name, position::text || ' ' || history::text || ' ' || \
         COALESCE(description, '-') FROM areas ORDER BY project, position",
    )
    .await;
    let want = [
        ("test/crafts kites", "0 false Kites"),
        (
            "test/crafts unsorted",
            "1 true closed items no area claimed at the migration",
        ),
        ("test/spans bridges", "0 false rope and plank crossings"),
    ];
    assert_eq!(areas, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let placed = pairs(
        &s,
        "SELECT i.project || ' ' || i.id, COALESCE(a.name, '-') FROM items i \
         LEFT JOIN areas a ON a.id=i.area_id ORDER BY i.project, i.id",
    )
    .await;
    let want = [
        ("test/crafts CON1", "kites"),
        ("test/crafts T1", "kites"),
        ("test/crafts T3", "kites"),
        ("test/crafts T4", "unsorted"),
        ("test/crafts T5", "-"),
        ("test/spans A1", "bridges"),
        ("test/spans T1", "bridges"),
    ];
    assert_eq!(placed, want.map(|(a, b)| (a.to_string(), b.to_string())));
}

#[tokio::test]
async fn test_an_assignment_names_the_agent_or_the_owner_and_no_one_else() {
    let s = Scratch::new(2).await;
    s.seed(
        r"
INSERT INTO projects (slug, created_at, updated_at) VALUES ('test/proj', 'c', 'u');
INSERT INTO areas (id, project, name, position) VALUES (1, 'test/proj', 'kiln', 0);
INSERT INTO items (rid, project, key, num, title, state, area_id, opened_at, updated_at) VALUES
  (1, 'test/proj', 'T', 1, 'fire', 'open', 1, 'o', 'u');
",
    )
    .await;
    let insert = |assignee: &str| {
        statement(
            &format!(
                "INSERT INTO assignments (rid, assignee, kind, started_at, host) \
                 VALUES (1, '{assignee}', 'ask', 's', 'h')"
            ),
            vec![],
        )
    };
    assert!(s.db.execute_raw(insert("someone")).await.is_err());
    assert!(s.db.execute_raw(insert("owner")).await.is_ok());
}

#[tokio::test]
async fn test_the_assignee_migration_refuses_a_row_outside_the_pair_with_its_count() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261006_000007_assignee")))
        .await
        .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('test/proj',
  '[{"key": "T", "kind": "work"}]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, opened_at, updated_at) VALUES
  (1, 'test/proj', 'T', 1, 'fire', 'open', 'agent', 'o', 'u');
INSERT INTO assignments (rid, assignee, kind, started_at, ended_at, host) VALUES
  (1, 'someone', 'ask', 's', 'e', 'h'), (1, 'other', 'ask', 's', 'e', 'h');
"#,
    )
    .await;
    let err = migrate(&s.db).await.unwrap_err().to_string();
    assert!(err.contains("2 assignments"), "{err}");
}

#[tokio::test]
async fn test_a_plan_stops_holding_a_wait_while_its_members_are_open() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261006_000010_plan_gates")))
        .await
        .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('test/proj',
  '[{"key": "T", "kind": "work"}, {"key": "A", "kind": "audit"}]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, opened_at, updated_at) VALUES
  (1, 'test/proj', 'T', 1, 'fire', 'open', 'agent', 'o', 'u'),
  (2, 'test/proj', 'A', 1, 'studio', 'open', 'agent', 'o', 'u'),
  (3, 'test/proj', 'A', 2, 'yard', 'open', 'agent', 'o', 'u');
UPDATE items SET wait_on='condition', wait_ref='the kiln cools', wait_since='w1' WHERE rid=1;
UPDATE items SET wait_on='condition', wait_ref='everything it opened is closed', wait_since='w2' WHERE rid=2;
UPDATE items SET wait_on='item', wait_item=1, wait_ref='T1', wait_since='w3' WHERE rid=3;
"#,
    )
    .await;
    migrate_before_the_drop(&s).await;
    let waits = pairs(
        &s,
        "SELECT id, COALESCE(wait_ref, '') FROM items WHERE wait_on IS NOT NULL ORDER BY id",
    )
    .await;
    let want = [("A2", "T1"), ("T1", "the kiln cools")];
    assert_eq!(waits, want.map(|(a, b)| (a.to_string(), b.to_string())));
}

#[tokio::test]
async fn test_a_package_item_becomes_a_plan_and_a_priority_tag_becomes_the_column() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261006_162014_item_types")))
        .await
        .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('test/proj',
  '[{"key": "T", "kind": "work"}, {"key": "B", "kind": "work"}, {"key": "Q", "kind": "decision"},
    {"key": "I", "kind": "research"}, {"key": "A", "kind": "audit"}, {"key": "PK", "kind": "package"},
    {"key": "ZZ", "kind": "work"}]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, turn, tags, opened_at, updated_at) VALUES
  (1, 'test/proj', 'T', 1, 'fold', 'open', 'agent', '["high", "single"]', 'o', 'u'),
  (2, 'test/proj', 'B', 1, 'tear', 'open', 'agent', '[]', 'o', 'u'),
  (3, 'test/proj', 'Q', 1, 'which', 'open', 'user', '["low"]', 'o', 'u'),
  (4, 'test/proj', 'I', 1, 'measure', 'open', 'agent', '[]', 'o', 'u'),
  (5, 'test/proj', 'A', 1, 'light', 'open', 'agent', '["critical"]', 'o', 'u'),
  (6, 'test/proj', 'PK', 1, 'one owner', 'open', 'agent', '[]', 'o', 'u'),
  (7, 'test/proj', 'ZZ', 1, 'sweep', 'open', 'agent', '[]', 'o', 'u');
"#,
    )
    .await;
    migrate_before_the_drop(&s).await;
    let got = pairs(
        &s,
        "SELECT id, type || ' ' || priority || ' ' || tags::text FROM items ORDER BY id",
    )
    .await;
    let want = [
        ("A1", "plan critical []"),
        ("B1", "bug normal []"),
        ("I1", "investigation normal []"),
        ("PK1", "plan normal []"),
        ("Q1", "question low []"),
        ("T1", r#"task high ["single"]"#),
        ("ZZ1", "task normal []"),
    ];
    assert_eq!(got, want.map(|(a, b)| (a.to_string(), b.to_string())));
}

#[tokio::test]
async fn test_a_central_idea_and_its_related_items_become_one_label_and_tags_and_groups_follow() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261006_163830_labels")))
        .await
        .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('test/proj',
  '[{"key": "T", "kind": "work"}, {"key": "CID", "kind": "idea"}]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, body, state, turn, tags, group_name, opened_at, updated_at) VALUES
  (1, 'test/proj', 'CID', 1, 'Every stall glows', E'Each stall lights its own lamp.\n', 'open', 'agent', '[]', NULL, 'o', 'u'),
  (2, 'test/proj', 'T', 1, 'fold the frames', '', 'open', 'agent', '["slow"]', 'paper', 'o', 'u'),
  (3, 'test/proj', 'T', 2, 'sweep the stalls', '', 'open', 'agent', '[]', NULL, 'o', 'u');
INSERT INTO links (rid, kind, to_rid) VALUES (2, 'related', 1), (1, 'related', 2), (3, 'related', 1), (1, 'related', 3);
"#,
    )
    .await;
    migrate_before_area_required(&s).await;
    let described = pairs(
        &s,
        "SELECT name, COALESCE(description, '-') FROM labels ORDER BY name",
    )
    .await;
    let want = [
        ("goal:every-stall-glows", "Each stall lights its own lamp."),
        ("group:paper", "-"),
        ("slow", "-"),
    ];
    assert_eq!(described, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let carried = pairs(
        &s,
        "SELECT i.id, string_agg(l.name, ',' ORDER BY l.name) FROM item_labels il \
         JOIN items i ON i.rid=il.rid JOIN labels l ON l.id=il.label_id GROUP BY i.id ORDER BY i.id",
    )
    .await;
    let want = [
        ("T1", "goal:every-stall-glows,group:paper,slow"),
        ("T2", "goal:every-stall-glows"),
    ];
    assert_eq!(carried, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let dropped = pairs(
        &s,
        "SELECT id, state || ' ' || resolution FROM items WHERE key='CID'",
    )
    .await;
    assert_eq!(
        dropped,
        [(
            "CID1".to_string(),
            "dropped became label goal:every-stall-glows".to_string()
        )]
    );
}

#[tokio::test]
async fn test_every_item_left_without_an_area_goes_to_unsorted_before_the_column_is_required() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261006_163830_labels")))
        .await
        .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('test/proj',
  '[{"key": "T", "kind": "work"}, {"key": "CID", "kind": "idea"}]', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, body, state, turn, opened_at, updated_at) VALUES
  (1, 'test/proj', 'CID', 1, 'Every stall glows', '', 'open', 'agent', 'o', 'u'),
  (2, 'test/proj', 'T', 1, 'fold the frames', '', 'open', 'agent', 'o', 'u'),
  (3, 'test/proj', 'T', 2, 'sweep the stalls', '', 'open', 'agent', 'o', 'u');
UPDATE items SET state='done', turn=NULL, resolution='swept' WHERE rid=3;
"#,
    )
    .await;
    Migrator::up(&s.db, None).await.unwrap();
    let placed = pairs(
        &s,
        "SELECT i.id, a.name || ' ' || a.history::text FROM items i JOIN areas a ON a.id=i.area_id \
         ORDER BY i.id",
    )
    .await;
    let want = [
        ("CID1", "unsorted true"),
        ("T1", "unsorted true"),
        ("T2", "unsorted true"),
    ];
    assert_eq!(placed, want.map(|(a, b)| (a.to_string(), b.to_string())));
    let moved = pairs(
        &s,
        "SELECT i.id, e.note FROM events e JOIN items i ON i.rid=e.rid \
         WHERE e.host='migration' AND e.kind='edited' ORDER BY i.id",
    )
    .await;
    assert_eq!(
        moved,
        [(
            "T1".to_string(),
            "placed in unsorted: it held no area".to_string()
        )]
    );
    let nullable = pairs(
        &s,
        "SELECT column_name, is_nullable FROM information_schema.columns \
         WHERE table_name='items' AND column_name='area_id'",
    )
    .await;
    assert_eq!(nullable, [("area_id".to_string(), "NO".to_string())]);
}

#[tokio::test]
async fn test_publications_read_back_newest_first_and_a_published_sha_is_recorded_once() {
    let s = Scratch::new(2).await;
    let (old, new, work) = ("a".repeat(40), "b".repeat(40), "c".repeat(40));
    s.seed(&format!(
        r"
INSERT INTO projects (slug, created_at, updated_at) VALUES ('test/proj', 'c', 'u');
INSERT INTO areas (id, project, name, position) VALUES (1, 'test/proj', 'kiln', 0);
INSERT INTO items (rid, project, key, num, title, state, resolution, type, area_id, opened_at, updated_at) VALUES
  (1, 'test/proj', 'A', 1, 'lay the kiln', 'done', 'fired', 'plan', 1, 'o', 'u'),
  (2, 'test/proj', 'A', 2, 'glaze the bowls', 'done', 'fired', 'plan', 1, 'o', 'u');
INSERT INTO publications (id, project, published_sha, work_sha, created_at) VALUES
  (1, 'test/proj', '{old}', '{work}', '2026-01-01T00:00:00Z'),
  (2, 'test/proj', '{new}', '{work}', '2026-01-02T00:00:00Z');
INSERT INTO publication_plans (publication, rid) VALUES (1, 1), (2, 1), (2, 2);
"
    ))
    .await;
    let read = pairs(
        &s,
        "SELECT p.published_sha, string_agg(i.id, ',' ORDER BY i.id) FROM publications p \
         JOIN publication_plans pp ON pp.publication=p.id JOIN items i ON i.rid=pp.rid \
         WHERE p.project='test/proj' GROUP BY p.id ORDER BY p.created_at DESC, p.id DESC",
    )
    .await;
    assert_eq!(read, [(new.clone(), "A1,A2".into()), (old, "A1".into())]);
    let again = statement(
        "INSERT INTO publications (project, published_sha, work_sha, created_at) VALUES (?, ?, ?, ?)",
        vec![
            "test/proj".into(),
            new.into(),
            "d".repeat(40).into(),
            "2026-01-03T00:00:00Z".into(),
        ],
    );
    let err = s.db.execute_raw(again).await.unwrap_err().to_string();
    assert!(
        err.contains("publications_project_published_sha_key"),
        "{err}"
    );
    let short = statement(
        "INSERT INTO publications (project, published_sha, work_sha, created_at) VALUES (?, 'abc', ?, ?)",
        vec!["test/proj".into(), "e".repeat(40).into(), "t".into()],
    );
    assert!(s.db.execute_raw(short).await.is_err());
}

#[tokio::test]
async fn test_a_group_tags_and_a_theme_naming_no_release_or_area_become_labels() {
    let s = Scratch::bare(2).await;
    Migrator::up(&s.db, Some(steps_before("m20261006_202103_column_labels")))
        .await
        .unwrap();
    s.seed(
        r#"
INSERT INTO projects (slug, keys, created_at, updated_at) VALUES ('test/proj', '[]', 'c', 'u');
INSERT INTO releases (id, project, name, position) VALUES (1, 'test/proj', '2.0', 0);
INSERT INTO areas (id, project, name, position) VALUES (1, 'test/proj', 'kites', 0);
INSERT INTO labels (id, project, name) VALUES (1, 'test/proj', 'Wet');
INSERT INTO items (rid, project, key, num, title, body, state, turn, tags, group_name, theme, opened_at, updated_at) VALUES
  (1, 'test/proj', 'T', 1, 'fold the frames', '', 'open', 'agent', '["wet", "high"]', 'paper', 'ci', 'o', 'u'),
  (2, 'test/proj', 'T', 2, 'sweep the stalls', '', 'open', 'agent', '[]', ' ', '2.0', 'o', 'u'),
  (3, 'test/proj', 'T', 3, 'tie the tails', '', 'open', 'agent', '[]', NULL, 'Kites', 'o', 'u');
INSERT INTO item_labels (rid, label_id) VALUES (3, 1);
"#,
    )
    .await;
    migrate_before_area_required(&s).await;
    let carried = pairs(
        &s,
        "SELECT i.id, string_agg(l.name, ',' ORDER BY l.name) FROM item_labels il \
         JOIN items i ON i.rid=il.rid JOIN labels l ON l.id=il.label_id GROUP BY i.id ORDER BY i.id",
    )
    .await;
    let want = [("T1", "Wet,ci,group:paper"), ("T3", "Wet")];
    assert_eq!(carried, want.map(|(a, b)| (a.to_string(), b.to_string())));
}

#[tokio::test]
async fn test_each_open_row_comes_to_agree_with_the_claim_and_turn_and_the_columns_go_free() {
    let s = Scratch::bare(2).await;
    Migrator::up(
        &s.db,
        Some(steps_before("m20261006_210713_open_assignments")),
    )
    .await
    .unwrap();
    s.seed(
        "INSERT INTO projects (slug, created_at, updated_at) VALUES ('pottery/kiln', 'c', 'u'); \
         INSERT INTO items (rid, project, key, num, title, state, turn, turn_note, asked_at, claim_branch, \
           claim_host, claim_since, resolution, opened_at, updated_at) VALUES \
           (1, 'pottery/kiln', 'T', 1, 'Wedge the clay', 'open', 'agent', NULL, NULL, 'build/t1-1', 'wheel', 's1', NULL, 'o', 'u1'), \
           (2, 'pottery/kiln', 'T', 2, 'Trim the foot', 'open', 'agent', NULL, NULL, NULL, NULL, NULL, NULL, 'o', 'u2'), \
           (3, 'pottery/kiln', 'T', 3, 'Load the kiln', 'open', 'user', 'until it cools', 'a3', NULL, NULL, NULL, NULL, 'o', 'u3'), \
           (4, 'pottery/kiln', 'T', 4, 'Fire the bisque', 'done', NULL, NULL, NULL, NULL, NULL, NULL, 'abc1234', 'o', 'u4'), \
           (5, 'pottery/kiln', 'Q', 5, 'Which glaze', 'open', 'user', 'pick a glaze', 'a5', NULL, NULL, NULL, NULL, 'o', 'u5'), \
           (6, 'pottery/kiln', 'T', 6, 'Sweep the floor', 'open', 'agent', NULL, NULL, 'build/t6-2', 'wheel', 's6', NULL, 'o', 'u6'); \
         INSERT INTO assignments (rid, assignee, kind, started_at, branch, host, note) VALUES \
           (1, 'agent', 'claim', 's1', 'build/t1-1', 'wheel', NULL), \
           (2, 'owner', 'ask', 'a2', NULL, '', 'which way'), \
           (4, 'agent', 'claim', 's4', 'build/t4-1', 'wheel', NULL), \
           (5, 'owner', 'ask', 'a5', NULL, '', NULL), \
           (6, 'agent', 'claim', 's6old', 'build/t6-1', 'wheel', NULL)",
    )
    .await;
    Migrator::up(&s.db, Some(1)).await.unwrap();
    let rows = s
        .db
        .query_all_raw(statement(
            "SELECT rid, kind, started_at, COALESCE(ended_at, '-'), COALESCE(branch, '-'), COALESCE(note, '-') \
             FROM assignments ORDER BY rid, id",
            vec![],
        ))
        .await
        .unwrap();
    let got: Vec<(i64, String, String, String, String, String)> = rows
        .iter()
        .map(|r| {
            (
                r.try_get_by_index(0).unwrap(),
                r.try_get_by_index(1).unwrap(),
                r.try_get_by_index(2).unwrap(),
                r.try_get_by_index(3).unwrap(),
                r.try_get_by_index(4).unwrap(),
                r.try_get_by_index(5).unwrap(),
            )
        })
        .collect();
    let row = |rid, kind: &str, from: &str, to: &str, branch: &str, note: &str| {
        (
            rid,
            kind.to_string(),
            from.to_string(),
            to.to_string(),
            branch.to_string(),
            note.to_string(),
        )
    };
    assert_eq!(
        got,
        [
            row(1, "claim", "s1", "-", "build/t1-1", "-"),
            row(2, "ask", "a2", "u2", "-", "which way"),
            row(3, "ask", "a3", "-", "-", "until it cools"),
            row(4, "claim", "s4", "u4", "build/t4-1", "-"),
            row(5, "ask", "a5", "-", "-", "pick a glaze"),
            row(6, "claim", "s6old", "u6", "build/t6-1", "-"),
            row(6, "claim", "s6", "-", "build/t6-2", "-"),
        ]
    );
    s.seed(
        "UPDATE items SET turn=NULL, turn_note=NULL, asked_at=NULL, claim_branch='build/t2-9', \
         claim_host=NULL, claim_since=NULL WHERE project='pottery/kiln'",
    )
    .await;
}

/// The columns a table holds now.
async fn columns(s: &Scratch, table: &str) -> Vec<String> {
    s.db.query_all_raw(statement(
        "SELECT column_name::text FROM information_schema.columns \
         WHERE table_schema='public' AND table_name=? ORDER BY column_name",
        vec![table.into()],
    ))
    .await
    .unwrap()
    .iter()
    .map(|r| r.try_get_by_index(0).unwrap())
    .collect()
}

#[tokio::test]
async fn test_the_drop_leaves_items_and_projects_without_the_columns_the_core_replaced() {
    let s = Scratch::new(2).await;
    let items = columns(&s, "items").await;
    for gone in [
        "turn",
        "turn_note",
        "asked_at",
        "claim_branch",
        "claim_host",
        "claim_since",
        "claim_runner",
        "claim_job",
        "claim_on",
        "wait_on",
        "wait_item",
        "wait_ref",
        "wait_since",
        "scope",
        "group_name",
        "theme",
        "rank",
        "tags",
        "conflict",
    ] {
        assert!(!items.contains(&gone.to_string()), "items still has {gone}");
    }
    for kept in [
        "state",
        "release_id",
        "area_id",
        "parent_rid",
        "priority",
        "type",
    ] {
        assert!(items.contains(&kept.to_string()), "items lost {kept}");
    }
    let index: Option<String> =
        s.db.query_one_raw(statement("SELECT to_regclass('items_wait')::text", vec![]))
            .await
            .unwrap()
            .unwrap()
            .try_get_by_index(0)
            .unwrap();
    assert_eq!(index, None);
    let projects = columns(&s, "projects").await;
    assert!(!projects.contains(&"keys".to_string()));
    assert!(!projects.contains(&"themes".to_string()));
    s.seed(
        "INSERT INTO projects (slug, created_at, updated_at) VALUES ('bakery/oven', 'c', 'u'); \
         INSERT INTO areas (id, project, name, position) VALUES (1, 'bakery/oven', 'crust', 0); \
         INSERT INTO items (project, key, num, title, state, area_id, opened_at, updated_at) \
           VALUES ('bakery/oven', 'T', 1, 'Proof the dough', 'open', 1, 'o', 'u')",
    )
    .await;
}

#[tokio::test]
async fn test_the_drop_refuses_while_an_item_holds_a_sync_conflict_and_names_it() {
    let s = Scratch::bare(2).await;
    Migrator::up(
        &s.db,
        Some(steps_before("m20261006_213453_drop_old_columns")),
    )
    .await
    .unwrap();
    s.seed(
        "INSERT INTO projects (slug, created_at, updated_at) VALUES ('bakery/oven', 'c', 'u'); \
         INSERT INTO items (rid, project, key, num, title, state, turn, conflict, opened_at, updated_at) VALUES \
           (1, 'bakery/oven', 'T', 1, 'Proof the dough', 'open', 'agent', 0, 'o', 'u'), \
           (2, 'bakery/oven', 'T', 2, 'Score the loaf', 'open', 'agent', 1, 'o', 'u')",
    )
    .await;
    let refused = migrate(&s.db).await.unwrap_err().to_string();
    assert!(refused.contains("bakery/oven T2"), "{refused}");
    assert!(!refused.contains("T1"), "{refused}");
    assert!(columns(&s, "items").await.contains(&"conflict".to_string()));
}

#[tokio::test]
async fn test_the_area_column_refuses_an_insert_with_none() {
    let s = Scratch::bare(2).await;
    Migrator::up(
        &s.db,
        Some(steps_before("m20261006_233349_items_area_not_null")),
    )
    .await
    .unwrap();
    s.seed(
        "INSERT INTO projects (slug, created_at, updated_at) VALUES ('bakery/oven', 'c', 'u'); \
         INSERT INTO areas (id, project, name, position) VALUES (1, 'bakery/oven', 'crust', 0); \
         INSERT INTO items (rid, project, key, num, title, state, area_id, opened_at, updated_at) VALUES \
           (1, 'bakery/oven', 'T', 1, 'Proof the dough', 'open', 1, 'o', 'u')",
    )
    .await;
    migrate(&s.db).await.unwrap();
    let none =
        s.db.execute_unprepared(
            "INSERT INTO items (project, key, num, title, state, opened_at, updated_at) \
             VALUES ('bakery/oven', 'T', 3, 'Glaze the bun', 'open', 'o', 'u')",
        )
        .await;
    assert!(none.is_err());
}

#[tokio::test]
async fn test_the_closes_of_a_project_read_from_an_index_on_its_events_by_kind() {
    let s = Scratch::bare(2).await;
    migrate(&s.db).await.unwrap();
    let index = s
        .db
        .query_one_raw(statement(
            "SELECT indexdef FROM pg_indexes WHERE tablename='events' AND indexname='events_project_kind'",
            vec![],
        ))
        .await
        .unwrap()
        .map(|r| r.try_get_by_index::<String>(0).unwrap());
    assert!(
        index
            .as_deref()
            .is_some_and(|d| d.ends_with("(project, kind, rid, at)")),
        "{index:?}"
    );
}
