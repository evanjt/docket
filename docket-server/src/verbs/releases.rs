//! `releases`: a project's releases as rows, the one place they are read and written. `add` places
//! a release by its version, `move` takes a release's open items to another, `ship` marks one shipped
//! once nothing in it is open.

use axum::Json;
use axum::extract::{Extension, Query, State};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};
use serde::Deserialize;

use docket_core::api::{ReleasesDone, ReleasesRequest};
use docket_core::item::Field;
use docket_core::release::{self, Listed, Release, Row};

use crate::auth::Caller;
use crate::store::sql;
use crate::verbs::{Call, Failure, require_owner};

const ROWS: &str = "SELECT id, name, position, target_date, shipped_at, note FROM releases \
                    WHERE project=? ORDER BY position";

/// A project's releases in position order, with their ids.
pub async fn listed<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Listed, DbErr> {
    let mut rows = Vec::new();
    for r in c.query_all_raw(sql(ROWS, vec![slug.into()])).await? {
        rows.push((
            r.try_get_by_index::<i64>(0)?,
            Release {
                name: r.try_get_by_index(1)?,
                position: r.try_get_by_index(2)?,
                target_date: r.try_get_by_index(3)?,
                shipped_at: r.try_get_by_index(4)?,
                note: r.try_get_by_index(5)?,
            },
        ));
    }
    Ok(Listed::new(rows))
}

/// The row id of the release a write names: `current`, a release not shipped, or empty for the
/// backlog.
///
/// # Errors
/// 409 naming the releases when the name is no open release here.
pub async fn release_id<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    given: &str,
) -> Result<Option<i64>, Failure> {
    let listed = listed(c, slug).await?;
    let name = release::resolve(&listed.all(), given)?;
    Ok(name.and_then(|n| listed.id(&n)))
}

/// The name of the release an item is in, none for the backlog.
pub async fn name_of<C: ConnectionTrait>(
    c: &C,
    id: Option<i64>,
) -> Result<Option<String>, Failure> {
    let Some(id) = id else {
        return Ok(None);
    };
    Ok(crate::store::scalar(c, "SELECT name FROM releases WHERE id=?", vec![id.into()]).await?)
}

#[derive(Deserialize)]
pub struct ListQuery {
    project: String,
    #[serde(default)]
    all: bool,
}

/// `GET /releases`: the project's releases in the order they ship, the shipped ones only with `all`.
///
/// # Errors
/// 404 for an unknown project.
pub async fn list(
    State(db): State<DatabaseConnection>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Row>>, Failure> {
    crate::store::project(&db, &q.project).await?;
    let listed = listed(&db, &q.project).await?;
    Ok(Json(
        listed
            .rows
            .into_iter()
            .filter(|(_, r)| q.all || r.shipped_at.is_none())
            .map(|(id, release)| Row { id, release })
            .collect(),
    ))
}

/// `POST /do/releases`: add, move or ship one release. The owner's key only.
///
/// # Errors
/// 403 on an agent's key, 409 for a name the rules refuse or a ship with open items left.
pub async fn releases(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<ReleasesRequest>,
) -> Result<Json<ReleasesDone>, Failure> {
    require_owner(&caller, "releases")?;
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let listed = listed(&call.tx.conn, &call.slug).await?;
    let name = req.name.trim().to_string();
    let moved = match req.action.as_str() {
        "add" => {
            add(&call, &listed, &name, &req).await?;
            Vec::new()
        }
        "move" => {
            let Some(to) = req.to.as_deref() else {
                return Err(Failure::Refused(format!(
                    "docket releases move {name} --to RELEASE: name where its open items go"
                )));
            };
            let from = existing(&listed, &name)?;
            move_open(&mut call, from, &name, to).await?
        }
        "ship" => {
            let from = existing(&listed, &name)?;
            let moved = match req.to.as_deref() {
                Some(to) => move_open(&mut call, from, &name, to).await?,
                None => Vec::new(),
            };
            let open = open_in(&call, from).await?;
            if let Some(why) = release::ship_refusal(&name, &open) {
                return Err(Failure::Refused(why));
            }
            call.tx
                .execute(
                    "UPDATE releases SET shipped_at=? WHERE id=?",
                    vec![call.ctx.now.clone().into(), from.into()],
                )
                .await?;
            moved
        }
        other => {
            return Err(Failure::Invalid(format!(
                "action is one of add, move, ship, not {}",
                docket_core::text::quoted(other)
            )));
        }
    };
    call.tx.touch_project(&call.slug);
    let releases = listed_after(&call).await?;
    call.tx.commit().await?;
    Ok(Json(ReleasesDone { releases, moved }))
}

fn existing(listed: &Listed, name: &str) -> Result<i64, Failure> {
    listed
        .id(name)
        .ok_or_else(|| Failure::Refused(format!("{name} is not a release here")))
}

async fn listed_after(call: &Call) -> Result<Vec<Release>, Failure> {
    Ok(listed(&call.tx.conn, &call.slug).await?.all())
}

/// A new release at the place its version gives it, the later ones moved up one.
async fn add(
    call: &Call,
    listed: &Listed,
    name: &str,
    req: &ReleasesRequest,
) -> Result<(), Failure> {
    let at = release::place(&listed.all(), name)?;
    call.tx
        .execute("SET CONSTRAINTS ALL DEFERRED", vec![])
        .await?;
    call.tx
        .execute(
            "UPDATE releases SET position=position+1 WHERE project=? AND position>=?",
            vec![call.slug.clone().into(), at.into()],
        )
        .await?;
    call.tx
        .execute(
            "INSERT INTO releases (project, name, position, target_date, note) VALUES (?, ?, ?, ?, ?)",
            vec![
                call.slug.clone().into(),
                name.into(),
                at.into(),
                req.target_date.clone().into(),
                req.note.clone().into(),
            ],
        )
        .await?;
    Ok(())
}

async fn open_in(call: &Call, release: i64) -> Result<Vec<String>, Failure> {
    let rows = call
        .tx
        .conn
        .query_all_raw(sql(
            "SELECT id FROM items WHERE release_id=? AND state='open' ORDER BY key, num",
            vec![release.into()],
        ))
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(r.try_get_by_index::<String>(0)?);
    }
    Ok(out)
}

/// The open items of one release moved to another, each with an edit event naming the move.
async fn move_open(
    call: &mut Call,
    from: i64,
    from_name: &str,
    to: &str,
) -> Result<Vec<String>, Failure> {
    let to_id = release_id(&call.tx.conn, &call.slug, to).await?;
    if to_id == Some(from) {
        return Err(Failure::Refused(format!(
            "{from_name} is where the items already are"
        )));
    }
    let to_name = name_of(&call.tx.conn, to_id)
        .await?
        .unwrap_or_else(|| "the backlog".to_string());
    let ids = open_in(call, from).await?;
    let note = format!("release {from_name} to {to_name}");
    for id in &ids {
        let r = call.item(id).await?;
        call.tx.update(r.rid, &[Field::ReleaseId(to_id)]).await?;
        call.tx
            .event(&call.slug, Some(r.rid), "edited", Some(&note), None, None)
            .await?;
    }
    Ok(ids)
}
