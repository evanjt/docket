//! Each item's type and priority column, read from its key's kind and its tags: the type is the
//! key's own when it is one of the fixed keys, else by the kind the project gave the key
//! (`ItemType::of_stored`); the priority is the tier its tags named, which then leave the tags. An
//! item whose file changes is left for the dump.

use sea_orm::{ConnectionTrait, DbErr};

use docket_core::rows::KeySpec;
use docket_core::word::{ItemType, Kind, priority, without_priority};

use crate::statement;

/// Sets the type and priority of every item from its key and tags.
///
/// # Errors
/// The database refuses a statement.
pub async fn assign<C: ConnectionTrait>(c: &C) -> Result<(), DbErr> {
    for p in c
        .query_all_raw(statement(
            "SELECT slug, keys FROM projects ORDER BY slug",
            vec![],
        ))
        .await?
    {
        let slug: String = p.try_get_by_index(0)?;
        let keys: serde_json::Value = p.try_get_by_index(1)?;
        let keys: Vec<KeySpec> = serde_json::from_value(keys).unwrap_or_default();
        let rows = c
            .query_all_raw(statement(
                "SELECT rid, key, tags FROM items WHERE project=? ORDER BY rid",
                vec![slug.clone().into()],
            ))
            .await?;
        for r in rows {
            let rid: i64 = r.try_get_by_index(0)?;
            let key: String = r.try_get_by_index(1)?;
            let tags: serde_json::Value = r.try_get_by_index(2)?;
            let tags: Vec<String> = serde_json::from_value(tags).unwrap_or_default();
            let kind = keys
                .iter()
                .find(|k| k.key == key)
                .map_or(Kind::Work, |k| k.kind);
            let kept = without_priority(&tags);
            c.execute_raw(statement(
                "UPDATE items SET type=?, priority=?, tags=? WHERE rid=?",
                vec![
                    ItemType::of_stored(&key, kind).as_str().into(),
                    priority(&tags).into(),
                    serde_json::json!(kept).into(),
                    rid.into(),
                ],
            ))
            .await?;
            c.execute_raw(statement(
                "INSERT INTO pending_dump (rid, project) VALUES (?, ?) ON CONFLICT DO NOTHING",
                vec![rid.into(), slug.clone().into()],
            ))
            .await?;
        }
    }
    Ok(())
}
