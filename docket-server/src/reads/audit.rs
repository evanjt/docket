//! `docket audit`: where a plan, story, package, concept, idea, group or theme stands.

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Query, State};
use sea_orm::{DatabaseConnection, FromQueryResult};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use docket_core::member::{Neighbours, Tie, members_of, opened_under};
use docket_core::text::{principle_refs, principles};
use docket_core::touch::declared_files;
use docket_core::word::Kind;

use crate::entities::item;
use crate::reads::public::{Kinds, facts, open_members, sql};
use crate::reads::rows::{marks, project_model, rows};
use crate::store;
use crate::verbs::Failure;
use crate::verbs::graph::{concepts_of, package_progress};

#[derive(FromQueryResult)]
struct TieRow {
    rid: i64,
    kind: String,
    to_rid: Option<i64>,
}

/// Every item-to-item link leaving an item of the project.
async fn ties(db: &DatabaseConnection, slug: &str) -> Result<Vec<Tie>, Failure> {
    let rows = TieRow::find_by_statement(sql(
        "SELECT l.rid, l.kind, l.to_rid FROM links l JOIN items i ON i.rid=l.rid \
         WHERE i.project=? AND l.kind IN ('related', 'opened') ORDER BY i.state, i.rid, l.kind, l.to_rid",
        vec![slug.into()],
    ))
    .all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|t| {
            Some(Tie {
                rid: t.rid,
                opened: t.kind == "opened",
                to: t.to_rid?,
            })
        })
        .collect())
}

/// The rids of the project's standing items.
async fn standing_rids(
    db: &DatabaseConnection,
    slug: &str,
    kinds: &Kinds,
) -> Result<HashSet<i64>, Failure> {
    let mut keys = kinds.keys(Kind::Concept);
    keys.extend(kinds.keys(Kind::Idea));
    if keys.is_empty() {
        return Ok(HashSet::new());
    }
    let mut values: Vec<sea_orm::Value> = vec![slug.into()];
    values.extend(keys.iter().map(|k| k.clone().into()));
    let found: Vec<i64> = store::column(
        db,
        &format!(
            "SELECT rid FROM items WHERE project=? AND key IN ({})",
            marks(keys.len())
        ),
        values,
    )
    .await?;
    Ok(found.into_iter().collect())
}

/// What belongs to a concept or central idea: tied to it either way, and all opened under that.
///
/// # Errors
/// The database.
pub async fn members(
    db: &DatabaseConnection,
    slug: &str,
    kinds: &Kinds,
    rid: i64,
) -> Result<Vec<i64>, Failure> {
    let standing = standing_rids(db, slug, kinds).await?;
    let ties = ties(db, slug).await?;
    let mut out: Vec<i64> = members_of(&ties, &standing, rid).into_iter().collect();
    out.sort_unstable();
    Ok(out)
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
    #[serde(default)]
    orphans: bool,
}

/// The rows an audit reads and what it says about its target, for the client to section and print.
///
/// # Errors
/// 400 without an id, a group, a theme or orphans; 404 for an unknown project or item.
pub async fn audit(
    State(db): State<DatabaseConnection>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Value>, Failure> {
    let model = project_model(&db, &q.project).await?;
    let kinds = Kinds::of(&model);
    let mut target = None;
    let mut breakdown = Map::new();
    let found = if q.orphans {
        orphans(&db, &q.project, &kinds).await?
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
            "SELECT * FROM items WHERE project=? AND theme ILIKE ? ESCAPE '' ORDER BY state, rid",
            vec![q.project.clone().into(), format!("%{t}%").into()],
        )
        .await?
    } else if let Some(id) = &q.id {
        let t = store::item_of(&db, &q.project, id).await?;
        let found = under(&db, &q.project, &kinds, &t, &mut breakdown).await?;
        target = Some(t);
        found
    } else {
        return Err(Failure::Invalid(
            "audit takes an id, --group, --theme or --orphans.".into(),
        ));
    };
    let counts = open_members(&db, &q.project).await?;
    let listed: Vec<Value> = found
        .iter()
        .map(|r| listed_row(r, &kinds, counts.get(&r.rid).copied().unwrap_or(0)))
        .collect();
    let mut out = json!({ "rows": listed, "breakdown": breakdown });
    if let Some(t) = &target {
        let about = about_target(&db, &q.project, &kinds, t, &counts).await?;
        if let (Value::Object(o), Value::Object(a)) = (&mut out, about) {
            o.extend(a);
        }
    }
    Ok(Json(out))
}

/// One row as the audit lists it.
fn listed_row(r: &item::Model, kinds: &Kinds, open: u64) -> Value {
    let kind = kinds.kind(&r.key);
    let open = if kind == Kind::Package && r.state == "open" {
        open
    } else {
        0
    };
    json!({
        "id": r.id, "title": r.title, "state": r.state, "kind": kind.as_str(),
        "word": docket_core::word::word(&facts(r, kind), open),
        "claim_branch": r.claim_branch, "wait_ref": r.wait_ref, "resolution": r.resolution,
    })
}

/// Open work items, questions and investigations that belong to no concept, oldest first.
async fn orphans(
    db: &DatabaseConnection,
    slug: &str,
    kinds: &Kinds,
) -> Result<Vec<item::Model>, Failure> {
    let con = kinds.keys(Kind::Concept);
    if con.is_empty() {
        return Ok(Vec::new());
    }
    let project = store::project(db, slug).await?;
    let work: Vec<String> = project
        .rules
        .keys
        .iter()
        .filter(|s| matches!(s.kind, Kind::Work | Kind::Decision | Kind::Research))
        .map(|s| s.key.clone())
        .collect();
    let mut values: Vec<sea_orm::Value> = vec![slug.into()];
    values.extend(work.iter().map(|k| k.clone().into()));
    let open = rows(
        db,
        &format!(
            "SELECT * FROM items WHERE project=? AND key IN ({}) AND state='open' ORDER BY opened_at, rid",
            marks(work.len())
        ),
        values,
    )
    .await?;
    let index = Neighbours::new(&all_ties(db).await?);
    let concepts: HashSet<i64> = store::column(
        db,
        &format!("SELECT rid FROM items WHERE key IN ({})", marks(con.len())),
        con.into_iter().map(Into::into).collect(),
    )
    .await?
    .into_iter()
    .collect();
    Ok(open
        .into_iter()
        .filter(|r| index.concepts_of(&concepts, r.rid).is_empty())
        .collect())
}

/// Every item-to-item link in the database, whichever project holds its ends.
async fn all_ties(db: &DatabaseConnection) -> Result<Vec<Tie>, Failure> {
    let rows = TieRow::find_by_statement(sql(
        "SELECT rid, kind, to_rid FROM links WHERE kind IN ('related', 'opened') AND to_rid IS NOT NULL",
        vec![],
    ))
    .all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|t| {
            Some(Tie {
                rid: t.rid,
                opened: t.kind == "opened",
                to: t.to_rid?,
            })
        })
        .collect())
}

/// The rows under an id: a standing item's members, or what it opened at any depth, by rid.
async fn under(
    db: &DatabaseConnection,
    slug: &str,
    kinds: &Kinds,
    t: &docket_core::item::Item,
    breakdown: &mut Map<String, Value>,
) -> Result<Vec<item::Model>, Failure> {
    let kind = kinds.kind(&t.key);
    let ties = ties(db, slug).await?;
    if kind.is_standing() {
        let standing = standing_rids(db, slug, kinds).await?;
        let mine: Vec<i64> = members_of(&ties, &standing, t.rid).into_iter().collect();
        let found = by_rid_chunks(db, &mine).await?;
        standing_breakdown(db, kinds, t, &found, &ties, &standing, breakdown).await?;
        return Ok(found);
    }
    let mut rids: Vec<i64> = opened_under(&ties, t.rid).into_iter().collect();
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

/// Where a concept's or idea's members sit: under which plans and packages, and how many they share
/// with the others of its kind.
async fn standing_breakdown(
    db: &DatabaseConnection,
    kinds: &Kinds,
    t: &docket_core::item::Item,
    found: &[item::Model],
    ties: &[Tie],
    standing: &HashSet<i64>,
    breakdown: &mut Map<String, Value>,
) -> Result<(), Failure> {
    let mine: HashSet<i64> = found.iter().map(|r| r.rid).collect();
    let by_rid: HashMap<i64, &item::Model> = found.iter().map(|r| (r.rid, r)).collect();
    for (kind, label) in [(Kind::Audit, "by plan"), (Kind::Package, "by package")] {
        let mut parts: Vec<(i64, Value)> = Vec::new();
        for r in found.iter().filter(|r| kinds.kind(&r.key) == kind) {
            let below: Vec<i64> = opened_under(ties, r.rid)
                .into_iter()
                .filter(|x| mine.contains(x))
                .collect();
            let open = below.iter().filter(|x| by_rid[x].state == "open").count();
            parts.push((r.num, json!([r.id, below.len(), open])));
        }
        parts.sort_by_key(|p| p.0);
        if !parts.is_empty() {
            breakdown.insert(
                label.into(),
                Value::Array(parts.into_iter().map(|p| p.1).collect()),
            );
        }
    }
    let same = kinds.kind(&t.key);
    let peers: Vec<i64> = standing_of_kind(db, &t.project, kinds, same)
        .await?
        .into_iter()
        .filter(|r| *r != t.rid)
        .collect();
    let index: Vec<HashSet<i64>> = peers
        .iter()
        .map(|p| members_of(ties, standing, *p))
        .collect();
    let shared = mine
        .iter()
        .filter(|x| index.iter().any(|m| m.contains(x)))
        .count();
    if shared > 0 {
        breakdown.insert("shared".into(), json!(shared));
        breakdown.insert("of".into(), json!(same.as_str()));
    }
    Ok(())
}

async fn standing_of_kind(
    db: &DatabaseConnection,
    slug: &str,
    kinds: &Kinds,
    kind: Kind,
) -> Result<Vec<i64>, Failure> {
    let keys = kinds.keys(kind);
    let mut values: Vec<sea_orm::Value> = vec![slug.into()];
    values.extend(keys.iter().map(|k| k.clone().into()));
    Ok(store::column(
        db,
        &format!(
            "SELECT rid FROM items WHERE project=? AND key IN ({})",
            marks(keys.len())
        ),
        values,
    )
    .await?)
}

/// The target itself, its principles and what cites each, and the ideas, stories and concepts bound to it.
async fn about_target(
    db: &DatabaseConnection,
    slug: &str,
    kinds: &Kinds,
    t: &docket_core::item::Item,
    counts: &HashMap<i64, u64>,
) -> Result<Value, Failure> {
    let kind = kinds.kind(&t.key);
    let open = counts.get(&t.rid).copied().unwrap_or(0);
    let model = rows(db, "SELECT * FROM items WHERE rid=?", vec![t.rid.into()]).await?;
    let word = model
        .first()
        .map(|m| listed_row(m, kinds, open)["word"].clone())
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
    Ok(json!({
        "target": {
            "id": t.id, "title": t.title, "word": word, "kind": kind.as_str(),
        },
        "principles": principles(&t.body),
        "served": served,
        "bound": bound(db, slug, kinds, t).await?,
    }))
}

/// `[[kind, [{id, title}]]]` for the ideas, stories and concepts tied to the target, or none at all
/// when no idea, story or concept is tied to it and it belongs to no concept. The links leaving the
/// target come first, by kind and target, then those reaching it, by link.
async fn bound(
    db: &DatabaseConnection,
    slug: &str,
    kinds: &Kinds,
    t: &docket_core::item::Item,
) -> Result<Value, Failure> {
    let near = rows(
        db,
        "SELECT i.* FROM links l JOIN items i ON i.rid = CASE WHEN l.rid=? THEN l.to_rid ELSE l.rid END \
         WHERE (l.rid=? OR l.to_rid=?) AND l.kind IN ('related', 'opened') \
         ORDER BY l.rid<>?, CASE WHEN l.rid=? THEN l.kind END, CASE WHEN l.rid=? THEN l.to_rid END, l.id",
        vec![
            t.rid.into(),
            t.rid.into(),
            t.rid.into(),
            t.rid.into(),
            t.rid.into(),
            t.rid.into(),
        ],
    )
    .await?;
    let mut out: Vec<(Kind, Vec<(String, String)>)> = Vec::new();
    for kind in [Kind::Idea, Kind::Story, Kind::Concept] {
        let keys = kinds.keys(kind);
        let mut seen = HashSet::new();
        let list = near
            .iter()
            .filter(|r| keys.contains(&r.key) && r.rid != t.rid && seen.insert(r.rid))
            .map(|r| (r.id.clone(), r.title.clone()))
            .collect();
        out.push((kind, list));
    }
    if out.iter().all(|(_, l)| l.is_empty()) {
        out.clear();
    }
    let project = store::project(db, slug).await?;
    let cons = concepts_of(db, &project, t.rid).await?;
    if !cons.is_empty() {
        if !out.iter().any(|(k, _)| *k == Kind::Concept) {
            out.push((Kind::Concept, Vec::new()));
        }
        for c in cons {
            let entry = out.iter_mut().find(|(k, _)| *k == Kind::Concept);
            if let Some((_, list)) = entry
                && !list.iter().any(|(id, _)| *id == c)
            {
                let row = store::item_of(db, slug, &c).await?;
                list.push((row.id, row.title));
            }
        }
    }
    Ok(json!(
        out.into_iter()
            .map(|(k, l)| json!([
                k.as_str(),
                l.into_iter()
                    .map(|(id, title)| json!({"id": id, "title": title}))
                    .collect::<Vec<_>>()
            ]))
            .collect::<Vec<_>>()
    ))
}
