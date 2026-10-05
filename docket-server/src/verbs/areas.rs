//! `areas`: a project's areas as rows, the one place they are read and written, and the one place an
//! area is carried down a plan. `add` puts an area last, `edit` renames it or changes its description
//! or priority, `move` gives it another place, `rm` removes one no item carries.

use axum::Json;
use axum::extract::{Extension, Query, State};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};
use serde::Deserialize;

use docket_core::api::{AreasDone, AreasRequest};
use docket_core::area::{self, Area, Listed, Row};
use docket_core::item::Field;

use crate::auth::Caller;
use crate::store::{Tx, column, sql};
use crate::verbs::{Call, Failure, require_owner};

const ROWS: &str = "SELECT id, name, description, position, priority, history FROM areas \
                    WHERE project=? ORDER BY position";

/// A project's areas in position order, with their ids.
pub async fn listed<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Listed, DbErr> {
    let mut rows = Vec::new();
    for r in c.query_all_raw(sql(ROWS, vec![slug.into()])).await? {
        rows.push((
            r.try_get_by_index::<i64>(0)?,
            Area {
                name: r.try_get_by_index(1)?,
                description: r.try_get_by_index(2)?,
                position: r.try_get_by_index(3)?,
                priority: r.try_get_by_index(4)?,
                history: r.try_get_by_index(5)?,
            },
        ));
    }
    Ok(Listed { rows })
}

/// The row id of the area a write names, found ignoring case; empty is none.
///
/// # Errors
/// 409 naming the project's areas when the name is none of them.
pub async fn area_id<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    given: &str,
) -> Result<Option<i64>, Failure> {
    let listed = listed(c, slug).await?;
    let name = area::resolve(&listed.all(), given)?;
    Ok(name.and_then(|n| listed.id(&n)))
}

/// Refuses an area row an open item cannot be put in: one marked `history`.
///
/// # Errors
/// 409 naming the project's other areas when the area holds closed items only.
pub async fn open_into<C: ConnectionTrait>(c: &C, slug: &str, id: i64) -> Result<(), Failure> {
    let listed = listed(c, slug).await?;
    match listed.name(Some(id)) {
        Some(name) => Ok(area::open_into(&listed.all(), name)?),
        None => Ok(()),
    }
}

/// Why an item filed with no area is refused: the areas it can name, or how to add the first.
pub fn unfiled(listed: &Listed) -> String {
    let mut rows: Vec<&(i64, Area)> = listed.rows.iter().collect();
    rows.sort_by_key(|(_, a)| a.position);
    if rows.is_empty() {
        return "every item names its area, and this project has none: add one with docket areas add NAME --about TEXT".to_string();
    }
    let names: Vec<&str> = rows.iter().map(|(_, a)| a.name.as_str()).collect();
    format!(
        "every item names its area: give --area with one of {}, or file it under its plan with --parent",
        names.join(", ")
    )
}

/// The name of an area by its row id.
pub async fn name_of<C: ConnectionTrait>(c: &C, id: Option<i64>) -> Result<Option<String>, DbErr> {
    let Some(id) = id else {
        return Ok(None);
    };
    crate::store::scalar(c, "SELECT name FROM areas WHERE id=?", vec![id.into()]).await
}

/// The area as an edit note names it.
pub fn label(name: Option<&str>) -> &str {
    name.unwrap_or("none")
}

/// Every item under `rid`, to any depth, with its area.
async fn under<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Vec<(i64, Option<i64>)>, DbErr> {
    let rows = c
        .query_all_raw(sql(
            "WITH RECURSIVE below(rid) AS ( \
               SELECT rid FROM items WHERE parent_rid=? \
               UNION SELECT i.rid FROM items i JOIN below b ON i.parent_rid=b.rid) \
             SELECT i.rid, i.area_id FROM items i JOIN below b ON b.rid=i.rid ORDER BY i.rid",
            vec![rid.into()],
        ))
        .await?;
    rows.iter()
        .map(|r| Ok((r.try_get_by_index(0)?, r.try_get_by_index(1)?)))
        .collect()
}

/// The area of every item under `rid`, to any depth, set to `to`, with an edit event on each that
/// moves saying `why`. `rid` itself is the caller's to set.
pub async fn carry_under(
    tx: &mut Tx,
    slug: &str,
    rid: i64,
    to: Option<i64>,
    why: &str,
) -> Result<(), Failure> {
    for (below, was) in under(&tx.conn, rid).await? {
        if was != to {
            tx.update(below, &[Field::AreaId(to)]).await?;
            tx.event(slug, Some(below), "edited", Some(why), None, None)
                .await?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct ListQuery {
    project: String,
}

/// `GET /areas`: the project's areas in their order.
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
            .map(|(id, area)| Row { id, area })
            .collect(),
    ))
}

/// `POST /do/areas`: add, edit, move or remove one area. The owner's key only.
///
/// # Errors
/// 403 on an agent's key, 409 for a name or priority the rules refuse, or a removal while an item
/// carries the area.
pub async fn areas(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<AreasRequest>,
) -> Result<Json<AreasDone>, Failure> {
    require_owner(&caller, "areas")?;
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let listed = listed(&call.tx.conn, &call.slug).await?;
    let all = listed.all();
    let name = req.name.trim().to_string();
    match req.action.as_str() {
        "add" => {
            area::check_name(&all, &name, None)?;
            let priority = area::check_priority(req.priority.as_deref().unwrap_or(""))?;
            let about = req
                .about
                .as_deref()
                .map(str::trim)
                .filter(|a| !a.is_empty());
            call.tx
                .execute(
                    "INSERT INTO areas (project, name, description, position, priority) \
                     VALUES (?, ?, ?, ?, ?)",
                    vec![
                        call.slug.clone().into(),
                        name.into(),
                        about.map(str::to_string).into(),
                        i64::try_from(all.len()).unwrap_or(i64::MAX).into(),
                        priority.into(),
                    ],
                )
                .await?;
        }
        "edit" => edit(&mut call, &listed, &name, &req).await?,
        "move" => {
            let Some(to) = req.to else {
                return Err(Failure::Refused(format!(
                    "docket areas move {name} --to PLACE: name the place it takes, the first being 1"
                )));
            };
            let order = area::reorder(&all, &name, to)?;
            place(&call, &listed, &order).await?;
        }
        "rm" => {
            let id = existing(&listed, &name)?;
            let carried: Vec<String> = column(
                &call.tx.conn,
                "SELECT id FROM items WHERE area_id=? ORDER BY key, num",
                vec![id.into()],
            )
            .await?;
            if let Some(why) = area::rm_refusal(&name, &carried) {
                return Err(Failure::Refused(why));
            }
            call.tx
                .execute("DELETE FROM areas WHERE id=?", vec![id.into()])
                .await?;
            let left = listed
                .rows
                .iter()
                .filter(|(i, _)| *i != id)
                .map(|(_, a)| a.name.clone())
                .collect::<Vec<_>>();
            place(&call, &listed, &left).await?;
        }
        other => {
            return Err(Failure::Invalid(format!(
                "action is one of add, edit, move, rm, not {}",
                docket_core::text::py_repr(other)
            )));
        }
    }
    call.tx.touch_project(&call.slug);
    let areas = listed_after(&call).await?;
    call.tx.commit().await?;
    Ok(Json(AreasDone { areas }))
}

fn existing(listed: &Listed, name: &str) -> Result<i64, Failure> {
    match listed.id(name) {
        Some(id) => Ok(id),
        None => Err(area::resolve(&listed.all(), name).err().map_or_else(
            || Failure::Refused(format!("{name} is not an area here")),
            Into::into,
        )),
    }
}

async fn listed_after(call: &Call) -> Result<Vec<Area>, Failure> {
    Ok(listed(&call.tx.conn, &call.slug).await?.all())
}

/// A new name, description or priority. A rename rewrites each item that carries the area.
async fn edit(
    call: &mut Call,
    listed: &Listed,
    name: &str,
    req: &AreasRequest,
) -> Result<(), Failure> {
    let id = existing(listed, name)?;
    if req.rename.is_none() && req.about.is_none() && req.priority.is_none() {
        return Err(Failure::Refused(format!(
            "docket areas edit {name} needs --name NEW, --about TEXT or --priority WORD"
        )));
    }
    if let Some(new) = &req.rename {
        let current = listed.name(Some(id)).unwrap_or(name);
        area::check_name(&listed.all(), new, Some(current))?;
        call.tx
            .execute(
                "UPDATE areas SET name=? WHERE id=?",
                vec![new.trim().into(), id.into()],
            )
            .await?;
        let carried: Vec<i64> = column(
            &call.tx.conn,
            "SELECT rid FROM items WHERE area_id=?",
            vec![id.into()],
        )
        .await?;
        for rid in carried {
            call.tx.touch(rid);
        }
    }
    if let Some(about) = &req.about {
        let about = Some(about.trim()).filter(|a| !a.is_empty());
        call.tx
            .execute(
                "UPDATE areas SET description=? WHERE id=?",
                vec![about.into(), id.into()],
            )
            .await?;
    }
    if let Some(given) = &req.priority {
        let priority = area::check_priority(given)?;
        call.tx
            .execute(
                "UPDATE areas SET priority=? WHERE id=?",
                vec![priority.into(), id.into()],
            )
            .await?;
    }
    Ok(())
}

/// Each area given the place its name has in `order`, counted from nought.
async fn place(call: &Call, listed: &Listed, order: &[String]) -> Result<(), Failure> {
    call.tx
        .execute("SET CONSTRAINTS ALL DEFERRED", vec![])
        .await?;
    for (at, name) in order.iter().enumerate() {
        if let Some(id) = listed.id(name) {
            call.tx
                .execute(
                    "UPDATE areas SET position=? WHERE id=?",
                    vec![i64::try_from(at).unwrap_or(i64::MAX).into(), id.into()],
                )
                .await?;
        }
    }
    Ok(())
}
