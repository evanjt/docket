//! `remap`: the owner's one-off move of done and dropped items' leading shas onto rewritten commits.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::DatabaseConnection;
use serde_json::json;

use docket_core::api::{RemapRequest, RemapRow, Remapped};
use docket_core::item::Field;
use docket_core::remap::Map;

use crate::auth::Caller;
use crate::store::items;
use crate::verbs::{Call, Failure, require_owner};

const CLOSED: &str = "SELECT * FROM items WHERE project=? AND state IN ('done', 'dropped') \
                      AND resolution IS NOT NULL ORDER BY rid";

/// Each closed item whose resolution leads with an old sha in the map takes the new one, with one
/// `remapped` event. State, turn, waits and parents stay as they are.
///
/// # Errors
/// 403 on an agent's key.
pub async fn remap(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<RemapRequest>,
) -> Result<Json<Remapped>, Failure> {
    require_owner(&caller, "admin remap")?;
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let map = Map::from_pairs(&req.map);
    let mut rows = Vec::new();
    for item in items(&call.tx.conn, CLOSED, vec![call.slug.clone().into()]).await? {
        let Some(r) = item.resolution.as_deref().and_then(|r| map.remap(r)) else {
            continue;
        };
        if !req.dry_run {
            call.tx
                .update(item.rid, &[Field::Resolution(Some(r.resolution.clone()))])
                .await?;
            let data = json!({ "old": r.old, "new": r.new });
            let note = format!("{} -> {}", r.old, r.new);
            call.tx
                .event(
                    &call.slug,
                    Some(item.rid),
                    "remapped",
                    Some(&note),
                    None,
                    Some(&data),
                )
                .await?;
        }
        rows.push(RemapRow {
            id: item.id,
            old: r.old,
            new: r.new,
        });
    }
    call.tx.commit().await?;
    Ok(Json(Remapped {
        dry_run: req.dry_run,
        rows,
    }))
}
