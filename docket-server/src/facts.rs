//! A project's facts: `GET /facts` reads them, `POST /do/fact` sets or unsets one under the rules of
//! `docket_core::fact`. Every statement on them is here.

use std::collections::BTreeMap;

use axum::extract::{Extension, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde::Deserialize;
use serde_json::Value;

use docket_core::api::{FactRequest, FactSet, Facts};
use docket_core::fact;
use docket_core::rules::COMPLEXITIES;
use docket_core::word::PRIORITIES;

use crate::auth::Caller;
use crate::store::{Tx, json, scalar, sql};
use crate::verbs::Failure;

const SKILLS: &str = "SELECT skills FROM projects WHERE slug=?";
const WRITE: &str = "UPDATE projects SET skills=?, updated_at=? WHERE slug=?";
const OWNER: &str = "SELECT key, value FROM owner_facts";
const OWNER_SET: &str = "INSERT INTO owner_facts (key, value, updated_at) VALUES (?, ?, ?) \
                         ON CONFLICT (key) DO UPDATE SET value=EXCLUDED.value, updated_at=EXCLUDED.updated_at";
const OWNER_UNSET: &str = "DELETE FROM owner_facts WHERE key=?";

pub fn router() -> Router<DatabaseConnection> {
    Router::new()
        .route("/facts", get(read))
        .route("/do/fact", post(set))
}

#[derive(Deserialize)]
pub struct InProject {
    pub project: String,
}

/// The facts a project sets, each a string, read from the stored JSON object.
pub async fn stored<C: ConnectionTrait>(
    c: &C,
    slug: &str,
) -> Result<BTreeMap<String, String>, Failure> {
    let skills: Option<Value> = scalar(c, SKILLS, vec![slug.into()]).await?;
    let Some(skills) = skills else {
        return Err(Failure::NotFound(format!("no project {slug}")));
    };
    Ok(parse(skills))
}

/// The owner-level agent settings, held once for every project.
pub async fn owner<C: ConnectionTrait>(c: &C) -> Result<BTreeMap<String, String>, Failure> {
    let rows = c.query_all_raw(sql(OWNER, vec![])).await?;
    let mut out = BTreeMap::new();
    for row in rows {
        out.insert(
            row.try_get_by_index::<String>(0)?,
            row.try_get_by_index::<String>(1)?,
        );
    }
    Ok(out)
}

/// Every string-valued fact of a stored skills object; anything else is left out.
fn parse(skills: Value) -> BTreeMap<String, String> {
    let Value::Object(map) = skills else {
        return BTreeMap::new();
    };
    map.into_iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
        .collect()
}

/// `docket skills`: the facts a project sets that docket reads. A retired fact stays stored and is left
/// out.
///
/// # Errors
/// 404 for an unknown project.
pub async fn read(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<Facts>, Failure> {
    let skills = fact::known(&stored(&db, &q.project).await?);
    Ok(Json(Facts {
        project: q.project,
        skills,
        owner: owner(&db).await?,
        priorities: PRIORITIES.map(String::from).to_vec(),
        levels: COMPLEXITIES.map(String::from).to_vec(),
    }))
}

/// `docket skills set KEY "value"`: one fact set, or unset by an empty value; with `all_projects`, the
/// owner-level value of an agent setting.
///
/// # Errors
/// 409 for a value the fact does not hold or a retired fact, 404 for an unknown project.
pub async fn set(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<FactRequest>,
) -> Result<Json<FactSet>, Failure> {
    let (slug, key, value) = (
        req.common.project.clone(),
        req.key.clone(),
        req.value.clone(),
    );
    let mut tx = Tx::begin(&db, &caller.host).await?;
    let skills = stored(&tx.conn, &slug).await?;
    if req.all_projects {
        fact::check_owner(&key, &value)?;
        if value.is_empty() {
            tx.execute(OWNER_UNSET, vec![key.clone().into()]).await?;
        } else {
            tx.execute(
                OWNER_SET,
                vec![key.clone().into(), value.into(), tx.now.clone().into()],
            )
            .await?;
        }
        tx.touch_project(&slug);
        let owner = owner(&tx.conn).await?;
        tx.commit().await?;
        return Ok(Json(FactSet {
            project: slug,
            key,
            skills: fact::known(&skills),
            owner,
        }));
    }
    fact::check(&key, &value)?;
    let skills = fact::with(&skills, &key, &value);
    let stored = json(serde_json::to_value(&skills).unwrap_or_default());
    tx.execute(
        WRITE,
        vec![stored, tx.now.clone().into(), slug.clone().into()],
    )
    .await?;
    tx.touch_project(&slug);
    tx.commit().await?;
    Ok(Json(FactSet {
        project: slug,
        key,
        skills: fact::known(&skills),
        owner: owner(&db).await?,
    }))
}

#[cfg(test)]
#[path = "tests/facts.rs"]
mod tests;
