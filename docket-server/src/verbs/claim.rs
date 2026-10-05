//! `start`, `release`, `close`, `drop` and `reopen`: the claim and the end of an item.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::{DatabaseConnection, Value};
use serde_json::{Map, Value as Json_, json};

use docket_core::api::{
    CloseRequest, Closed, DropRequest, Dropped, JobReportRequest, Moved, ReleaseRequest,
    ReopenRequest, Share, StartRequest, Started,
};
use docket_core::area;
use docket_core::assignment::{OUTCOMES, past_failure_limit};
use docket_core::item::{Field, Item};
use docket_core::rules;
use docket_core::text::gates_result;
use docket_core::word::Kind;

use crate::auth::Caller;
use crate::store::scalar;
use crate::verbs::areas;
use crate::verbs::graph::{hold_waiters, live_overlaps, open_under, release_waiters};
use crate::verbs::view::{brief, item_view};
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
    let kind = r.item_type.kind();
    if kind == Kind::Audit && r.claim_branch.is_none() {
        let open: Vec<String> = open_under(&call.tx.conn, &call.slug, r.rid)
            .await?
            .into_iter()
            .map(|x| x.id)
            .collect();
        if !open.is_empty() {
            return Err(rules::open_under_plan(
                &r,
                &open,
                "It is audited once nothing under it is open.",
            )
            .into());
        }
    }
    if kind.is_retired_plan() {
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
        item: item_view(c, row).await?,
        kind: row.item_type.kind().as_str().to_string(),
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
    assign_past_failure_limit(&mut call, row.rid).await?;
    let row = call.tx.fresh(row.rid).await?;
    let item = item_view(&call.tx.conn, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

/// Hands the item to the owner when the attempt just ended failed and the item has now failed the
/// project's `failure_limit` times. It goes past the owner's limit on open asks: the owner is the
/// only one left who can move it.
async fn assign_past_failure_limit(call: &mut Call, rid: i64) -> Result<(), Failure> {
    let conn = &call.tx.conn;
    let last: Option<String> = scalar(
        conn,
        "SELECT outcome FROM assignments WHERE rid=? AND kind='claim' AND ended_at IS NOT NULL \
         ORDER BY id DESC LIMIT 1",
        vec![rid.into()],
    )
    .await?;
    if last.as_deref() != Some("failed") {
        return Ok(());
    }
    let failed: Option<i64> = scalar(
        conn,
        "SELECT COUNT(*) FROM assignments WHERE rid=? AND kind='claim' AND outcome='failed'",
        vec![rid.into()],
    )
    .await?;
    let failed = usize::try_from(failed.unwrap_or(0)).unwrap_or(0);
    let skills = crate::facts::stored(conn, &call.slug).await?;
    let owner = crate::facts::owner(conn).await?;
    let limit = docket_core::fact::layered(&skills, &owner, "failure_limit")
        .and_then(|(v, _)| v.parse().ok())
        .unwrap_or(docket_core::assignment::FAILURE_LIMIT);
    if !past_failure_limit(failed, limit) {
        return Ok(());
    }
    let note = format!("{failed} attempts failed: it needs the owner to look at why");
    call.tx
        .update(
            rid,
            &[
                Field::Turn(Some("user".to_string())),
                Field::TurnNote(Some(note.clone())),
                Field::AskedAt(Some(call.ctx.now.clone())),
            ],
        )
        .await?;
    call.tx
        .event(&call.slug, Some(rid), "asked", Some(&note), None, None)
        .await?;
    Ok(())
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
        ("report", given(req.report.as_deref()).map(|v| json!(v))),
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
    let item = item_view(&call.tx.conn, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

/// Done, under a sha or what closed it; releases waiters.
///
/// # Errors
/// 409 when the rules refuse it, or a plan that never came due has open work
/// under it.
pub async fn close(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<CloseRequest>,
) -> Result<Json<Closed>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let kind = r.item_type.kind();
    let resolution = given(req.resolution.as_deref());
    if resolution.is_none() && kind != Kind::Decision {
        return Err(Failure::Refused(format!(
            "close needs a resolution: the sha the work landed as, or what closed it. No branch of {} was found to read one from.",
            r.claim_branch.as_deref().unwrap_or("this item")
        )));
    }
    let open_members: Vec<String> = if kind == Kind::Audit {
        open_under(&call.tx.conn, &call.slug, r.rid)
            .await?
            .into_iter()
            .map(|x| x.id)
            .collect()
    } else {
        Vec::new()
    };
    let cols = rules::close(&r, &call.ctx, resolution, kind, &open_members)?;
    let resolution = resolution.unwrap_or_default().to_string();
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
    let released = release_waiters(&mut call.tx, &call.project, &row, "closed").await?;
    let out = Closed {
        item: item_view(&call.tx.conn, &row).await?,
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
    let released = release_waiters(&mut call.tx, &call.project, &row, "dropped").await?;
    let out = Dropped {
        item: item_view(&call.tx.conn, &row).await?,
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
    let mut cols = rules::reopen(&r, given(Some(&req.why)), "agent")?;
    let listed = areas::listed(&call.tx.conn, &call.slug).await?;
    let own = listed.name(r.area_id).map(str::to_string);
    let given = req.area.as_deref().map(str::trim).filter(|a| !a.is_empty());
    let moved = if let Some(given) = given {
        let to = areas::area_id(&call.tx.conn, &call.slug, given).await?;
        if let Some(to) = to {
            areas::open_into(&call.tx.conn, &call.slug, to).await?;
        }
        to.filter(|to| Some(*to) != r.area_id)
    } else {
        if let Some(own) = own.as_deref() {
            area::open_into(&listed.all(), own).map_err(|_| {
                    Failure::Refused(format!(
                        "{} is in {own}, which holds closed items only: reopen it with --area NAME, one of {}",
                        r.id,
                        listed
                            .all()
                            .iter()
                            .filter(|a| !a.history)
                            .map(|a| a.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                })?;
        }
        None
    };
    if let Some(to) = moved {
        cols.push(Field::AreaId(Some(to)));
    }
    call.tx.update(r.rid, &cols).await?;
    if let Some(to) = moved {
        let name = areas::name_of(&call.tx.conn, Some(to)).await?;
        let why = format!("area {}", areas::label(name.as_deref()));
        call.tx
            .event(&call.slug, Some(r.rid), "edited", Some(&why), None, None)
            .await?;
    }
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
    let row = call.tx.fresh(r.rid).await?;
    hold_waiters(&mut call.tx, &call.project, &row).await?;
    let item = item_view(&call.tx.conn, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}
