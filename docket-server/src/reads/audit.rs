//! `docket audit`: where a plan, story, package, idea, group, label or area stands.

use std::collections::{BTreeMap, HashMap};

use axum::Json;
use axum::extract::{Query, State};
use sea_orm::DatabaseConnection;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use docket_core::assignment::Held;
use docket_core::label;
use docket_core::member::{Tie, descendants};
use docket_core::stall::Wait;
use docket_core::text::{principle_refs, principles};
use docket_core::touch::declared_files;
use docket_core::word::Kind;

use crate::entities::item;
use crate::reads::public::{Members, member_counts, word_of};
use crate::reads::rows::{marks, project_model, rows};
use crate::store;
use crate::verbs::Failure;
use crate::verbs::graph::{package_progress, project_ties, waits};
use crate::verbs::{areas, labels};
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
    /// The items carrying a label; `theme` is the name it had.
    #[serde(alias = "theme")]
    label: Option<String>,
    area: Option<String>,
}

/// The rows an audit reads and what it says about its target, for the client to section and print.
///
/// # Errors
/// 400 without an id, a group, a label or an area; 404 for an unknown project, item or area.
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
    } else if let Some(name) = q
        .group
        .as_deref()
        .map(label::of_group)
        .or_else(|| q.label.clone())
    {
        let carrying: Vec<i64> = labels::carrying(&db, &q.project, &name)
            .await?
            .into_iter()
            .collect();
        rows(
            &db,
            "SELECT * FROM items WHERE rid = ANY(?) ORDER BY state, rid",
            vec![carrying.into()],
        )
        .await?
    } else if let Some(id) = &q.id {
        let t = store::item_of(&db, &q.project, id).await?;
        let found = under(&db, &q.project, &t, &mut breakdown).await?;
        target = Some(t);
        found
    } else {
        return Err(Failure::Invalid(
            "audit takes an id, --group, --label or --area.".into(),
        ));
    };
    let counts = member_counts(&db, &q.project).await?;
    let rids: Vec<i64> = found.iter().map(|r| r.rid).collect();
    let held = store::held(&db, &rids).await?;
    let waiting = waits(&db, &q.project).await?;
    let listed: Vec<Value> = found
        .iter()
        .map(|r| {
            let members = counts.get(&r.rid).copied().unwrap_or_default();
            listed_row(r, held.get(&r.rid), waiting.get(&r.rid), members)
        })
        .collect();
    let mut out = json!({ "rows": listed, "breakdown": breakdown, "area": area });
    if let Some(t) = &target {
        let about = about_target(&db, &q.project, t, &counts, &waiting).await?;
        if let (Value::Object(o), Value::Object(a)) = (&mut out, about) {
            o.extend(a);
        }
    }
    Ok(Json(out))
}

/// One row as the audit lists it, with its open assignment and what its dependencies hold it on.
fn listed_row(
    r: &item::Model,
    held: Option<&Held>,
    wait: Option<&Wait>,
    members: Members,
) -> Value {
    let kind = kind_of_type(&r.item_type);
    let claim_branch = match held {
        Some(Held::Claim(c)) => Some(c.branch.as_str()),
        _ => None,
    };
    json!({
        "id": r.id, "title": r.title, "state": r.state, "kind": kind.as_str(),
        "word": word_of(&r.state, held, wait.is_some(), kind, members),
        "claim_branch": claim_branch, "wait_ref": wait.map(|w| &w.id), "resolution": r.resolution,
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
    waiting: &BTreeMap<i64, Wait>,
) -> Result<Value, Failure> {
    let kind = t.item_type.kind();
    let members = counts.get(&t.rid).copied().unwrap_or_default();
    let model = rows(db, "SELECT * FROM items WHERE rid=?", vec![t.rid.into()]).await?;
    let word = model
        .first()
        .map(|m| listed_row(m, t.held.as_ref(), waiting.get(&m.rid), members)["word"].clone())
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
