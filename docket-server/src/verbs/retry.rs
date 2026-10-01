//! `retry`: an item the loop parked or sent back, given a fresh start.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::DatabaseConnection;
use serde_json::json;

use docket_core::api::{Moved, RetryRequest};
use docket_core::rules;

use crate::auth::Caller;
use crate::verbs::view::item_view;
use crate::verbs::{Call, Failure, given};

/// Back to the agents when it was the owner's turn, its bounces forgotten either way: the event carries
/// the retry mark the loop counts from.
///
/// # Errors
/// 409 when the item is closed, held, or the owner's turn with no note.
pub async fn retry(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<RetryRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let note = given(req.note.as_deref());
    let cols = rules::retry(&r, note)?;
    let row = if cols.is_empty() {
        r.clone()
    } else {
        call.tx.update(r.rid, &cols).await?
    };
    let kind = if cols.is_empty() { "edited" } else { "replied" };
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            kind,
            Some(note.unwrap_or("retry")),
            Some(call.branch()),
            Some(&json!({ "retry": true })),
        )
        .await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

#[cfg(test)]
#[path = "../tests/retry.rs"]
mod tests;
