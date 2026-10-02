//! The docket schema as migrations, applied by the server before it listens, and the statements every
//! raw query is sent as.

mod m20261001_000001_schema;
mod m20261002_000001_machines_and_leads;
#[cfg(any(test, feature = "scratch"))]
pub mod scratch;

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement, TransactionTrait, Value,
};
use sea_orm_migration::{MigrationTrait, MigratorTrait};

/// The identity columns, by table. Rows written with their own keys leave each to be moved past them.
pub const IDENTITIES: [(&str, &str); 3] = [("items", "rid"), ("events", "seq"), ("links", "id")];

/// The advisory lock held while migrating, so two servers starting together migrate one at a time.
const MIGRATING: i64 = 0x646f_636b_6574;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261001_000001_schema::Migration),
            Box::new(m20261002_000001_machines_and_leads::Migration),
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
///
/// # Errors
/// The database refuses.
pub async fn reset_identities<C: ConnectionTrait>(c: &C) -> Result<(), DbErr> {
    for (table, column) in IDENTITIES {
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
