//! `label`: a label given to an item or taken from it, the one place labels are read and written. A
//! label is stored on the item it was given to alone; `carried` reads it down every plan to the items
//! under it.

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Extension, Query, State};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};
use serde::Deserialize;

use docket_core::api::{LabelDone, LabelRequest, LabelRow};
use docket_core::label::{self, Label};
use docket_core::member::{Edge, Tie, labels_carried};

use crate::auth::Caller;
use crate::store::{column, scalar, sql};
use crate::verbs::{Call, Failure, given};

/// The rids from `rid` up its parents, `rid` first, each once.
async fn chain<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Vec<i64>, DbErr> {
    let up: Vec<i64> = column(
        c,
        "WITH RECURSIVE up(rid, depth) AS ( \
           SELECT rid, 0 FROM items WHERE rid=? \
           UNION ALL SELECT p.rid, up.depth + 1 FROM up JOIN items i ON i.rid=up.rid \
             JOIN items p ON p.rid=i.parent_rid WHERE up.depth < 1000) \
         SELECT rid FROM up ORDER BY depth",
        vec![rid.into()],
    )
    .await?;
    let mut seen = Vec::with_capacity(up.len());
    for r in up {
        if !seen.contains(&r) {
            seen.push(r);
        }
    }
    Ok(seen)
}

/// The labels an item carries with their descriptions: its own, then those of each plan above it,
/// nearest first.
pub async fn carried<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Vec<Label>, DbErr> {
    let up = chain(c, rid).await?;
    let ties: Vec<Tie> = up
        .windows(2)
        .map(|w| Tie {
            rid: w[0],
            edge: Edge::Parent,
            to: w[1],
        })
        .collect();
    let rows = c
        .query_all_raw(sql(
            "SELECT il.rid, l.name, l.description FROM item_labels il \
             JOIN labels l ON l.id=il.label_id \
             WHERE il.rid IN (SELECT rid FROM items WHERE rid = ANY(?)) ORDER BY l.name",
            vec![up.clone().into()],
        ))
        .await?;
    let mut own: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    let mut described: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    for r in rows {
        let name: String = r.try_get_by_index(1)?;
        own.entry(r.try_get_by_index(0)?)
            .or_default()
            .push(name.clone());
        described.insert(name, r.try_get_by_index(2)?);
    }
    Ok(labels_carried(&ties, &own, rid)
        .into_iter()
        .map(|name| Label {
            description: described.get(&name).cloned().flatten(),
            name,
        })
        .collect())
}

/// The id of a project's label by name, ignoring case, made when the project has none by it.
async fn made<C: ConnectionTrait>(c: &C, slug: &str, name: &str) -> Result<i64, DbErr> {
    let known: Option<i64> = scalar(
        c,
        "SELECT id FROM labels WHERE project=? AND lower(name)=lower(?)",
        vec![slug.into(), name.into()],
    )
    .await?;
    match known {
        Some(id) => Ok(id),
        None => scalar(
            c,
            "INSERT INTO labels (project, name, description) VALUES (?, ?, NULL) RETURNING id",
            vec![slug.into(), name.into()],
        )
        .await?
        .ok_or(DbErr::RecordNotInserted),
    }
}

/// Give an item a label by name, making the label when the project has none by it.
///
/// # Errors
/// The database.
pub async fn give<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    rid: i64,
    name: &str,
) -> Result<(), DbErr> {
    let id = made(c, slug, name).await?;
    c.execute_raw(sql(
        "INSERT INTO item_labels (rid, label_id) VALUES (?, ?) ON CONFLICT DO NOTHING",
        vec![rid.into(), id.into()],
    ))
    .await?;
    Ok(())
}

/// The item's own labels that `within` passes become `names`: each of them not among `names` is
/// taken, and each of `names` the item lacks is given.
///
/// # Errors
/// The database.
pub async fn set<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    rid: i64,
    names: &[String],
    within: impl Fn(&str) -> bool,
) -> Result<(), DbErr> {
    for name in own(c, rid).await? {
        let wanted = names.iter().any(|n| n.eq_ignore_ascii_case(&name));
        if within(&name) && !wanted {
            c.execute_raw(sql(
                "DELETE FROM item_labels WHERE rid=? AND label_id IN \
                 (SELECT id FROM labels WHERE project=? AND name=?)",
                vec![rid.into(), slug.into(), name.into()],
            ))
            .await?;
        }
    }
    for name in names {
        give(c, slug, rid, name).await?;
    }
    Ok(())
}

/// The names of the labels an item was given itself, by name.
///
/// # Errors
/// The database.
pub async fn own<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Vec<String>, DbErr> {
    column(
        c,
        "SELECT l.name FROM item_labels il JOIN labels l ON l.id=il.label_id \
         WHERE il.rid=? ORDER BY l.name",
        vec![rid.into()],
    )
    .await
}

/// Each item of a project carrying a label whose name matches a `LIKE` pattern ignoring case, with
/// the label's name: the items given it and everything under them. Binds the project, then the
/// pattern.
pub const CARRIERS: &str = "WITH RECURSIVE down(rid, name) AS ( \
       SELECT il.rid, l.name FROM item_labels il JOIN labels l ON l.id=il.label_id \
        WHERE l.project=? AND lower(l.name) LIKE lower(?) \
       UNION SELECT i.rid, down.name FROM items i JOIN down ON i.parent_rid=down.rid) \
     SELECT rid, name FROM down";

/// The condition keeping the rows whose `column` is an item carrying a label by [`CARRIERS`], or
/// with `not` one not carrying it. Binds as [`CARRIERS`] does.
#[must_use]
pub fn carried_cond(column: &str, not: bool) -> String {
    let op = if not { "NOT IN" } else { "IN" };
    format!(" AND {column} {op} (SELECT rid FROM ({CARRIERS}) c)")
}

/// The `LIKE` pattern that matches the label `name` alone.
#[must_use]
pub fn exactly(name: &str) -> String {
    name.trim()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// The items of a project carrying a label whose name matches `pattern`, by [`CARRIERS`], by name
/// then rid.
///
/// # Errors
/// The database.
pub async fn carriers<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    pattern: &str,
) -> Result<Vec<(i64, String)>, DbErr> {
    let rows = c
        .query_all_raw(sql(
            &format!("{CARRIERS} ORDER BY name, rid"),
            vec![slug.into(), pattern.into()],
        ))
        .await?;
    rows.iter()
        .map(|r| Ok((r.try_get_by_index(0)?, r.try_get_by_index(1)?)))
        .collect()
}

/// The items of a project carrying the label `name`, given it or under a plan given it.
///
/// # Errors
/// The database.
pub async fn carrying<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    name: &str,
) -> Result<HashSet<i64>, DbErr> {
    Ok(carriers(c, slug, &exactly(name))
        .await?
        .into_iter()
        .map(|(rid, _)| rid)
        .collect())
}

/// The items carrying the label `name` when one is given: what a read narrowed to a label keeps.
///
/// # Errors
/// The database.
pub async fn narrowed<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    name: Option<&str>,
) -> Result<Option<HashSet<i64>>, DbErr> {
    match name {
        Some(name) => Ok(Some(carrying(c, slug, name).await?)),
        None => Ok(None),
    }
}

/// The other items of a project in the group an item's labels name, by rid; none when they name
/// none.
///
/// # Errors
/// The database.
pub async fn grouped_with<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    rid: i64,
) -> Result<Vec<i64>, DbErr> {
    let mine: Vec<String> = carried(c, rid).await?.into_iter().map(|l| l.name).collect();
    let Some(group) = label::group_of(&mine) else {
        return Ok(Vec::new());
    };
    let mut others: Vec<i64> = carrying(c, slug, &label::of_group(group))
        .await?
        .into_iter()
        .filter(|r| *r != rid)
        .collect();
    others.sort_unstable();
    Ok(others)
}

/// A project's labels by name.
pub async fn listed<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Vec<Label>, DbErr> {
    let rows = c
        .query_all_raw(sql(
            "SELECT name, description FROM labels WHERE project=? ORDER BY name",
            vec![slug.into()],
        ))
        .await?;
    rows.iter()
        .map(|r| {
            Ok(Label {
                name: r.try_get_by_index(0)?,
                description: r.try_get_by_index(1)?,
            })
        })
        .collect()
}

#[derive(Deserialize)]
pub struct ListQuery {
    project: String,
}

/// `GET /labels`: the project's labels by name, with how many items were given each.
///
/// # Errors
/// 404 for an unknown project.
pub async fn list(
    State(db): State<DatabaseConnection>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<LabelRow>>, Failure> {
    crate::store::project(&db, &q.project).await?;
    let rows = db
        .query_all_raw(sql(
            "SELECT l.name, l.description, \
               (SELECT count(*) FROM item_labels il WHERE il.label_id=l.id) \
             FROM labels l WHERE l.project=? ORDER BY l.name",
            vec![q.project.clone().into()],
        ))
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(LabelRow {
            label: Label {
                name: r.try_get_by_index(0)?,
                description: r.try_get_by_index(1)?,
            },
            items: r.try_get_by_index(2)?,
        });
    }
    Ok(Json(out))
}

/// `POST /do/label`: give an item a label, making it when the project has none by the name, or take
/// one from it. An agent's key may label.
///
/// # Errors
/// 409 for an empty name, or a removal of a label the item does not carry itself.
pub async fn label(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<LabelRequest>,
) -> Result<Json<LabelDone>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let name = label::check_name(&req.name)?;
    let known: Option<i64> = scalar(
        &call.tx.conn,
        "SELECT id FROM labels WHERE project=? AND lower(name)=lower(?)",
        vec![call.slug.clone().into(), name.clone().into()],
    )
    .await?;
    match req.action.as_str() {
        "add" => {
            give(&call.tx.conn, &call.slug, r.rid, &name).await?;
            if req.about.is_some() {
                let about = given(req.about.as_deref()).map(str::to_string);
                call.tx
                    .execute(
                        "UPDATE labels SET description=? WHERE project=? AND lower(name)=lower(?)",
                        vec![about.into(), call.slug.clone().into(), name.clone().into()],
                    )
                    .await?;
            }
        }
        "rm" => {
            let removed = match known {
                Some(id) => {
                    call.tx
                        .execute(
                            "DELETE FROM item_labels WHERE rid=? AND label_id=?",
                            vec![r.rid.into(), id.into()],
                        )
                        .await?
                }
                None => 0,
            };
            if removed == 0 {
                return Err(Failure::Refused(format!(
                    "{} does not carry the label {name}",
                    r.id
                )));
            }
        }
        other => {
            return Err(Failure::Invalid(format!(
                "action is one of add, rm, not {}",
                docket_core::text::py_repr(other)
            )));
        }
    }
    let note = format!(
        "{} {name}",
        if req.action == "add" {
            "label"
        } else {
            "unlabel"
        }
    );
    call.tx
        .event(&call.slug, Some(r.rid), "edited", Some(&note), None, None)
        .await?;
    call.tx.touch(r.rid);
    call.tx.touch_project(&call.slug);
    let labels = carried(&call.tx.conn, r.rid).await?;
    call.tx.commit().await?;
    Ok(Json(LabelDone { id: r.id, labels }))
}
