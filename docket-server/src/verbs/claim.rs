//! `start`, `release`, `close`, `drop` and `reopen`: the claim and the end of an item.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::{DatabaseConnection, Value};
use serde_json::{Map, Value as Json_, json};

use docket_core::api::{
    CloseRequest, Closed, DropRequest, Dropped, Moved, ReleaseRequest, ReopenRequest, Share,
    StartRequest, Started,
};
use docket_core::item::Item;
use docket_core::rules;
use docket_core::text::{gates_result, opened_ids};
use docket_core::word::Kind;

use crate::auth::Caller;
use crate::store::item_opt;
use crate::verbs::graph::{
    belongs_to_none, is_package, live_overlaps, open_under, package_members, package_of,
    release_waiters, settle_audits,
};
use crate::verbs::view::{brief, item_view, kind};
use crate::verbs::{Call, Failure, ROLES, RUNNERS, choice, given, some_ids};

/// The runner and model a close or release names, for the data it writes.
fn run_facts(runner: Option<&str>, model: Option<&str>) -> Map<String, Json_> {
    let mut data = Map::new();
    if let Some(r) = given(runner) {
        data.insert("runner".into(), json!(r));
    }
    if let Some(m) = given(model) {
        data.insert("model".into(), json!(m));
    }
    data
}

fn data_or_none(data: Map<String, Json_>) -> Option<Json_> {
    if data.is_empty() {
        None
    } else {
        Some(Json_::Object(data))
    }
}

/// Claim an item on a branch.
///
/// # Errors
/// 409 when the rules refuse the claim, or a package still has open tickets.
pub async fn start(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<StartRequest>,
) -> Result<Json<Started>, Failure> {
    choice("runner", req.runner.as_deref(), &RUNNERS)?;
    choice("role", req.role.as_deref(), &ROLES)?;
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    if given(req.job.as_deref()).is_some() && given(req.runner.as_deref()).is_none() {
        return Err(Failure::Refused(format!(
            "a job is named in its runner: docket start {} --runner codex --job {}",
            r.id,
            req.job.unwrap_or_default()
        )));
    }
    let cols = rules::start(&r, &call.ctx)?;
    let review = is_package(&call.project, &r.key);
    if review {
        let held = package_members(&call.tx.conn, r.rid, true).await?;
        if !held.is_empty() {
            return Err(Failure::Refused(format!(
                "{} is built through its tickets, and {} are open: {}. docket next --under {}. It is claimed for its review once none is.",
                r.id,
                held.len(),
                some_ids(&held, 8),
                r.id
            )));
        }
    }
    let mut data = Map::new();
    let role = given(req.role.as_deref()).or(if review { Some("review") } else { None });
    if let Some(role) = role {
        data.insert("role".into(), json!(role));
    }
    data.extend(run_facts(req.runner.as_deref(), req.model.as_deref()));
    let row = claim_row(&mut call, &r, &cols, &req).await?;
    call.tx
        .event(
            &call.slug,
            Some(row.rid),
            "claimed",
            None,
            Some(call.branch()),
            data_or_none(data).as_ref(),
        )
        .await?;
    let out = started(&call, &row, review).await?;
    call.tx.commit().await?;
    Ok(Json(out))
}

/// The claim written only if nobody took the item first.
async fn claim_row(
    call: &mut Call,
    r: &Item,
    cols: &[docket_core::item::Field],
    req: &StartRequest,
) -> Result<Item, Failure> {
    let mut claimed = r.clone();
    claimed.apply(cols);
    let values: Vec<Value> = vec![
        claimed.claim_branch.into(),
        claimed.claim_host.into(),
        claimed.claim_since.into(),
        req.runner.clone().into(),
        req.job.clone().into(),
        req.on.clone().into(),
        call.ctx.now.clone().into(),
        r.rid.into(),
        i64::from(call.ctx.force).into(),
    ];
    let rows = call
        .tx
        .items(
            "UPDATE items SET claim_branch=?, claim_host=?, claim_since=?, claim_runner=?, claim_job=?, \
             claim_on=?, updated_at=? WHERE rid=? AND state='open' AND (claim_branch IS NULL OR ?) RETURNING *",
            values,
        )
        .await?;
    let Some(row) = rows.into_iter().next() else {
        let r2 = call.tx.fresh(r.rid).await?;
        return Err(Failure::Refused(format!(
            "{} was taken by {} on {} a moment ago.",
            r.id,
            r2.claim_branch.unwrap_or_default(),
            r2.claim_host.unwrap_or_default()
        )));
    };
    call.tx.touch(row.rid);
    Ok(row)
}

async fn started(call: &Call, row: &Item, review: bool) -> Result<Started, Failure> {
    let c = &call.tx.conn;
    let shares = live_overlaps(c, &call.slug, row)
        .await?
        .into_iter()
        .map(|(holder, paths)| Share {
            holder: holder.id,
            branch: holder.claim_branch.unwrap_or_default(),
            host: holder.claim_host.unwrap_or_default(),
            paths,
        })
        .collect();
    let group_others =
        match &row.group_name {
            Some(g) => crate::store::column(
                c,
                "SELECT id FROM items WHERE project=? AND group_name=? AND rid<>? AND state='open'",
                vec![call.slug.clone().into(), g.clone().into(), row.rid.into()],
            )
            .await?,
            None => Vec::new(),
        };
    Ok(Started {
        item: item_view(c, &call.project, row).await?,
        kind: kind(&call.project, row).as_str().to_string(),
        review,
        shares,
        package: package_of(c, &call.project, row.rid)
            .await?
            .map(|p| brief(&p)),
        worktree_hint: call.project.worktree_hint.clone(),
        group_others,
        no_concept: belongs_to_none(c, &call.project, row).await?,
    })
}

/// Give a claim back.
///
/// # Errors
/// 409 when the item is not claimed, or held by another branch.
pub async fn release(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<ReleaseRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let cols = rules::release(&r, &call.ctx)?;
    let row = call.tx.update(r.rid, &cols).await?;
    let mut data = run_facts(req.runner.as_deref(), req.model.as_deref());
    if req.bounce {
        data.insert("bounce".into(), json!(true));
    }
    if let Some(rebase) = given(req.rebase.as_deref()) {
        data.insert("rebase".into(), json!(rebase));
    }
    call.tx
        .event(
            &call.slug,
            Some(row.rid),
            "released",
            given(req.note.as_deref()),
            Some(call.branch()),
            data_or_none(data).as_ref(),
        )
        .await?;
    settle_audits(&mut call.tx, &call.project, &[row.rid]).await?;
    let row = call.tx.fresh(row.rid).await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

/// Done, under a sha or what it opened; releases waiters.
///
/// # Errors
/// 409 when the rules refuse it, the item is standing, gated with open work, or a package with open members.
pub async fn close(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<CloseRequest>,
) -> Result<Json<Closed>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let kind = kind(&call.project, &r);
    let resolution = given(req.resolution.as_deref());
    if resolution.is_none() && kind != Kind::Decision {
        return Err(Failure::Refused(format!(
            "close needs a resolution: the sha the work landed as, or what closed it. No branch of {} was found to read one from.",
            r.claim_branch.as_deref().unwrap_or("this item")
        )));
    }
    let cols = rules::close(&r, &call.ctx, resolution, kind)?;
    let resolution = resolution.unwrap_or_default().to_string();
    if kind.is_standing() {
        return Err(Failure::Refused(format!(
            "{} is a {} and stays open for good. Retire it with docket drop {} \"why\" if it no longer holds.",
            r.id,
            kind.as_str(),
            r.id
        )));
    }
    if kind.gated_turn().is_some() {
        let pending = open_under(&call.tx.conn, r.rid).await?;
        if !pending.is_empty() {
            return Err(Failure::Refused(format!(
                "{} opened work that is still open: {}. It closes only when nothing it opened is open; release it and it comes back when they close.",
                r.id,
                some_ids(&pending, 10)
            )));
        }
    }
    let mut data = run_facts(req.runner.as_deref(), req.model.as_deref());
    if given(req.gates.as_deref()).is_some() {
        data.insert("gates".into(), json!(gates_result(req.gates.as_deref())?));
    }
    if kind == Kind::Package {
        let pending = package_members(&call.tx.conn, r.rid, true).await?;
        if !pending.is_empty() {
            return Err(Failure::Refused(format!(
                "{} still holds open members: {}. Close or drop each first.",
                r.id,
                some_ids(&pending, 10)
            )));
        }
    }
    let row = call.tx.update(r.rid, &cols).await?;
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "closed",
            Some(&resolution),
            Some(call.branch()),
            data_or_none(data).as_ref(),
        )
        .await?;
    let mut opened = Vec::new();
    if kind == Kind::Decision {
        let keys: Vec<&str> = call
            .project
            .rules
            .keys
            .iter()
            .map(|s| s.key.as_str())
            .collect();
        for oid in opened_ids(Some(&resolution), &keys) {
            if let Some(t) = item_opt(&call.tx.conn, &call.slug, &oid).await? {
                call.tx.set_link(t.rid, "opened", r.rid, false).await?;
                opened.push(oid);
            }
        }
    }
    let mut released = release_waiters(&mut call.tx, &call.slug, &row, "closed").await?;
    released.extend(settle_audits(&mut call.tx, &call.project, &[r.rid]).await?);
    let mut review_ready = None;
    if let Some(pkg) = package_of(&call.tx.conn, &call.project, r.rid).await?
        && package_members(&call.tx.conn, pkg.rid, true)
            .await?
            .is_empty()
    {
        review_ready = Some(pkg.id);
    }
    let out = Closed {
        item: item_view(&call.tx.conn, &call.project, &row).await?,
        released: released.iter().map(brief).collect(),
        opened,
        review_ready,
    };
    call.tx.commit().await?;
    Ok(Json(out))
}

/// Closed without doing: superseded, archived, will not fix.
///
/// # Errors
/// 409 when the rules refuse it, or a package still holds members.
pub async fn drop(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<DropRequest>,
) -> Result<Json<Dropped>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let sup = match given(req.superseded_by.as_deref()) {
        Some(id) => Some(call.item(id).await?),
        None => None,
    };
    let why = match (given(req.why.as_deref()), &sup) {
        (Some(w), _) => Some(w.to_string()),
        (None, Some(s)) => Some(format!("superseded by {}", s.id)),
        (None, None) => None,
    };
    let cols = rules::drop(&r, &call.ctx, why.as_deref(), sup.as_ref().map(|s| s.rid))?;
    if is_package(&call.project, &r.key) && !call.ctx.force {
        let held = package_members(&call.tx.conn, r.rid, true).await?;
        if !held.is_empty() {
            return Err(Failure::Refused(format!(
                "{} still holds {}. Fold it into another package (docket fold PKn {}), or move or drop its members first.",
                r.id,
                some_ids(&held, 8),
                r.id
            )));
        }
    }
    let row = call.tx.update(r.rid, &cols).await?;
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "dropped",
            why.as_deref(),
            Some(call.branch()),
            None,
        )
        .await?;
    let mut released = release_waiters(&mut call.tx, &call.slug, &row, "dropped").await?;
    released.extend(settle_audits(&mut call.tx, &call.project, &[r.rid]).await?);
    let out = Dropped {
        item: item_view(&call.tx.conn, &call.project, &row).await?,
        released: released.iter().map(brief).collect(),
    };
    call.tx.commit().await?;
    Ok(Json(out))
}

/// A done or dropped item back to open, with a reason.
///
/// # Errors
/// 409 when the item is open already or no reason is given.
pub async fn reopen(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<ReopenRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let cols = rules::reopen(&r, given(Some(&req.why)), "agent")?;
    call.tx.update(r.rid, &cols).await?;
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "reopened",
            Some(&req.why),
            Some(call.branch()),
            None,
        )
        .await?;
    settle_audits(&mut call.tx, &call.project, &[r.rid]).await?;
    let row = call.tx.fresh(r.rid).await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}
