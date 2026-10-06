//! `/changelog`: the closed work of one release, grouped by area.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::Value;

use docket_core::changelog::{Entry, changelog};
use docket_core::release::CURRENT;

use crate::reads::public::{Failure, failure, internal, items_where, project_of};
use docket_core::word::kind_of_type;

#[derive(Deserialize)]
pub struct ChangelogQuery {
    project: String,
    release: Option<String>,
}

/// The sections of a release's closed work, the current release when none is named.
///
/// # Errors
/// 404 for an unknown project, 400 for a release the project lacks or when none is open.
pub async fn changelog_of(
    State(db): State<DatabaseConnection>,
    Query(q): Query<ChangelogQuery>,
) -> Result<Json<Value>, Failure> {
    project_of(&db, &q.project).await?;
    let releases = crate::verbs::releases::listed(&db, &q.project)
        .await
        .map_err(|e| internal(&e))?;
    let given = q
        .release
        .as_deref()
        .filter(|r| !r.is_empty())
        .unwrap_or(CURRENT);
    let name = if given == CURRENT {
        releases.open.first().cloned()
    } else {
        Some(given.to_string())
    };
    let Some(id) = name.as_deref().and_then(|n| releases.id(n)) else {
        let names: Vec<&str> = releases.rows.iter().map(|(_, r)| r.name.as_str()).collect();
        return Err(failure(
            StatusCode::BAD_REQUEST,
            &format!("release: choose from {}", names.join(", ")),
        ));
    };
    let areas = crate::verbs::areas::listed(&db, &q.project)
        .await
        .map_err(|e| internal(&e))?;
    let all = items_where(&db, &q.project, "ORDER BY updated_at, rid", vec![]).await?;
    let entries: Vec<Entry> = all
        .iter()
        .filter(|i| i.release_id == Some(id))
        .map(|i| Entry {
            id: i.id.clone(),
            title: i.title.clone(),
            state: i.state.clone(),
            kind: kind_of_type(&i.item_type),
            area: areas.name(Some(i.area_id)).map(str::to_string),
            plan: i
                .parent_rid
                .and_then(|p| all.iter().find(|x| x.rid == p))
                .map(|p| p.title.clone()),
        })
        .collect();
    let sections = changelog(&entries, &areas.all());
    Ok(Json(serde_json::json!({
        "release": name,
        "sections": sections,
    })))
}
