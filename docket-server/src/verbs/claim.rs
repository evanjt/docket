//! `start`, `release`, `close`, `drop` and `reopen`: the claim and the end of an item.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::{DatabaseConnection, Value};
use serde_json::{Map, Value as Json_, json};

use docket_core::api::{
    CloseRequest, Closed, DropRequest, Dropped, JobReportRequest, Moved, ReleaseRequest,
    ReopenRequest, Share, StartRequest, Started,
};
use docket_core::assignment::OUTCOMES;
use docket_core::item::Item;
use docket_core::rules;
use docket_core::text::gates_result;
use docket_core::word::Kind;

use crate::auth::Caller;
use crate::verbs::graph::{
    came_due, hold_waiters, live_overlaps, open_under, release_waiters, settle_audits,
};
use crate::verbs::view::{brief, item_view, kind};
use crate::verbs::{Call, Failure, ROLES, RUNNERS, choice, given};

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
/// 409 when the rules refuse the claim, or the item is of a kind kept to read.
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
    let kind = kind(&call.project, &r);
    if kind.is_read_only() {
        return Err(Failure::Refused(format!(
            "{} is a {} and kept to read: plans group the work now, and its tickets are claimed one by one.",
            r.id,
            kind.as_str()
        )));
    }
    let mut data = Map::new();
    for (name, value) in [
        ("role", &req.role),
        ("effort", &req.effort),
        ("job", &req.job),
        ("machine", &req.on),
    ] {
        if let Some(v) = given(value.as_deref()) {
            data.insert(name.into(), json!(v));
        }
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
    let out = started(&call, &row).await?;
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
        call.ctx.force.into(),
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

async fn started(call: &Call, row: &Item) -> Result<Started, Failure> {
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
                "SELECT id FROM items WHERE project=? AND group_name=? AND rid<>? AND state='open' ORDER BY rid",
                vec![call.slug.clone().into(), g.clone().into(), row.rid.into()],
            )
            .await?,
            None => Vec::new(),
        };
    Ok(Started {
        item: item_view(c, &call.project, row).await?,
        kind: kind(&call.project, row).as_str().to_string(),
        shares,
        worktree_hint: call.project.worktree_hint.clone(),
        group_others,
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
    choice("outcome", req.outcome.as_deref(), &OUTCOMES)?;
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let cols = rules::release(&r, &call.ctx)?;
    let row = call.tx.update(r.rid, &cols).await?;
    let mut data = run_facts(req.runner.as_deref(), req.model.as_deref());
    if let Some(outcome) = given(req.outcome.as_deref()) {
        data.insert("outcome".into(), json!(outcome));
    }
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

/// The job's own records kept on the open claim: its times and exit in the event, its tokens and
/// reported cost on the attempt's row.
///
/// # Errors
/// 409 when the item is not claimed.
pub async fn job_report(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<JobReportRequest>,
) -> Result<Json<Moved>, Failure> {
    let call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    if r.claim_branch.is_none() {
        return Err(Failure::Refused(format!("{} holds no claim", r.id)));
    }
    let mut data = Map::new();
    for (name, value) in [
        ("start", given(req.start.as_deref()).map(|v| json!(v))),
        ("end", given(req.end.as_deref()).map(|v| json!(v))),
        ("exit", req.exit.map(|v| json!(v))),
        ("tokens_in", req.tokens_in.map(|v| json!(v))),
        ("tokens_out", req.tokens_out.map(|v| json!(v))),
        ("cost_reported", req.cost_reported.map(|v| json!(v))),
    ] {
        if let Some(v) = value {
            data.insert(name.into(), v);
        }
    }
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "job_reported",
            None,
            Some(call.branch()),
            data_or_none(data).as_ref(),
        )
        .await?;
    let row = call.tx.fresh(r.rid).await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

/// Done, under a sha or what closed it; releases waiters.
///
/// # Errors
/// 409 when the rules refuse it, the item is standing, or a plan that never came due has open work
/// under it.
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
    if kind == Kind::Audit {
        let pending: Vec<String> = open_under(&call.tx.conn, &call.slug, r.rid)
            .await?
            .into_iter()
            .map(|x| x.id)
            .collect();
        rules::close_plan(&r, &pending, came_due(&call.tx.conn, r.rid).await?)?;
    }
    let mut data = run_facts(req.runner.as_deref(), req.model.as_deref());
    if given(req.gates.as_deref()).is_some() {
        data.insert("gates".into(), json!(gates_result(req.gates.as_deref())?));
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
    let mut released = release_waiters(&mut call.tx, &call.project, &row, "closed").await?;
    released.extend(settle_audits(&mut call.tx, &call.project, &[r.rid]).await?);
    let out = Closed {
        item: item_view(&call.tx.conn, &call.project, &row).await?,
        released: released.iter().map(brief).collect(),
    };
    call.tx.commit().await?;
    Ok(Json(out))
}

/// Closed without doing: superseded, archived, will not fix.
///
/// # Errors
/// 409 when the rules refuse it.
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
    let mut released = release_waiters(&mut call.tx, &call.project, &row, "dropped").await?;
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
    hold_waiters(&mut call.tx, &call.project, &row).await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}
