//! `releases`: a project's releases as rows, the one place they are read and written. `add` places
//! a release by its version, `move` takes a release's open items to another, `ship` marks one shipped
//! once nothing in it is open, `rename` gives one a new name and place, `merge` takes every item of
//! one into another and deletes it, `rm` deletes one nothing is in.

use axum::Json;
use axum::extract::{Extension, Query, State};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};
use serde::Deserialize;

use docket_core::api::{ReleasesDone, ReleasesRequest};
use docket_core::item::Field;
use docket_core::release::{self, Listed, Release, Row};

use crate::auth::Caller;
use crate::store::sql;
use crate::verbs::graph::Order;
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

/// `POST /do/releases`: add, move, ship, rename, merge or remove one release. The owner's key only.
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
            move_open(&mut call, from, &name, to, req.carry).await?
        }
        "ship" => {
            let from = existing(&listed, &name)?;
            let moved = match req.to.as_deref() {
                Some(to) => move_open(&mut call, from, &name, to, req.carry).await?,
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
        "rename" => {
            let from = existing(&listed, &name)?;
            let Some(new) = req.rename.as_deref().map(str::trim) else {
                return Err(Failure::Refused(format!(
                    "docket releases rename {name} NEW: give the new name"
                )));
            };
            rename(&mut call, &listed, from, &name, new).await?;
            Vec::new()
        }
        "merge" => {
            let from = existing(&listed, &name)?;
            let Some(to) = req.to.as_deref().map(str::trim) else {
                return Err(Failure::Refused(format!(
                    "docket releases merge {name} --into RELEASE: name the release it joins"
                )));
            };
            merge(&mut call, &listed, from, &name, to, req.carry).await?
        }
        "rm" => {
            let from = existing(&listed, &name)?;
            let held = items_in(&call, from, false).await?;
            if let Some(why) = release::rm_refusal(&name, &held) {
                return Err(Failure::Refused(why));
            }
            call.tx
                .execute("DELETE FROM releases WHERE id=?", vec![from.into()])
                .await?;
            close_gap(&call, listed.rows.iter().find(|(i, _)| *i == from)).await?;
            project_event(&call, &format!("release {name} removed")).await?;
            Vec::new()
        }
        other => {
            return Err(Failure::Invalid(format!(
                "action is one of add, move, ship, rename, merge, rm, not {}",
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
    items_in(call, release, true).await
}

/// The ids of the items in a release, by key and number: the open ones, or every one.
async fn items_in(call: &Call, release: i64, open_only: bool) -> Result<Vec<String>, Failure> {
    let state = if open_only { " AND state='open'" } else { "" };
    let rows = call
        .tx
        .conn
        .query_all_raw(sql(
            &format!("SELECT id FROM items WHERE release_id=?{state} ORDER BY key, num"),
            vec![release.into()],
        ))
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(r.try_get_by_index::<String>(0)?);
    }
    Ok(out)
}

/// A project event naming a change to the releases.
async fn project_event(call: &Call, note: &str) -> Result<(), Failure> {
    call.tx
        .event(&call.slug, None, "edited", Some(note), None, None)
        .await?;
    Ok(())
}

/// The later releases moved down one over the place a removed release held, so positions stay a
/// run from nought.
async fn close_gap(call: &Call, gone: Option<&(i64, Release)>) -> Result<(), Failure> {
    let Some((_, gone)) = gone else {
        return Ok(());
    };
    call.tx
        .execute("SET CONSTRAINTS ALL DEFERRED", vec![])
        .await?;
    call.tx
        .execute(
            "UPDATE releases SET position=position-1 WHERE project=? AND position>?",
            vec![call.slug.clone().into(), gone.position.into()],
        )
        .await?;
    Ok(())
}

/// A release under a new name at the place its version gives it, every item in it rewritten for the
/// dump.
async fn rename(
    call: &mut Call,
    listed: &Listed,
    id: i64,
    old: &str,
    new: &str,
) -> Result<(), Failure> {
    let at = release::rename_place(&listed.all(), old, new)?;
    let was = listed
        .rows
        .iter()
        .find(|(i, _)| *i == id)
        .map_or(0, |(_, r)| r.position);
    if at != was && !call.ctx.force {
        Order::load(&call.tx.conn, &call.slug)
            .await?
            .refuse_rename(old, new, at)?;
    }
    call.tx
        .execute("SET CONSTRAINTS ALL DEFERRED", vec![])
        .await?;
    call.tx
        .execute(
            "UPDATE releases SET position=position-1 WHERE project=? AND position>?",
            vec![call.slug.clone().into(), was.into()],
        )
        .await?;
    call.tx
        .execute(
            "UPDATE releases SET position=position+1 WHERE project=? AND position>=? AND id<>?",
            vec![call.slug.clone().into(), at.into(), id.into()],
        )
        .await?;
    call.tx
        .execute(
            "UPDATE releases SET name=?, position=? WHERE id=?",
            vec![new.into(), at.into(), id.into()],
        )
        .await?;
    let carried: Vec<i64> = crate::store::column(
        &call.tx.conn,
        "SELECT rid FROM items WHERE release_id=?",
        vec![id.into()],
    )
    .await?;
    for rid in carried {
        call.tx.touch(rid);
    }
    project_event(call, &format!("release {old} renamed to {new}")).await
}

/// Every item of a release moved to another, the open ones checked for order as `move` checks
/// them, and the emptied release deleted.
async fn merge(
    call: &mut Call,
    listed: &Listed,
    from: i64,
    from_name: &str,
    to: &str,
    carry: bool,
) -> Result<Vec<String>, Failure> {
    let Some((to_id, target)) = listed.rows.iter().find(|(_, r)| r.name == to) else {
        return Err(Failure::Refused(format!("{to} is not a release here")));
    };
    let open = open_in(call, from).await?;
    if let Some(why) = release::merge_refusal(from_name, to, target.shipped_at.is_some(), &open) {
        return Err(Failure::Refused(why));
    }
    let moved = move_items(call, from, from_name, Some(*to_id), to, carry, true).await?;
    call.tx
        .execute("DELETE FROM releases WHERE id=?", vec![from.into()])
        .await?;
    close_gap(call, listed.rows.iter().find(|(i, _)| *i == from)).await?;
    project_event(call, &format!("release {from_name} merged into {to}")).await?;
    Ok(moved)
}

/// The open items of one release moved to another, each with an edit event naming the move.
async fn move_open(
    call: &mut Call,
    from: i64,
    from_name: &str,
    to: &str,
    carry: bool,
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
    move_items(call, from, from_name, to_id, &to_name, carry, false).await
}

/// The items of one release moved to `to_id`, each with an edit event naming the move: the open
/// ones, or with `closed` every one. Only the open ones are held to the release order.
async fn move_items(
    call: &mut Call,
    from: i64,
    from_name: &str,
    to_id: Option<i64>,
    to_name: &str,
    carry: bool,
    closed: bool,
) -> Result<Vec<String>, Failure> {
    let open = open_in(call, from).await?;
    let mut rids = Vec::with_capacity(open.len());
    for id in &open {
        rids.push(call.item(id).await?.rid);
    }
    let order = Order::load(&call.tx.conn, &call.slug).await?;
    if carry {
        let note = format!("release {to_name} (carried with {})", open.join(", "));
        for (rid, _, release) in order.carried_all(&rids, to_id) {
            call.tx.update(rid, &[Field::ReleaseId(release)]).await?;
            call.tx
                .event(&call.slug, Some(rid), "edited", Some(&note), None, None)
                .await?;
        }
    } else if !call.ctx.force {
        order.refuse(&order.moving_all(&rids, to_id))?;
    }
    let ids = if closed {
        items_in(call, from, false).await?
    } else {
        open
    };
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
