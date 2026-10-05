//! Rows read by a raw query, shaped as `--json` prints them, with any extra columns the query selects.

use std::collections::{HashMap, HashSet};

use sea_orm::{ConnectionTrait, DbErr, FromQueryResult};
use serde_json::{Map, Value};

use crate::entities::{item, project};
use crate::reads::public::{Members, member_counts, public, sql};
use crate::verbs::Failure;
use docket_core::word::kind_of_type;

/// A stored row and the extra columns its query selected.
pub type Extra = (item::Model, Map<String, Value>);

/// Item rows for a raw query; `extras` names the selected columns beyond the item's own.
///
/// # Errors
/// The database.
pub async fn rows_with<C: ConnectionTrait>(
    db: &C,
    text: &str,
    values: Vec<sea_orm::Value>,
    extras: &[&str],
) -> Result<Vec<Extra>, DbErr> {
    let rows = db.query_all_raw(sql(text, values)).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let model = item::Model::from_query_result(&row, "")?;
        let mut extra = Map::new();
        for name in extras {
            extra.insert((*name).to_string(), column(&row, name));
        }
        out.push((model, extra));
    }
    Ok(out)
}

/// Item rows for a raw query with no extra columns.
///
/// # Errors
/// The database.
pub async fn rows<C: ConnectionTrait>(
    db: &C,
    text: &str,
    values: Vec<sea_orm::Value>,
) -> Result<Vec<item::Model>, DbErr> {
    Ok(rows_with(db, text, values, &[])
        .await?
        .into_iter()
        .map(|(m, _)| m)
        .collect())
}

/// One selected column as JSON, whatever SQLite type it holds.
fn column(row: &sea_orm::QueryResult, name: &str) -> Value {
    if let Ok(Some(n)) = row.try_get::<Option<i64>>("", name) {
        return Value::from(n);
    }
    if let Ok(Some(f)) = row.try_get::<Option<f64>>("", name) {
        return Value::from(f);
    }
    match row.try_get::<Option<String>>("", name) {
        Ok(Some(s)) => Value::from(s),
        _ => Value::Null,
    }
}

/// The rows in the `--json` shape, each with its extra columns, in their given order. A row of another
/// project than the one asked about is read by its own project's keys.
///
/// # Errors
/// The database.
pub async fn shaped<C: ConnectionTrait>(
    db: &C,
    project: &project::Model,
    rows: Vec<Extra>,
) -> Result<Vec<Value>, Failure> {
    let mut counts: HashMap<i64, Members> = HashMap::new();
    counts.extend(member_counts(db, &project.slug).await?);
    let mut counted = HashSet::from([project.slug.clone()]);
    for (row, _) in &rows {
        if counted.insert(row.project.clone()) {
            counts.extend(member_counts(db, &row.project).await?);
        }
    }
    let mut out = Vec::with_capacity(rows.len());
    for (row, extra) in rows {
        let open = counts.get(&row.rid).copied().unwrap_or_default();
        let kind = kind_of_type(&row.item_type);
        let mut d = public(db, kind, row, open, None).await?;
        d.extend(extra);
        out.push(Value::Object(d));
    }
    Ok(out)
}

/// A project's stored row.
///
/// # Errors
/// 404 when the slug is unknown.
pub async fn project_model<C: ConnectionTrait>(
    db: &C,
    slug: &str,
) -> Result<project::Model, Failure> {
    use sea_orm::EntityTrait;
    project::Entity::find_by_id(slug)
        .one(db)
        .await?
        .ok_or_else(|| Failure::NotFound(format!("no project {slug}")))
}

/// `?,?,?` for n values, `NULL` when there are none.
#[must_use]
pub fn marks(n: usize) -> String {
    crate::reads::public::marks(n)
}
