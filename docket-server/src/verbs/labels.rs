//! `label`: a label given to an item or taken from it, the one place labels are read and written. A
//! label is stored on the item it was given to alone; `carried` reads it down every plan to the items
//! under it.

use axum::Json;
use axum::extract::{Extension, Query, State};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};
use serde::Deserialize;

use docket_core::api::{LabelDone, LabelRequest, LabelRow};
use docket_core::label::{self, Label};
use docket_core::member::{Edge, Tie, labels_carried};

use crate::auth::Caller;
use crate::store::{column, sql};
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
    let known: Option<i64> = crate::store::scalar(
        &call.tx.conn,
        "SELECT id FROM labels WHERE project=? AND lower(name)=lower(?)",
        vec![call.slug.clone().into(), name.clone().into()],
    )
    .await?;
    match req.action.as_str() {
        "add" => {
            let about = given(req.about.as_deref()).map(str::to_string);
            let id = match known {
                Some(id) => {
                    if req.about.is_some() {
                        call.tx
                            .execute(
                                "UPDATE labels SET description=? WHERE id=?",
                                vec![about.into(), id.into()],
                            )
                            .await?;
                    }
                    id
                }
                None => crate::store::scalar(
                    &call.tx.conn,
                    "INSERT INTO labels (project, name, description) VALUES (?, ?, ?) RETURNING id",
                    vec![call.slug.clone().into(), name.clone().into(), about.into()],
                )
                .await?
                .ok_or(DbErr::RecordNotInserted)?,
            };
            call.tx
                .execute(
                    "INSERT INTO item_labels (rid, label_id) VALUES (?, ?) ON CONFLICT DO NOTHING",
                    vec![r.rid.into(), id.into()],
                )
                .await?;
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
