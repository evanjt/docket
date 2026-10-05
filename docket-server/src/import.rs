//! A SQLite docket database copied into an empty Postgres one: every row kept with its rid, event
//! seq and link id, the search and assignment rows rebuilt, and the counts compared per project and
//! table before the copy commits.

use std::collections::{BTreeMap, BTreeSet};

use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DatabaseTransaction, DbBackend,
    QueryResult, Statement, TransactionTrait, Value,
};

use crate::store::{json, sql};

/// How a column is read from SQLite and bound for Postgres.
#[derive(Clone, Copy)]
enum Kind {
    Int,
    Real,
    Text,
    Json,
}

use Kind::{Int, Json, Real, Text};

/// A table copied whole: the columns written, as SQLite names them, and the order rows are read in.
struct Table {
    name: &'static str,
    columns: &'static [(&'static str, Kind)],
    order: &'static str,
}

/// The tables in the order their references allow.
const TABLES: [Table; 8] = [
    Table {
        name: "projects",
        columns: &[
            ("slug", Text),
            ("keys", Json),
            ("remotes", Json),
            ("themes", Json),
            ("cite_roots", Json),
            ("repos", Json),
            ("fleet_repo", Text),
            ("integration_ref", Text),
            ("worktree_hint", Text),
            ("test_hint", Text),
            ("skills", Json),
            ("created_at", Text),
            ("updated_at", Text),
        ],
        order: "slug",
    },
    Table {
        name: "roots",
        columns: &[
            ("host", Text),
            ("path", Text),
            ("project", Text),
            ("bound_at", Text),
            ("how", Text),
        ],
        order: "host, path",
    },
    Table {
        name: "items",
        columns: &[
            ("rid", Int),
            ("project", Text),
            ("key", Text),
            ("num", Int),
            ("title", Text),
            ("state", Text),
            ("turn", Text),
            ("turn_note", Text),
            ("asked_at", Text),
            ("claim_branch", Text),
            ("claim_host", Text),
            ("claim_since", Text),
            ("claim_runner", Text),
            ("claim_job", Text),
            ("claim_on", Text),
            ("wait_on", Text),
            ("wait_item", Int),
            ("wait_ref", Text),
            ("wait_since", Text),
            ("decision", Text),
            ("decided_at", Text),
            ("resolution", Text),
            ("superseded_by", Int),
            ("scope", Text),
            ("complexity", Text),
            ("group_name", Text),
            ("theme", Text),
            ("rank", Int),
            ("tags", Json),
            ("body", Text),
            ("conflict", Int),
            ("opened_at", Text),
            ("updated_at", Text),
        ],
        order: "rid",
    },
    Table {
        name: "events",
        columns: &[
            ("seq", Int),
            ("uid", Text),
            ("project", Text),
            ("rid", Int),
            ("at", Text),
            ("host", Text),
            ("branch", Text),
            ("kind", Text),
            ("note", Text),
            ("data", Json),
        ],
        order: "seq",
    },
    Table {
        name: "links",
        columns: &[
            ("rowid", Int),
            ("rid", Int),
            ("kind", Text),
            ("to_rid", Int),
            ("to_path", Text),
            ("to_line", Int),
        ],
        order: "rowid",
    },
    Table {
        name: "pending_dump",
        columns: &[("rid", Int), ("project", Text)],
        order: "rid",
    },
    Table {
        name: "chores",
        columns: &[
            ("name", Text),
            ("project", Text),
            ("at", Real),
            ("seconds", Real),
            ("result", Text),
            ("failures", Int),
            ("ok", Int),
        ],
        order: "name, project",
    },
    Table {
        name: "meta",
        columns: &[("k", Text), ("v", Text)],
        order: "k",
    },
];

/// Each table's rows by project, a query both databases answer alike.
const COUNTS: [(&str, &str); 8] = [
    (
        "projects",
        "SELECT slug, COUNT(*) FROM projects GROUP BY slug",
    ),
    (
        "roots",
        "SELECT project, COUNT(*) FROM roots GROUP BY project",
    ),
    (
        "items",
        "SELECT project, COUNT(*) FROM items GROUP BY project",
    ),
    (
        "events",
        "SELECT project, COUNT(*) FROM events GROUP BY project",
    ),
    (
        "links",
        "SELECT i.project, COUNT(*) FROM links l JOIN items i ON i.rid = l.rid GROUP BY i.project",
    ),
    (
        "pending_dump",
        "SELECT project, COUNT(*) FROM pending_dump GROUP BY project",
    ),
    (
        "chores",
        "SELECT project, COUNT(*) FROM chores GROUP BY project",
    ),
    ("meta", "SELECT '', COUNT(*) FROM meta"),
];

/// The search rows of every item, as a write builds them: its text and the paths it cites.
const SEARCH: &str = "INSERT INTO search (rid, id, title, body, files) \
     SELECT i.rid, i.id, i.title, i.body, COALESCE((SELECT string_agg(DISTINCT l.to_path, ' ' \
     ORDER BY l.to_path) FROM links l WHERE l.rid = i.rid AND l.kind IN ('cites_file', 'cites_test')), '') \
     FROM items i";

/// Each open item's wait on another, as the dependency it is.
const DEPENDS: &str = "INSERT INTO dependencies (rid, on_rid, created_at) \
     SELECT rid, wait_item, wait_since FROM items \
     WHERE state = 'open' AND wait_on = 'item' AND wait_item IS NOT NULL";

/// The bind values Postgres takes at most in one statement.
const MAX_VALUES: usize = 65_535;

/// `(table, project)` and the rows it holds there.
pub type Counts = BTreeMap<(String, String), i64>;

/// Every row of the SQLite database at `from` written into `to`, which must hold no rows yet. The
/// counts are compared before the copy commits, and returned.
///
/// # Errors
/// The file cannot be read as SQLite, `to` already holds rows, a stored JSON value does not parse,
/// Postgres refuses a row, or a count differs.
pub async fn import(from: &str, to: &DatabaseConnection) -> Result<Counts, String> {
    let source = open(from).await?;
    let tx = to.begin().await.map_err(|e| e.to_string())?;
    refuse_rows(&tx).await?;
    tx.execute_unprepared("SET CONSTRAINTS ALL DEFERRED")
        .await
        .map_err(|e| e.to_string())?;
    for table in &TABLES {
        copy(&source, &tx, table).await?;
    }
    tx.execute_unprepared(SEARCH)
        .await
        .map_err(|e| e.to_string())?;
    tx.execute_unprepared(DEPENDS)
        .await
        .map_err(|e| e.to_string())?;
    docket_migration::reset_identities(&tx)
        .await
        .map_err(|e| e.to_string())?;
    docket_migration::assignments::rebuild_all(&tx)
        .await
        .map_err(|e| e.to_string())?;
    let had = counts(&source).await?;
    let has = counts(&tx).await?;
    compare(&had, &has)?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(has)
}

/// The SQLite file opened for reading only, never locked or written.
async fn open(path: &str) -> Result<DatabaseConnection, String> {
    let mut options = ConnectOptions::new(format!("sqlite://{path}?mode=ro&immutable=true"));
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options)
        .await
        .map_err(|e| format!("{path}: {e}"))?;
    if db.get_database_backend() != DbBackend::Sqlite {
        return Err(format!("{path} is not a SQLite file"));
    }
    Ok(db)
}

async fn refuse_rows(tx: &DatabaseTransaction) -> Result<(), String> {
    let held = counts(tx).await?;
    let n: i64 = held.values().sum();
    if n > 0 {
        return Err(format!(
            "the database already holds {n} rows; an import only fills an empty one"
        ));
    }
    Ok(())
}

/// One table's rows read from SQLite and written to Postgres in batches.
async fn copy(
    source: &DatabaseConnection,
    tx: &DatabaseTransaction,
    table: &Table,
) -> Result<(), String> {
    let names: Vec<&str> = table.columns.iter().map(|(n, _)| *n).collect();
    let read = format!(
        "SELECT {} FROM {} ORDER BY {}",
        names.join(", "),
        table.name,
        table.order
    );
    let rows = source
        .query_all_raw(Statement::from_string(DbBackend::Sqlite, read))
        .await
        .map_err(|e| format!("{}: {e}", table.name))?;
    let width = names.len();
    let per = (MAX_VALUES / width).min(1000);
    for batch in rows.chunks(per) {
        let mut values = Vec::with_capacity(batch.len() * width);
        for row in batch {
            values.extend(row_values(table, row)?);
        }
        let marks = format!("({})", vec!["?"; width].join(", "));
        let text = format!(
            "INSERT INTO {} ({}) VALUES {}",
            table.name,
            names.join(", ").replace("rowid", "id"),
            vec![marks; batch.len()].join(", ")
        );
        tx.execute_raw(sql(&text, values))
            .await
            .map_err(|e| format!("{}: {e}", table.name))?;
    }
    Ok(())
}

/// One row's values, each read as its column holds it and bound as Postgres takes it.
fn row_values(table: &Table, row: &QueryResult) -> Result<Vec<Value>, String> {
    let mut out = Vec::with_capacity(table.columns.len());
    for (i, (name, kind)) in table.columns.iter().enumerate() {
        let at = |e: sea_orm::DbErr| format!("{}.{name}: {e}", table.name);
        let value = match kind {
            Int => row.try_get_by_index::<Option<i64>>(i).map_err(at)?.into(),
            Real => row.try_get_by_index::<Option<f64>>(i).map_err(at)?.into(),
            Text => row
                .try_get_by_index::<Option<String>>(i)
                .map_err(at)?
                .into(),
            Json => match row.try_get_by_index::<Option<String>>(i).map_err(at)? {
                Some(text) => json(
                    serde_json::from_str(&text)
                        .map_err(|e| format!("{}.{name}: {e} in {text:?}", table.name))?,
                ),
                None => Value::Json(None),
            },
        };
        out.push(value);
    }
    Ok(out)
}

/// Every table's rows by project.
async fn counts<C: ConnectionTrait>(c: &C) -> Result<Counts, String> {
    let backend = c.get_database_backend();
    let mut out = Counts::new();
    for (table, text) in COUNTS {
        let rows = c
            .query_all_raw(Statement::from_string(backend, text))
            .await
            .map_err(|e| format!("{table}: {e}"))?;
        for row in rows {
            let project: String = row.try_get_by_index(0).map_err(|e| e.to_string())?;
            let n: i64 = row.try_get_by_index(1).map_err(|e| e.to_string())?;
            if n > 0 {
                out.insert((table.to_string(), project), n);
            }
        }
    }
    Ok(out)
}

/// Refuses a copy whose counts differ from its source's, naming each that does.
fn compare(had: &Counts, has: &Counts) -> Result<(), String> {
    let keys: BTreeSet<&(String, String)> = had.keys().chain(has.keys()).collect();
    let differ: Vec<String> = keys
        .into_iter()
        .filter(|k| had.get(*k) != has.get(*k))
        .map(|(table, project)| {
            let k = (table.clone(), project.clone());
            format!(
                "{table} of {project:?}: {} in SQLite, {} in Postgres",
                had.get(&k).unwrap_or(&0),
                has.get(&k).unwrap_or(&0)
            )
        })
        .collect();
    if differ.is_empty() {
        return Ok(());
    }
    Err(format!("the counts differ: {}", differ.join("; ")))
}

#[cfg(test)]
#[path = "tests/import.rs"]
mod tests;
