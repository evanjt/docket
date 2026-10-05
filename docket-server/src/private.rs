//! `GET /private`: the data that names the owner's private work, from which a client finds what a
//! public repository must not carry. Only the owner's key reads it.

use axum::extract::{Extension, State};
use axum::routing::get;
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DatabaseConnection};

use docket_core::private::Private;
use docket_core::word::ItemType;

use crate::auth::Caller;
use crate::store::{column, sql};
use crate::verbs::{Failure, require_owner};

const PROJECTS: &str = "SELECT slug FROM projects ORDER BY slug";
const OWNERS: &str = "SELECT DISTINCT skills->>'owner' FROM projects \
                      WHERE skills->>'owner' IS NOT NULL AND skills->>'owner' <> '' ORDER BY 1";
const HOSTS: &str = "SELECT host FROM events UNION SELECT claim_host FROM items \
                     WHERE claim_host IS NOT NULL UNION SELECT host FROM leads ORDER BY 1";
const TITLES: &str = "SELECT DISTINCT title FROM items WHERE length(title) >= $1 ORDER BY 1";
const KEYS: &str = "SELECT DISTINCT key FROM items ORDER BY 1";

pub fn router() -> Router<DatabaseConnection> {
    Router::new().route("/private", get(read))
}

/// # Errors
/// 403 on an agent's key.
pub async fn read(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
) -> Result<Json<Private>, Failure> {
    require_owner(&caller, "private")?;
    let machines = db
        .query_all_raw(sql("SELECT name, ssh FROM machines ORDER BY name", vec![]))
        .await?
        .into_iter()
        .filter_map(|r| {
            Some((
                r.try_get::<String>("", "name").ok()?,
                r.try_get::<String>("", "ssh").ok()?,
            ))
        })
        .collect();
    let mut keys: Vec<String> = column(&db, KEYS, vec![]).await?;
    keys.extend(ItemType::ALL.iter().map(|t| t.key().to_string()));
    keys.sort();
    keys.dedup();
    Ok(Json(Private {
        projects: column(&db, PROJECTS, vec![]).await?,
        owners: column(&db, OWNERS, vec![]).await?,
        machines,
        hosts: column(&db, HOSTS, vec![]).await?,
        keys,
        titles: column(
            &db,
            &TITLES.replace("$1", &docket_core::private::TITLE_LEAST.to_string()),
            vec![],
        )
        .await?,
    }))
}

#[cfg(test)]
#[path = "tests/private.rs"]
mod tests;
