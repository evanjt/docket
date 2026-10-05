//! A project's publications: `GET /publications` lists them newest first, the first being where the
//! next squash starts, and `POST /do/publication` records one a squash wrote. The rows are the
//! server's; a client only posts and reads them.

use axum::extract::{Extension, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult};

use docket_core::api::{PublicationRequest, Publications};
use docket_core::publication::{Publication, check_sha};
use docket_core::word::ItemType;

use crate::auth::Caller;
use crate::facts::InProject;
use crate::store::{Tx, item_of, project, scalar, sql};
use crate::verbs::Failure;

const LIST: &str = "SELECT p.published_sha, p.work_sha, p.created_at, \
                    COALESCE((SELECT array_agg(i.id ORDER BY i.id) FROM publication_plans pp \
                      JOIN items i ON i.rid=pp.rid WHERE pp.publication=p.id), '{}') AS plans \
                    FROM publications p WHERE p.project=? ORDER BY p.created_at DESC, p.id DESC";
const RECORDED: &str = "SELECT COUNT(*) FROM publications WHERE project=? AND published_sha=?";
const RECORD: &str = "INSERT INTO publications (project, published_sha, work_sha, created_at) \
                      VALUES (?, ?, ?, ?) RETURNING id";
const COVERS: &str = "INSERT INTO publication_plans (publication, rid) VALUES (?, ?) \
                      ON CONFLICT DO NOTHING";

pub fn router() -> Router<DatabaseConnection> {
    Router::new()
        .route("/publications", get(read))
        .route("/do/publication", post(record))
}

#[derive(FromQueryResult)]
struct Row {
    published_sha: String,
    work_sha: String,
    created_at: String,
    plans: Vec<String>,
}

async fn listed<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Publications, Failure> {
    let rows = c.query_all_raw(sql(LIST, vec![slug.into()])).await?;
    let publications = rows
        .iter()
        .map(|r| {
            Row::from_query_result(r, "").map(|r| Publication {
                published: r.published_sha,
                work: r.work_sha,
                plans: r.plans,
                created_at: r.created_at,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(Publications {
        project: slug.to_string(),
        publications,
    })
}

/// A project's publications, newest first.
///
/// # Errors
/// 404 for an unknown project.
pub async fn read(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<Publications>, Failure> {
    project(&db, &q.project).await?;
    Ok(Json(listed(&db, &q.project).await?))
}

/// One publication recorded; the project's publications as they stand after, newest first. Any
/// key may record one.
///
/// # Errors
/// 404 for an unknown project or plan, 409 for a sha that is not a full one, an item that is no
/// plan, or a published sha recorded already.
pub async fn record(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<PublicationRequest>,
) -> Result<Json<Publications>, Failure> {
    check_sha("published", &req.published)?;
    check_sha("work", &req.work)?;
    let slug = req.common.project.clone();
    let tx = Tx::begin(&db, &caller.host).await?;
    tx.project(&slug).await?;
    let recorded: Option<i64> = scalar(
        &tx.conn,
        RECORDED,
        vec![slug.clone().into(), req.published.clone().into()],
    )
    .await?;
    if recorded.unwrap_or(0) > 0 {
        return Err(Failure::Refused(format!(
            "{} is recorded as published in {slug} already",
            req.published
        )));
    }
    let mut rids = Vec::new();
    for id in &req.plans {
        let plan = item_of(&tx.conn, &slug, id).await?;
        if plan.item_type != ItemType::Plan {
            return Err(Failure::Refused(format!(
                "{} is a {}, and a publication covers plans",
                plan.id,
                plan.item_type.as_str()
            )));
        }
        rids.push(plan.rid);
    }
    let id: i64 = scalar(
        &tx.conn,
        RECORD,
        vec![
            slug.clone().into(),
            req.published.into(),
            req.work.into(),
            tx.now.clone().into(),
        ],
    )
    .await?
    .ok_or(sea_orm::DbErr::RecordNotInserted)?;
    for rid in rids {
        tx.execute(COVERS, vec![id.into(), rid.into()]).await?;
    }
    let out = listed(&tx.conn, &slug).await?;
    tx.commit().await?;
    Ok(Json(out))
}

#[cfg(test)]
#[path = "tests/publications.rs"]
mod tests;
