//! `wait`, `resume`, `ask`, `reply`, `answer` and `decide`: whose move an item is, and what was decided.

use std::fmt::Write;

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::DatabaseConnection;
use serde_json::json;

use docket_core::api::{
    AnswerRequest, Answered, AskRequest, Asked, DecideRequest, Moved, ReplyRequest, ResumeRequest,
    Twin, WaitRequest,
};
use docket_core::item::Field;
use docket_core::rules;

use crate::auth::Caller;
use crate::verbs::graph::{
    is_standing, refuse_later, release_list, release_waiters, settle_audits, similar_rows,
    wait_cycle,
};
use crate::verbs::view::{brief, item_view, kind};
use crate::verbs::{Call, Failure, chars, given};

/// Park an item behind another, or until a condition.
///
/// # Errors
/// 409 when the item is standing, both or neither target is given, the target is closed or holds it.
pub async fn wait(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<WaitRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    if is_standing(&call.project, &r.key) {
        return Err(Failure::Refused(format!(
            "{} is a standing item and never waits.",
            r.id
        )));
    }
    let (on, until) = (given(req.on.as_deref()), given(req.until.as_deref()));
    if on.is_some() == until.is_some() {
        return Err(Failure::Refused(
            "wait takes exactly one of --on ID or --until \"condition\".".to_string(),
        ));
    }
    let (cols, note) = if let Some(on) = on {
        let t = call.item(on).await?;
        if t.state != "open" {
            return Err(Failure::Refused(format!(
                "{} is {} already; nothing to wait for.",
                t.id, t.state
            )));
        }
        if let Some(refused) = wait_cycle(&call.tx.conn, &call.project, &r, &t).await? {
            return Err(refused);
        }
        let listed = release_list(&call.tx.conn, &call.slug).await?;
        refuse_later(&listed, &r, &t, call.ctx.force)?;
        (
            rules::wait(&r, &call.ctx, "item", Some(t.rid), &t.id)?,
            format!("on {}", t.id),
        )
    } else {
        let condition = until.unwrap_or_default().trim();
        (
            rules::wait(&r, &call.ctx, "condition", None, condition)?,
            format!("until: {condition}"),
        )
    };
    let row = call.tx.update(r.rid, &cols).await?;
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "waited",
            Some(&note),
            Some(call.branch()),
            None,
        )
        .await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

/// Clear a wait by hand.
///
/// # Errors
/// 409 when the item is not waiting.
pub async fn resume(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<ResumeRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let cols = rules::resume(&r)?;
    let row = call.tx.update(r.rid, &cols).await?;
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "resumed",
            given(req.note.as_deref()),
            Some(call.branch()),
            None,
        )
        .await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

/// Hand an item to the owner: their turn, with what is needed.
///
/// # Errors
/// 409 when the rules refuse it.
pub async fn ask(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<AskRequest>,
) -> Result<Json<Asked>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let cols = rules::ask(&r, &call.ctx, given(Some(&req.note)))?;
    let twins = similar_rows(&call.tx.conn, &call.slug, &r, 3, Some("done")).await?;
    let row = call.tx.update(r.rid, &cols).await?;
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "asked",
            Some(&req.note),
            Some(call.branch()),
            None,
        )
        .await?;
    let out = Asked {
        item: item_view(&call.tx.conn, &call.project, &row).await?,
        twins: twins
            .into_iter()
            .map(|t| Twin {
                id: t.id,
                title: t.title,
                resolution: t.resolution.unwrap_or_default(),
            })
            .collect(),
    };
    call.tx.commit().await?;
    Ok(Json(out))
}

/// Hand it back to the agents, with what happened.
///
/// # Errors
/// 409 when it is the agent's turn already.
pub async fn reply(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<ReplyRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let cols = rules::reply(&r, given(Some(&req.note)))?;
    let row = call.tx.update(r.rid, &cols).await?;
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "replied",
            Some(&req.note),
            Some(call.branch()),
            None,
        )
        .await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

/// Record a decision on a question; releases what waited on it.
///
/// # Errors
/// 409 when the rules refuse it.
pub async fn answer(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<AnswerRequest>,
) -> Result<Json<Answered>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let mut carriers = Vec::new();
    for id in &req.carried_by {
        carriers.push(call.item(id).await?);
    }
    let kind = kind(&call.project, &r);
    let mut cols = rules::answer(
        &r,
        &call.ctx,
        given(Some(&req.decision)),
        kind,
        req.derived.as_deref(),
    )?;
    let stamp = call.stamp();
    let repeat = rules::is_repeat_answer(&r, &req.decision, req.derived.as_deref());
    let mut body = if repeat {
        r.body.clone()
    } else {
        rules::supersede_decisions(&r.body).0
    };
    body.truncate(body.trim_end_matches('\n').len());
    let _ = match &req.derived {
        _ if repeat => Ok(()),
        None => write!(body, "\n\n**Decision, {stamp}.** {}", req.decision.trim()),
        Some(basis) => write!(
            body,
            "\n\n**Decision, {stamp}, derived.** {}. Basis: {}",
            req.decision.trim().trim_end_matches('.'),
            basis.trim()
        ),
    };
    cols.push(Field::Body(body.trim_start_matches('\n').to_string()));
    let decision = cols.iter().find_map(|f| match f {
        Field::Decision(d) => d.clone(),
        _ => None,
    });
    let row = call.tx.update(r.rid, &cols).await?;
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "decided",
            decision.as_deref(),
            Some(call.branch()),
            None,
        )
        .await?;
    let mut released = release_waiters(&mut call.tx, &call.slug, &row, "decided").await?;
    let row = if carriers.is_empty() {
        row
    } else {
        let ids: Vec<&str> = carriers.iter().map(|c| c.id.as_str()).collect();
        let resolution = format!("carried by {}", ids.join(", "));
        let cols = rules::close(&row, &call.ctx, Some(&resolution), kind)?;
        let row = call.tx.update(r.rid, &cols).await?;
        call.tx
            .event(
                &call.slug,
                Some(r.rid),
                "closed",
                Some(&resolution),
                Some(call.branch()),
                None,
            )
            .await?;
        for c in &carriers {
            call.tx.set_link(c.rid, "opened", r.rid, false).await?;
        }
        released.extend(release_waiters(&mut call.tx, &call.slug, &row, "closed").await?);
        released.extend(settle_audits(&mut call.tx, &call.project, &[r.rid]).await?);
        row
    };
    let out = Answered {
        item: item_view(&call.tx.conn, &call.project, &row).await?,
        released: released.iter().map(brief).collect(),
    };
    call.tx.commit().await?;
    Ok(Json(out))
}

/// A choice made on any item by best practice, recorded with its basis for the owner's digest.
///
/// # Errors
/// 404 when the item is unknown.
pub async fn decide(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<DecideRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let body = format!(
        "{}\n\n**Decided, {}.** {}. Basis: {}",
        r.body.trim_end_matches('\n'),
        call.stamp(),
        req.choice.trim().trim_end_matches('.'),
        req.basis.trim()
    );
    let row = call
        .tx
        .update(
            r.rid,
            &[Field::Body(body.trim_start_matches('\n').to_string())],
        )
        .await?;
    let data = json!({ "derived": chars(req.basis.trim(), 400) });
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "decided",
            Some(&chars(req.choice.trim(), 400)),
            Some(call.branch()),
            Some(&data),
        )
        .await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}
