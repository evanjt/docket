//! `new` and `add`: what opens an item.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};

use docket_core::api::{AddRequest, Decided, NewRequest, Opened};
use docket_core::item::{Field, Item};
use docket_core::project::require_fileable;
use docket_core::rules::{COMPLEXITIES, default_turn};
use docket_core::word::{ItemType, Kind, PRIORITIES};

use crate::auth::Caller;
use crate::store::NewItem;
use crate::verbs::areas;
use crate::verbs::graph::{decided_like, refuse_later, release_list};
use crate::verbs::releases::release_id;
use crate::verbs::view::item_view;
use crate::verbs::{Call, Failure, TURNS, chars, choice, require_owner};

/// # Errors
/// 403 on an agent's key whose branch holds no plan, investigation or question, 409 on an unknown
/// key or a tier outside the four.
pub async fn new(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<NewRequest>,
) -> Result<Json<Opened>, Failure> {
    let call = Call::begin(&db, &caller, &req.common).await?;
    let from = filing_from(&call, &caller).await?;
    open_item(call, req, None, from).await.map(Json)
}

/// A side finding filed as a low-priority ticket.
///
/// # Errors
/// 403 on an agent's key whose branch holds no plan, investigation or question.
pub async fn add(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<AddRequest>,
) -> Result<Json<Opened>, Failure> {
    let call = Call::begin(&db, &caller, &req.common).await?;
    let from = filing_from(&call, &caller).await?;
    let key = match req.key {
        Some(k) => k,
        None => ItemType::Bug.key().to_string(),
    };
    let as_new = NewRequest {
        common: req.common,
        key,
        title: req.title,
        body: req.body,
        turn: None,
        complexity: None,
        priority: Some("low".to_string()),
        theme: None,
        release: req.release,
        group: None,
        repo: None,
        area: req.area,
        parent: req.parent,
    };
    open_item(call, as_new, req.from, from).await.map(Json)
}

/// What a job's filing springs from: the plan, investigation or question its branch holds a claim
/// on. A job building a ticket files nothing and reports what it saw instead, and the owner files
/// from nothing.
async fn filing_from(call: &Call, caller: &Caller) -> Result<Option<Item>, Failure> {
    if caller.owner {
        return Ok(None);
    }
    let held = crate::store::items(
        &call.tx.conn,
        "SELECT * FROM items WHERE project=? AND state='open' AND rid IN \
         (SELECT rid FROM assignments WHERE ended_at IS NULL AND kind='claim' AND branch=?)",
        vec![call.slug.clone().into(), call.branch().into()],
    )
    .await?;
    match held
        .into_iter()
        .find(|r| files_findings(r.item_type.kind()))
    {
        Some(r) => Ok(Some(r)),
        None => require_owner(caller, "new").map(|()| None),
    }
}

/// A kind whose work is finding what to do: a plan, an audit of one, an investigation or a question.
fn files_findings(kind: Kind) -> bool {
    !matches!(kind, Kind::Work | Kind::Concept | Kind::Idea)
}

/// The labels `--theme NAME`, `--group NAME` and `--repo PATH` give a new item: the theme as it is
/// named, the group as its group label, the repository as its repo label. A blank name gives none.
fn given_labels(
    theme: Option<&str>,
    group: Option<&str>,
    repo: Option<&str>,
) -> Result<Vec<String>, Failure> {
    fn named(v: Option<&str>) -> Option<&str> {
        v.map(str::trim).filter(|v| !v.is_empty())
    }
    let theme = named(theme).map(str::to_string);
    let group = named(group).map(docket_core::label::of_group);
    let repo = docket_core::label::of_repo(repo.unwrap_or_default())?;
    Ok(theme.into_iter().chain(group).chain(repo).collect())
}

/// The plan a new item is filed under, which must be open.
async fn parent_plan(call: &Call, given: Option<&str>) -> Result<Option<Item>, Failure> {
    let Some(id) = given.map(str::trim).filter(|p| !p.is_empty()) else {
        return Ok(None);
    };
    let plan = call.item(id).await?;
    if !plan.item_type.kind().is_plan() {
        return Err(Failure::Refused(format!(
            "{} is not a plan, and only a plan holds children.",
            plan.id
        )));
    }
    if plan.state != "open" {
        return Err(Failure::Refused(format!(
            "{} is {}, and a closed plan holds no open children.",
            plan.id, plan.state
        )));
    }
    Ok(Some(plan))
}

/// The area a new item is filed in: its plan's, or the one it names. Naming another than its plan's
/// is refused, and so is naming none, which lists the project's areas.
async fn filed_area(call: &Call, given: Option<&str>, plan: Option<&Item>) -> Result<i64, Failure> {
    let named = match given {
        Some(g) => Some(areas::area_id(&call.tx.conn, &call.slug, g).await?),
        None => None,
    };
    let Some(plan) = plan else {
        return match named.flatten() {
            Some(id) => {
                areas::open_into(&call.tx.conn, &call.slug, id).await?;
                Ok(id)
            }
            None => Err(Failure::Refused(areas::unfiled(
                &areas::listed(&call.tx.conn, &call.slug).await?,
            ))),
        };
    };
    if let Some(named) = named
        && named != plan.area_id
    {
        let theirs = areas::name_of(&call.tx.conn, plan.area_id).await?;
        let theirs = match theirs {
            Some(n) => format!("its area, {n}"),
            None => format!("its area, and {} has none", plan.id),
        };
        return Err(Failure::Refused(format!(
            "an item under {} takes {theirs}: leave out --area",
            plan.id
        )));
    }
    plan.area_id.ok_or_else(|| {
        Failure::Refused(format!(
            "{} has no area, so nothing can be filed under it: give it one with docket edit {} --area NAME",
            plan.id, plan.id
        ))
    })
}

/// A new item put under its plan, refused when it ships after the plan it would hold.
async fn file_under(call: &mut Call, plan: &Item, r: &Item) -> Result<Item, Failure> {
    let listed = release_list(&call.tx.conn, &call.slug).await?;
    refuse_later(&listed, plan, r, call.ctx.force)?;
    let row = call
        .tx
        .update(r.rid, &[Field::ParentRid(Some(plan.rid))])
        .await?;
    let note = format!("parent {}", plan.id);
    call.tx
        .event(&call.slug, Some(r.rid), "edited", Some(&note), None, None)
        .await?;
    Ok(row)
}

async fn open_item(
    mut call: Call,
    req: NewRequest,
    seen_by: Option<String>,
    from: Option<Item>,
) -> Result<Opened, Failure> {
    let key = req.key.to_uppercase();
    let item_type = require_fileable(&key)?;
    choice("turn", req.turn.as_deref(), &TURNS)?;
    choice("complexity", req.complexity.as_deref(), &COMPLEXITIES)?;
    choice("priority", req.priority.as_deref(), &PRIORITIES)?;
    let turn = match req.turn {
        Some(t) => t,
        None => default_turn(item_type).to_string(),
    };
    let body = req.body.unwrap_or_default();
    let priority = req.priority.as_deref().unwrap_or("normal");
    if turn == "user" {
        crate::verbs::turn::ensure_owner_room(&call).await?;
    }
    let plan = parent_plan(&call, req.parent.as_deref()).await?;
    let release_id = match (req.release.as_deref(), &plan) {
        (Some(given), _) => release_id(&call.tx.conn, &call.slug, given).await?,
        (None, Some(plan)) => plan.release_id,
        (None, None) => None,
    };
    let area_id = filed_area(&call, req.area.as_deref(), plan.as_ref()).await?;
    let title = req.title.trim().to_string();
    let labels = given_labels(
        req.theme.as_deref(),
        req.group.as_deref(),
        req.repo.as_deref(),
    )?;
    let num = call.tx.next_num(&call.slug, &key).await?;
    let r = call
        .tx
        .insert(NewItem {
            project: call.slug.clone(),
            key: key.clone(),
            num,
            title: title.clone(),
            body: body.trim_end_matches('\n').to_string(),
            complexity: req.complexity,
            release_id,
            area_id,
            item_type,
            priority: priority.to_string(),
            labels,
        })
        .await?;
    let mut data = serde_json::Map::new();
    if let Some(job) = seen_by {
        data.insert("observed_by".into(), json!(job));
    }
    if turn == "user" {
        data.insert("turn".into(), json!(turn));
    }
    let data = Some(Value::Object(data));
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "opened",
            Some(&chars(&title, 120)),
            Some(call.branch()),
            data.as_ref(),
        )
        .await?;
    let mut r = call.tx.fresh(r.rid).await?;
    if let Some(plan) = &plan {
        r = file_under(&mut call, plan, &r).await?;
    }
    if let Some(from) = &from {
        call.tx.set_link(r.rid, "origin", from.rid, false).await?;
        r = call.tx.fresh(r.rid).await?;
    }
    let decided = decided_like(&call.tx.conn, &call.project, &r).await?;
    let out = Opened {
        item: item_view(&call.tx.conn, &r).await?,
        kind: item_type.kind().as_str().to_string(),
        decided_like: decided
            .into_iter()
            .map(|t| Decided {
                id: t.id,
                title: t.title,
                decision: t.decision.unwrap_or_default(),
            })
            .collect(),
    };
    call.tx.commit().await?;
    Ok(out)
}
