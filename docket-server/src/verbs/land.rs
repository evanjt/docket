//! `land`: the steps of a landing recorded on the items of a finished build. A step writes an event
//! on each item named, which `assignment::land` folds into the stage its open claim reads from.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::DatabaseConnection;
use serde_json::{Map, Value, json};

use docket_core::api::{LandRequest, MovedMany};
use docket_core::assignment::LAND_ACTIONS;
use docket_core::item::Item;
use docket_core::rules::holds;

use crate::auth::Caller;
use crate::lead;
use crate::verbs::view::item_view;
use crate::verbs::{Call, Failure, choice, given};

/// What a step writes on each item: the kind of its event, its note and its data.
struct Step {
    kind: &'static str,
    note: Option<String>,
    data: Map<String, Value>,
}

/// Record a landing step on every item named, or give their claims back.
///
/// # Errors
/// 400 for an action outside its choices, 409 when no item is named, an item holds no open claim,
/// or the caller is neither the claim's branch nor the project's lead.
pub async fn land(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<LandRequest>,
) -> Result<Json<MovedMany>, Failure> {
    choice("action", Some(&req.action), &LAND_ACTIONS)?;
    if req.ids.is_empty() {
        return Err(Failure::Refused(
            "a landing step names the items it records on".into(),
        ));
    }
    let step = step_of(&req)?;
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let lead = lead::held(&call.tx.conn, &call.slug)
        .await?
        .is_some_and(|l| l.host == call.ctx.host && l.branch.as_deref() == Some(call.branch()));
    let mut rows: Vec<Item> = Vec::new();
    for id in &req.ids {
        let r = call.item(id).await?;
        let Some(claim) = r.claim() else {
            return Err(Failure::Refused(format!("{} holds no claim.", r.id)));
        };
        if r.state != "open" {
            return Err(Failure::Refused(format!("{} is not open.", r.id)));
        }
        if !lead && !holds(&r, &call.ctx) {
            return Err(Failure::Refused(format!(
                "{} is held by {} on {} since {}: only that branch or the project's lead records its landing.",
                r.id, claim.branch, claim.host, claim.since
            )));
        }
        rows.push(r);
    }
    let mut items = Vec::new();
    for r in rows {
        let row = call.tx.update(r.rid, &[]).await?;
        call.tx
            .event(
                &call.slug,
                Some(row.rid),
                step.kind,
                step.note.as_deref(),
                Some(call.branch()),
                Some(&Value::Object(step.data.clone())),
            )
            .await?;
        let fresh = call.tx.fresh(row.rid).await?;
        items.push(item_view(&call.tx.conn, &fresh).await?);
    }
    call.tx.commit().await?;
    Ok(Json(MovedMany { items }))
}

/// The event an action writes. `back` is the release `unclaim --outcome gate` writes.
fn step_of(req: &LandRequest) -> Result<Step, Failure> {
    let mut data = Map::new();
    match req.action.as_str() {
        "queue" => Ok(Step {
            kind: "land_queued",
            note: None,
            data,
        }),
        "gating" => {
            let (Some(batch), Some(base)) =
                (given(req.batch.as_deref()), given(req.base.as_deref()))
            else {
                return Err(Failure::Refused(
                    "a gate run names its batch and its base sha".into(),
                ));
            };
            let members = if req.members.is_empty() {
                &req.ids
            } else {
                &req.members
            };
            data.insert("batch".into(), json!(batch));
            data.insert("base".into(), json!(base));
            data.insert("members".into(), json!(members));
            Ok(Step {
                kind: "land_gating",
                note: Some(format!("base {base}, with {}", members.join(", "))),
                data,
            })
        }
        "failed" => Ok(Step {
            kind: "land_failed",
            note: given(req.log.as_deref()).map(str::to_string),
            data,
        }),
        "conflict" => {
            data.insert("files".into(), json!(req.files));
            Ok(Step {
                kind: "land_conflict",
                note: (!req.files.is_empty()).then(|| req.files.join(", ")),
                data,
            })
        }
        _ => {
            data.insert("outcome".into(), json!("gate"));
            Ok(Step {
                kind: "released",
                note: given(req.reason.as_deref()).map(str::to_string),
                data,
            })
        }
    }
}
