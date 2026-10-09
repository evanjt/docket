//! The docket schema as migrations, applied by the server before it listens, and the statements every
//! raw query is sent as.

pub mod assignments;
pub mod item_types;
mod m20261001_000001_schema;
mod m20261002_000001_machines_and_leads;
mod m20261005_000001_owner_facts;
mod m20261005_000002_runner_limits;
mod m20261005_000003_machine_path;
mod m20261005_000004_assignments;
mod m20261006_000001_dependencies;
mod m20261006_000002_condition_waits;
mod m20261006_000003_ask_need;
mod m20261006_000004_job_reports;
mod m20261006_000005_releases;
mod m20261006_000006_parents;
mod m20261006_000007_assignee;
mod m20261006_000008_areas;
mod m20261006_000009_job_times;
mod m20261006_000010_plan_gates;
pub(crate) mod m20261006_000011_area_places;
mod m20261006_162014_item_types;
mod m20261006_163830_labels;
mod m20261006_164713_area_placements;
mod m20261006_174229_publications;
mod m20261006_202103_column_labels;
mod m20261006_210713_open_assignments;
mod m20261006_213453_drop_old_columns;
mod m20261006_220000_remapped_events;
mod m20261006_233349_items_area_not_null;
mod m20261006_235900_outcome_ended;
mod m20261007_073521_events_close_index;
mod m20261009_073000_project_rename;
pub mod parents;
#[cfg(any(test, feature = "scratch"))]
pub mod scratch;

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement, TransactionTrait, Value,
};
use sea_orm_migration::{MigrationTrait, MigratorTrait};

/// The identity columns, by table. Rows written with their own keys leave each to be moved past them.
pub const IDENTITIES: [(&str, &str); 7] = [
    ("items", "rid"),
    ("events", "seq"),
    ("links", "id"),
    ("releases", "id"),
    ("areas", "id"),
    ("labels", "id"),
    ("publications", "id"),
];

/// The advisory lock held while migrating, so two servers starting together migrate one at a time.
const MIGRATING: i64 = 0x646f_636b_6574;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261001_000001_schema::Migration),
            Box::new(m20261002_000001_machines_and_leads::Migration),
            Box::new(m20261005_000001_owner_facts::Migration),
            Box::new(m20261005_000002_runner_limits::Migration),
            Box::new(m20261005_000003_machine_path::Migration),
            Box::new(m20261005_000004_assignments::Migration),
            Box::new(m20261006_000001_dependencies::Migration),
            Box::new(m20261006_000002_condition_waits::Migration),
            Box::new(m20261006_000003_ask_need::Migration),
            Box::new(m20261006_000004_job_reports::Migration),
            Box::new(m20261006_000005_releases::Migration),
            Box::new(m20261006_000006_parents::Migration),
            Box::new(m20261006_000007_assignee::Migration),
            Box::new(m20261006_000008_areas::Migration),
            Box::new(m20261006_000009_job_times::Migration),
            Box::new(m20261006_000010_plan_gates::Migration),
            Box::new(m20261006_000011_area_places::Migration),
            Box::new(m20261006_162014_item_types::Migration),
            Box::new(m20261006_163830_labels::Migration),
            Box::new(m20261006_164713_area_placements::Migration),
            Box::new(m20261006_174229_publications::Migration),
            Box::new(m20261006_202103_column_labels::Migration),
            Box::new(m20261006_210713_open_assignments::Migration),
            Box::new(m20261006_213453_drop_old_columns::Migration),
            Box::new(m20261006_220000_remapped_events::Migration),
            Box::new(m20261006_233349_items_area_not_null::Migration),
            Box::new(m20261006_235900_outcome_ended::Migration),
            Box::new(m20261007_073521_events_close_index::Migration),
            Box::new(m20261009_073000_project_rename::Migration),
        ]
    }
}

/// Applies every migration not yet applied, in one transaction, and names those it applied.
///
/// # Errors
/// The database refuses a migration; none of them is kept.
pub async fn migrate(db: &DatabaseConnection) -> Result<Vec<String>, DbErr> {
    let tx = db.begin().await?;
    tx.execute_unprepared(&format!("SELECT pg_advisory_xact_lock({MIGRATING})"))
        .await?;
    let pending: Vec<String> = Migrator::get_pending_migrations(&tx)
        .await?
        .iter()
        .map(|m| m.name().to_string())
        .collect();
    Migrator::up(&tx, None).await?;
    tx.commit().await?;
    Ok(pending)
}

/// Moves each identity past the highest key its table holds, so the next row written gets a new one.
/// A table a partial migration has not made yet is left alone.
///
/// # Errors
/// The database refuses.
pub async fn reset_identities<C: ConnectionTrait>(c: &C) -> Result<(), DbErr> {
    for (table, column) in IDENTITIES {
        let made = c
            .query_one_raw(statement(
                "SELECT to_regclass(?) IS NOT NULL",
                vec![table.into()],
            ))
            .await?
            .map(|r| r.try_get_by_index::<bool>(0))
            .transpose()?
            .unwrap_or(false);
        if !made {
            continue;
        }
        c.execute_unprepared(&format!(
            "SELECT setval(pg_get_serial_sequence('{table}', '{column}'), \
             COALESCE((SELECT MAX({column}) FROM {table}), 0) + 1, false)"
        ))
        .await?;
    }
    Ok(())
}

/// The URL as sea-orm reads a Postgres one: `postgresql://`, the scheme a cluster operator's secret
/// writes, becomes `postgres://`.
#[must_use]
pub fn postgres_url(url: &str) -> String {
    match url.strip_prefix("postgresql://") {
        Some(rest) => format!("postgres://{rest}"),
        None => url.to_string(),
    }
}

/// A Postgres statement from text written with `?` for each value, numbered `$1`, `$2` in order.
#[must_use]
pub fn statement(text: &str, values: Vec<Value>) -> Statement {
    Statement::from_sql_and_values(DbBackend::Postgres, numbered(text), values)
}

/// Each `?` outside a quoted literal or identifier as `$n`, counting from one.
#[must_use]
pub fn numbered(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut quote: Option<char> = None;
    let mut n = 0;
    for c in text.chars() {
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), _) if q == c => quote = None,
            (None, '?') => {
                n += 1;
                out.push('$');
                out.push_str(&n.to_string());
                continue;
            }
            _ => {}
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
