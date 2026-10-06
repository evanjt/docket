use axum::Json;
use axum::extract::{Path, Query, State};
use sea_orm::sea_query::NullOrdering;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, Order, QueryFilter, QueryOrder};
use serde::Deserialize;
use serde_json::{Value, json};

use docket_core::offers::offers as rules_offers;
use docket_core::rules::COMPLEXITIES;
use docket_core::word::{Kind, PRIORITIES};

use crate::entities::{item, link};
use crate::reads::public::{Failure, Members, id_of, internal, item_of, project_of, public};
use crate::store::to_item;
use crate::verbs::graph::{member_count, open_under};
use docket_core::assignment::Held;
use docket_core::word::kind_of_type;

#[derive(Deserialize)]
pub struct InProject {
    project: String,
}

/// An item's children, the direct ones only, by key and number.
async fn children(db: &DatabaseConnection, rid: i64) -> Result<Vec<item::Model>, sea_orm::DbErr> {
    item::Entity::find()
        .filter(item::Column::ParentRid.eq(rid))
        .order_by_asc(item::Column::Key)
        .order_by_asc(item::Column::Num)
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
    project_of(&db, &q.project).await?;
    let row = item_of(&db, &q.project, &id).await?;
    let kind = kind_of_type(&row.item_type);
    let under = children(&db, row.rid).await.map_err(|e| internal(&e))?;
    let members = (kind == Kind::Package).then_some(&under);
    let held = if kind.is_plan() {
        member_count(&db, row.rid).await.map_err(|e| internal(&e))?
    } else {
        Members::default()
    };

    let (mut related, mut origin, mut cites) = (Vec::new(), Vec::new(), Vec::new());
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
            ("origin", Some(rid)) => {
                origin.extend(id_of(&db, rid).await.map_err(|e| internal(&e))?);
            }
            _ => cites.push(json!({ "path": l.to_path, "line": l.to_line, "kind": l.kind })),
        }
    }

    let parent = match row.parent_rid {
        Some(rid) => id_of(&db, rid).await.map_err(|e| internal(&e))?,
        None => None,
    };
    let body = row.body.clone();
    let mut out = public(&db, kind, row, held, None)
        .await
        .map_err(|e| internal(&e))?;
    out.insert("body".into(), json!(body));
    out.insert("related".into(), json!(related));
    out.insert("parent".into(), json!(parent));
    out.insert("origin".into(), json!(origin));
    out.insert(
        "children".into(),
        json!(under.iter().map(|c| c.id.clone()).collect::<Vec<_>>()),
    );
    out.insert("cites".into(), json!(cites));
    if let Some(members) = members {
        let rids: Vec<i64> = members.iter().map(|m| m.rid).collect();
        let live = crate::store::held(&db, &rids)
            .await
            .map_err(|e| internal(&e))?
            .values()
            .filter(|h| matches!(h, Held::Claim(_)))
            .count();
        let done = members.len() - members.iter().filter(|m| m.state == "open").count();
        out.insert(
            "progress".into(),
            json!({ "done": done, "total": members.len(), "live": live }),
        );
    }
    Ok(Json(Value::Object(out)))
}

/// What an item takes now: the verbs the rules accept on it, each with the fields it is refused without and
/// the branch it is sent with, and the priority tiers and complexity levels the verbs that set them accept.
///
/// # Errors
/// 404 when the project or the item is unknown.
pub async fn offers(
    State(db): State<DatabaseConnection>,
    Path(id): Path<String>,
    Query(q): Query<InProject>,
) -> Result<Json<Value>, Failure> {
    project_of(&db, &q.project).await?;
    let row = item_of(&db, &q.project, &id).await?;
    let kind = kind_of_type(&row.item_type);
    let id = row.id.clone();
    let pending = if kind == Kind::Audit {
        open_under(&db, &q.project, row.rid)
            .await
            .map_err(|e| internal(&e))?
            .into_iter()
            .map(|x| x.id)
            .collect()
    } else {
        Vec::new()
    };
    let wait = crate::verbs::graph::wait_of(&db, row.rid)
        .await
        .map_err(|e| internal(&e))?;
    Ok(Json(json!({
        "id": id,
        "verbs": rules_offers(&to_item(row), kind, &pending, wait.as_ref()),
        "priorities": PRIORITIES,
        "levels": COMPLEXITIES,
    })))
}
