//! `project` and `reindex`: a checkout's project matched or created, and a project's index rebuilt.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::DatabaseConnection;
use serde_json::Value;

use docket_core::api::{Common, ProjectRequest, ProjectResolved, Reindexed};
use docket_core::project::{default_keys, matches};
use docket_core::pyjson;

use crate::auth::Caller;
use crate::store::{Tx, column, scalar};
use crate::verbs::Failure;

/// The project a checkout belongs to: matched by its remote's slug, a shared remote or its directory
/// name, else created with the default keys. Its remotes are recorded on the project either way.
///
/// # Errors
/// 409 when the checkout could be several projects, or none is found and creating is not asked.
pub async fn project(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<ProjectRequest>,
) -> Result<Json<ProjectResolved>, Failure> {
    let mut tx = Tx::begin(&db, &caller.host).await?;
    let slugs: Vec<String> =
        column(&tx.conn, "SELECT slug FROM projects ORDER BY slug", vec![]).await?;
    let mut known = Vec::with_capacity(slugs.len());
    for slug in slugs {
        known.push((slug.clone(), remotes_of(&tx, &slug).await?));
    }
    let found = matches(
        &known,
        req.candidate.as_deref(),
        &req.remotes,
        &req.basename,
    );
    if found.len() > 1 {
        return Ok(Json(ProjectResolved {
            slug: None,
            how: "ambiguous".into(),
            matches: found,
        }));
    }
    let (slug, how) = match found.into_iter().next() {
        Some(slug) => (slug, "matched"),
        None if !req.create => {
            return Ok(Json(ProjectResolved {
                slug: None,
                how: "none".into(),
                matches: Vec::new(),
            }));
        }
        None => {
            let slug = req
                .candidate
                .clone()
                .unwrap_or_else(|| req.basename.clone());
            create(&mut tx, &slug, &req.remotes).await?;
            (slug, "created")
        }
    };
    add_remotes(&mut tx, &slug, &req.remotes).await?;
    tx.commit().await?;
    Ok(Json(ProjectResolved {
        slug: Some(slug),
        how: how.into(),
        matches: Vec::new(),
    }))
}

async fn remotes_of(tx: &Tx, slug: &str) -> Result<Vec<String>, Failure> {
    let text: Option<String> = scalar(
        &tx.conn,
        "SELECT remotes FROM projects WHERE slug=?",
        vec![slug.into()],
    )
    .await?;
    Ok(text
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default())
}

fn sorted_unique(urls: impl IntoIterator<Item = String>) -> Value {
    let mut urls: Vec<String> = urls.into_iter().collect();
    urls.sort();
    urls.dedup();
    Value::from(urls)
}

async fn create(tx: &mut Tx, slug: &str, remotes: &[String]) -> Result<(), Failure> {
    tx.execute(
        "INSERT INTO projects (slug, keys, remotes, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        vec![
            slug.into(),
            pyjson::dumps(&default_keys(), false).into(),
            pyjson::dumps(&sorted_unique(remotes.iter().cloned()), false).into(),
            tx.now.clone().into(),
            tx.now.clone().into(),
        ],
    )
    .await?;
    tx.touch_project(slug);
    Ok(())
}

/// The checkout's remotes added to the project's, when they bring one it lacks.
async fn add_remotes(tx: &mut Tx, slug: &str, urls: &[String]) -> Result<(), Failure> {
    if urls.is_empty() {
        return Ok(());
    }
    let have = remotes_of(tx, slug).await?;
    let all = sorted_unique(have.iter().chain(urls).cloned());
    if all == Value::from(have) {
        return Ok(());
    }
    tx.execute(
        "UPDATE projects SET remotes=?, updated_at=? WHERE slug=?",
        vec![
            pyjson::dumps(&all, false).into(),
            tx.now.clone().into(),
            slug.into(),
        ],
    )
    .await?;
    tx.touch_project(slug);
    Ok(())
}

/// The search rows and citation links of every item of a project, rebuilt from their bodies. No item
/// changes, so nothing is marked for the dump.
///
/// # Errors
/// 404 for an unknown project.
pub async fn reindex(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<Common>,
) -> Result<Json<Reindexed>, Failure> {
    let tx = Tx::begin(&db, &caller.host).await?;
    tx.project(&req.project).await?;
    let rids: Vec<i64> = column(
        &tx.conn,
        "SELECT rid FROM items WHERE project=? ORDER BY rid",
        vec![req.project.clone().into()],
    )
    .await?;
    for rid in &rids {
        tx.index(*rid).await?;
    }
    tx.commit().await?;
    Ok(Json(Reindexed {
        project: req.project,
        items: rids.len(),
    }))
}
