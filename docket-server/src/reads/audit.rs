//! `docket audit`: where a plan, story, package, idea, group, theme or area stands.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use docket_core::member::{Tie, descendants};
use docket_core::text::{principle_refs, principles};
use docket_core::touch::declared_files;
use docket_core::word::Kind;

use crate::entities::item;
use crate::reads::public::{Members, member_counts, word_of};
use crate::reads::rows::{marks, project_model, rows};
use crate::store;
use crate::verbs::Failure;
use crate::verbs::areas;
use crate::verbs::graph::{package_progress, project_ties};
use docket_core::word::kind_of_type;

/// Every tie of the project, as the server's graph reads it.
async fn ties(db: &DatabaseConnection, slug: &str) -> Result<Vec<Tie>, Failure> {
    Ok(project_ties(db, slug).await?)
}

/// Rows by rid, read 500 rids at a time in rid order, each batch by key and number.
///
/// # Errors
/// The database.
pub async fn by_rid_chunks(
    db: &DatabaseConnection,
    rids: &[i64],
) -> Result<Vec<item::Model>, Failure> {
    let mut sorted = rids.to_vec();
    sorted.sort_unstable();
    let mut out = Vec::new();
    for part in sorted.chunks(500) {
        let values = part.iter().map(|r| (*r).into()).collect();
        let text = format!(
            "SELECT * FROM items WHERE rid IN ({}) ORDER BY key, num",
            marks(part.len())
        );
        out.extend(rows(db, &text, values).await?);
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct AuditQuery {
    project: String,
    id: Option<String>,
    group: Option<String>,
    theme: Option<String>,
    area: Option<String>,
}

/// The rows an audit reads and what it says about its target, for the client to section and print.
///
/// # Errors
/// 400 without an id, a group, a theme or an area; 404 for an unknown project, item or area.
pub async fn audit(
    State(db): State<DatabaseConnection>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Value>, Failure> {
    project_model(&db, &q.project).await?;
    let mut target = None;
    let mut breakdown = Map::new();
    let mut area = Value::Null;
    let found = if let Some(name) = &q.area {
        let listed = areas::listed(&db, &q.project).await?;
        let Some(id) = listed.id(name) else {
            return Err(Failure::NotFound(format!(
                "{name} is not an area here: the areas are {}",
                listed
                    .all()
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        };
        if let Some((_, a)) = listed.rows.iter().find(|(i, _)| *i == id) {
            area = json!({ "name": a.name, "description": a.description, "priority": a.priority });
        }
        rows(
            &db,
            "SELECT * FROM items WHERE project=? AND area_id=? ORDER BY state, rid",
            vec![q.project.clone().into(), id.into()],
        )
        .await?
    } else if let Some(g) = &q.group {
        rows(
            &db,
            "SELECT * FROM items WHERE project=? AND group_name=? ORDER BY state, rid",
            vec![q.project.clone().into(), g.clone().into()],
        )
        .await?
    } else if let Some(t) = &q.theme {
        rows(
            &db,
            "SELECT * FROM items WHERE project=? AND lower(theme)=lower(?) ORDER BY state, rid",
            vec![q.project.clone().into(), t.clone().into()],
        )
        .await?
    } else if let Some(id) = &q.id {
        let t = store::item_of(&db, &q.project, id).await?;
        let found = under(&db, &q.project, &t, &mut breakdown).await?;
        target = Some(t);
        found
    } else {
        return Err(Failure::Invalid(
            "audit takes an id, --group, --theme or --area.".into(),
        ));
    };
    let counts = member_counts(&db, &q.project).await?;
    let listed: Vec<Value> = found
        .iter()
        .map(|r| listed_row(r, counts.get(&r.rid).copied().unwrap_or_default()))
        .collect();
    let mut out = json!({ "rows": listed, "breakdown": breakdown, "area": area });
    if let Some(t) = &target {
        let about = about_target(&db, &q.project, t, &counts).await?;
        if let (Value::Object(o), Value::Object(a)) = (&mut out, about) {
            o.extend(a);
        }
    }
    Ok(Json(out))
}

/// One row as the audit lists it.
fn listed_row(r: &item::Model, members: Members) -> Value {
    let kind = kind_of_type(&r.item_type);
    json!({
        "id": r.id, "title": r.title, "state": r.state, "kind": kind.as_str(),
        "word": word_of(
            &r.state,
            r.claim_branch.as_deref(),
            r.wait_on.as_deref(),
            r.turn.as_deref(),
            kind,
            members,
        ),
        "claim_branch": r.claim_branch, "wait_ref": r.wait_ref, "resolution": r.resolution,
    })
}

/// The rows under an id: its children at any depth, by rid.
async fn under(
    db: &DatabaseConnection,
    slug: &str,
    t: &docket_core::item::Item,
    breakdown: &mut Map<String, Value>,
) -> Result<Vec<item::Model>, Failure> {
    let kind = t.item_type.kind();
    let ties = ties(db, slug).await?;
    let mut rids: Vec<i64> = descendants(&ties, t.rid).into_iter().collect();
    rids.sort_unstable();
    let mut found = Vec::new();
    for rid in rids {
        found.extend(rows(db, "SELECT * FROM items WHERE rid=?", vec![rid.into()]).await?);
    }
    if kind == Kind::Package {
        breakdown.insert("progress".into(), json!(package_progress(db, t.rid).await?));
        breakdown.insert("touches".into(), json!(declared_files(&t.body)));
    }
    Ok(found)
}

/// The target itself, its principles and what cites each and its area.
async fn about_target(
    db: &DatabaseConnection,
    slug: &str,
    t: &docket_core::item::Item,
    counts: &HashMap<i64, Members>,
) -> Result<Value, Failure> {
    let kind = t.item_type.kind();
    let members = counts.get(&t.rid).copied().unwrap_or_default();
    let model = rows(db, "SELECT * FROM items WHERE rid=?", vec![t.rid.into()]).await?;
    let word = model
        .first()
        .map(|m| listed_row(m, members)["word"].clone())
        .unwrap_or_default();
    let mut served: Map<String, Value> = Map::new();
    let citing = rows(
        db,
        "SELECT * FROM items WHERE project=? AND body ILIKE ? ESCAPE '' ORDER BY state, rid",
        vec![slug.into(), format!("%{}#%", t.id).into()],
    )
    .await?;
    for r in &citing {
        for n in principle_refs(&r.body, &t.id) {
            let entry = served.entry(n.to_string()).or_insert_with(|| json!([]));
            if let Value::Array(list) = entry {
                list.push(json!({ "id": r.id, "state": r.state }));
            }
        }
    }
    let listed = areas::listed(db, slug).await?;
    let area = model
        .first()
        .and_then(|m| listed.rows.iter().find(|(id, _)| Some(*id) == m.area_id))
        .map_or(Value::Null, |(_, a)| {
            json!({ "name": a.name, "description": a.description, "priority": a.priority })
        });
    Ok(json!({
        "target": {
            "id": t.id, "title": t.title, "word": word, "kind": kind.as_str(),
        },
        "area": area,
        "principles": principles(&t.body),
        "served": served,
        "labels": crate::verbs::labels::carried(db, t.rid).await?,
    }))
}
