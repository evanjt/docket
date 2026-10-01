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
use docket_core::clock;
use docket_core::fact;

use crate::auth::Caller;
use crate::store::{Tx, json, scalar};
use crate::verbs::Failure;

const SKILLS: &str = "SELECT skills FROM projects WHERE slug=?";
const WRITE: &str = "UPDATE projects SET skills=?, updated_at=? WHERE slug=?";
const TICK: &str = "SELECT v FROM meta WHERE k=?";

pub fn router() -> Router<DatabaseConnection> {
    Router::new()
        .route("/facts", get(read))
        .route("/do/fact", post(set))
}

#[derive(Deserialize)]
pub struct InProject {
    project: String,
}

/// The facts a project sets, each a string, read from the stored JSON object.
async fn stored<C: ConnectionTrait>(
    c: &C,
    slug: &str,
) -> Result<BTreeMap<String, String>, Failure> {
    let skills: Option<Value> = scalar(c, SKILLS, vec![slug.into()]).await?;
    let Some(skills) = skills else {
        return Err(Failure::NotFound(format!("no project {slug}")));
    };
    Ok(parse(skills))
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

/// When the loop last ticked for the project, from the epoch seconds it records in `meta`.
async fn last_tick<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Option<String>, Failure> {
    let raw: Option<String> = scalar(c, TICK, vec![format!("last_tick {slug}").into()]).await?;
    Ok(raw
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|t| t.is_finite() && *t >= 0.0)
        .map(|t| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            clock::stamp(t as u64)
        }))
}

/// `docket skills`: the facts a project sets, and the loop's last tick.
///
/// # Errors
/// 404 for an unknown project.
pub async fn read(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<Facts>, Failure> {
    let skills = stored(&db, &q.project).await?;
    let last_tick = last_tick(&db, &q.project).await?;
    Ok(Json(Facts {
        project: q.project,
        skills,
        last_tick,
    }))
}

/// `docket skills set KEY "value"`: one fact set, or unset by an empty value.
///
/// # Errors
/// 409 for a value the fact does not hold or a pool above its ceiling, 403 for a fact the owner keeps
/// set on an agent's key, 404 for an unknown project.
pub async fn set(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<FactRequest>,
) -> Result<Json<FactSet>, Failure> {
    let (slug, key, value) = (req.common.project, req.key, req.value);
    let mut tx = Tx::begin(&db, &caller.host).await?;
    let skills = stored(&tx.conn, &slug).await?;
    fact::check(&key, &value)?;
    if let Some(why) = fact::owner_only(&key).filter(|_| !caller.owner) {
        return Err(Failure::Forbidden(why));
    }
    fact::ceiling(&skills, &key, &value)?;
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
        skills,
    }))
}

#[cfg(test)]
#[path = "tests/facts.rs"]
mod tests;
