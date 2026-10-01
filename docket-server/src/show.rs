use axum::Json;
use axum::extract::{Path, Query, State};
use sea_orm::sea_query::NullOrdering;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, Order, QueryFilter, QueryOrder};
use serde::Deserialize;
use serde_json::{Value, json};

use docket_core::word::Kind;

use crate::entities::{item, link};
use crate::reads::public::{Failure, Kinds, id_of, internal, item_of, project_of, public};

#[derive(Deserialize)]
pub struct InProject {
    project: String,
}

/// The items a package opened, the direct ones only.
async fn members(db: &DatabaseConnection, rid: i64) -> Result<Vec<item::Model>, sea_orm::DbErr> {
    let rids: Vec<i64> = link::Entity::find()
        .filter(link::Column::ToRid.eq(rid))
        .filter(link::Column::Kind.eq("opened"))
        .all(db)
        .await?
        .into_iter()
        .map(|l| l.rid)
        .collect();
    item::Entity::find()
        .filter(item::Column::Rid.is_in(rids))
        .all(db)
        .await
}

/// One item by its id within a project, in the shape `docket show --json` prints.
///
/// # Errors
/// 404 when the project or the item is unknown.
pub async fn show(
    State(db): State<DatabaseConnection>,
    Path(id): Path<String>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    let project = project_of(&db, &q.project).await?;
    let row = item_of(&db, &q.project, &id).await?;
    let kind = Kinds::of(&project).kind(&row.key);
    let members = if kind == Kind::Package {
        Some(members(&db, row.rid).await.map_err(|e| internal(&e))?)
    } else {
        None
    };
    let open_members = members
        .iter()
        .flatten()
        .filter(|m| m.state == "open")
        .count() as u64;

    let (mut related, mut opened, mut cites) = (Vec::new(), Vec::new(), Vec::new());
    let links = link::Entity::find()
        .filter(link::Column::Rid.eq(row.rid))
        .order_by_asc(link::Column::Kind)
        .order_by_with_nulls(link::Column::ToRid, Order::Asc, NullOrdering::First)
        .order_by_with_nulls(link::Column::ToPath, Order::Asc, NullOrdering::First)
        .order_by_with_nulls(link::Column::ToLine, Order::Asc, NullOrdering::First)
        .order_by_asc(link::Column::Id)
        .all(&db)
        .await
        .map_err(|e| internal(&e))?;
    for l in links {
        match (l.kind.as_str(), l.to_rid) {
            ("related", Some(rid)) => {
                related.extend(id_of(&db, rid).await.map_err(|e| internal(&e))?);
            }
            ("opened", Some(rid)) => {
                opened.extend(id_of(&db, rid).await.map_err(|e| internal(&e))?);
            }
            _ => cites.push(json!({ "path": l.to_path, "line": l.to_line, "kind": l.kind })),
        }
    }

    let mut out = public(&db, kind, row, open_members, None)
        .await
        .map_err(|e| internal(&e))?;
    out.insert("related".into(), json!(related));
    out.insert("opened".into(), json!(opened));
    out.insert("cites".into(), json!(cites));
    if let Some(members) = &members {
        let live = members.iter().filter(|m| m.claim_branch.is_some()).count();
        let done = members.len() - members.iter().filter(|m| m.state == "open").count();
        out.insert(
            "progress".into(),
            json!({ "done": done, "total": members.len(), "live": live }),
        );
    }
    Ok(Json(Value::Object(out)))
}
