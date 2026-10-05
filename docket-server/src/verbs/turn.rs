//! `wait`, `resume`, `ask`, `reply`, `answer` and `decide`: whose move an item is, and what was decided.

use std::fmt::Write;

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::DatabaseConnection;
use serde_json::json;

use docket_core::api::{
    AnswerRequest, Answered, AskRequest, Asked, DecideRequest, DepRequest, Moved, ReplyRequest,
    ResumeRequest, Twin, WaitRequest,
};
use docket_core::area;
use docket_core::item::{Field, Item};
use docket_core::rules;
use docket_core::word::{ItemType, Kind};

use crate::auth::Caller;
use crate::store::NewItem;
use crate::verbs::areas;
use crate::verbs::graph::{
    depends_on, holds_of, refuse_later, release_list, release_waiters, rewait, similar_rows,
    standing,
};
use crate::verbs::view::{brief, item_view};
use crate::verbs::{Call, Failure, chars, given};

/// Park an item behind another, or until a condition: a condition becomes a task on the owner's
/// turn, which the item depends on.
///
/// # Errors
/// 409 when both or neither target is given, the target is satisfied already,
/// ships later, or the dependency would close a cycle.
pub async fn wait(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<WaitRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let (on, until) = (given(req.on.as_deref()), given(req.until.as_deref()));
    if on.is_some() == until.is_some() {
        return Err(Failure::Refused(
            "wait takes exactly one of --on ID or --until \"condition\".".to_string(),
        ));
    }
    let target = match on {
        Some(on) => call.item(on).await?,
        None => owner_task(&mut call, &r, until.unwrap_or_default().trim()).await?,
    };
    let row = depend(&mut call, &r, &[target]).await?;
    let item = item_view(&call.tx.conn, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

/// A condition as a task on the owner's turn, in the waiter's release and related to it.
async fn owner_task(call: &mut Call, r: &Item, condition: &str) -> Result<Item, Failure> {
    rules::require_open(r, "wait")?;
    rules::require_hold(r, &call.ctx, "wait")?;
    if condition.is_empty() {
        return Err(Failure::Refused(
            "wait --until needs the condition, in words.".to_string(),
        ));
    }
    let item_type = ItemType::Task;
    let key = item_type.key().to_string();
    let num = call.tx.next_num(&call.slug, &key).await?;
    let task = call
        .tx
        .insert(NewItem {
            project: call.slug.clone(),
            key,
            num,
            title: condition.to_string(),
            turn: "user".to_string(),
            body: format!(
                "{} waits until this holds. Close it when it does, or drop it if it no longer matters.",
                r.id
            ),
            complexity: None,
            theme: r.theme.clone(),
            release_id: r.release_id,
            area_id: r.area_id,
            group_name: None,
            scope: None,
            item_type,
            priority: "normal".to_string(),
            tags: Vec::new(),
        })
        .await?;
    call.tx
        .event(
            &call.slug,
            Some(task.rid),
            "opened",
            Some(&chars(condition, 120)),
            Some(call.branch()),
            None,
        )
        .await?;
    let task = call
        .tx
        .update(
            task.rid,
            &[
                Field::TurnNote(Some(condition.to_string())),
                Field::AskedAt(Some(call.ctx.now.clone())),
            ],
        )
        .await?;
    call.tx.set_link(r.rid, "related", task.rid, false).await?;
    call.tx.set_link(task.rid, "related", r.rid, false).await?;
    Ok(task)
}

/// `r` depends on each of `on` as well as what it depended on, and waits while any still holds it.
async fn depend(call: &mut Call, r: &Item, on: &[Item]) -> Result<Item, Failure> {
    let st = standing(&call.tx.conn, &call.slug).await?;
    let mut holds = holds_of(&call.tx.conn, &call.project, &st).await?;
    let mut had = depends_on(&call.tx.conn, r.rid).await?;
    let listed = release_list(&call.tx.conn, &call.slug).await?;
    for t in on {
        rules::depend(r, &call.ctx, t.rid)?;
        if had.contains(&t.rid) {
            return Err(Failure::Refused(format!(
                "{} already depends on {}.",
                r.id, t.id
            )));
        }
        let Some(h) = docket_core::stall::holder(t.rid, &st.targets) else {
            let how = if t.state == "open" {
                "decided"
            } else {
                t.state.as_str()
            };
            return Err(Failure::Refused(format!(
                "{} is {how} already; nothing to wait for.",
                t.id
            )));
        };
        let holder = call.tx.fresh(h).await?;
        refuse_later(&listed, r, &holder, call.ctx.force)?;
        if let Some(path) = docket_core::stall::path(&holds, h, r.rid) {
            return Err(Failure::Refused(format!(
                "{} cannot depend on {}, which would close the cycle {}.",
                r.id,
                t.id,
                st.cycle(&path)
            )));
        }
        holds.entry(r.rid).or_default().push(h);
        had.push(t.rid);
    }
    for t in on {
        call.tx
            .execute(
                "INSERT INTO dependencies (rid, on_rid, created_at) VALUES (?, ?, ?)",
                vec![r.rid.into(), t.rid.into(), call.ctx.now.clone().into()],
            )
            .await?;
    }
    call.tx.touch(r.rid);
    rewait(&mut call.tx, &call.slug, r, &st, "").await?;
    let ids: Vec<&str> = on.iter().map(|t| t.id.as_str()).collect();
    let note = format!("on {}", ids.join(", "));
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
    call.tx.fresh(r.rid).await
}

/// Add or remove what an item depends on. Removing the last that holds it resumes it.
///
/// # Errors
/// 409 as `wait` refuses a dependency, or when a dependency to remove is not there.
pub async fn dep(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<DepRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    if req.on.is_empty() {
        return Err(Failure::Refused(
            "dep needs the items it depends on: docket dep add ID ON...".to_string(),
        ));
    }
    let mut on = Vec::new();
    for id in &req.on {
        on.push(call.item(id).await?);
    }
    let row = if req.remove {
        undepend(&mut call, &r, &on).await?
    } else {
        depend(&mut call, &r, &on).await?
    };
    let item = item_view(&call.tx.conn, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

async fn undepend(call: &mut Call, r: &Item, on: &[Item]) -> Result<Item, Failure> {
    rules::require_open(r, "dep rm")?;
    rules::require_hold(r, &call.ctx, "dep rm")?;
    let had = depends_on(&call.tx.conn, r.rid).await?;
    if let Some(t) = on.iter().find(|t| !had.contains(&t.rid)) {
        return Err(Failure::Refused(format!(
            "{} does not depend on {}.",
            r.id, t.id
        )));
    }
    for t in on {
        call.tx
            .execute(
                "DELETE FROM dependencies WHERE rid=? AND on_rid=?",
                vec![r.rid.into(), t.rid.into()],
            )
            .await?;
    }
    call.tx.touch(r.rid);
    let ids: Vec<&str> = on.iter().map(|t| t.id.as_str()).collect();
    let note = format!("no longer depends on {}", ids.join(", "));
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "edited",
            Some(&note),
            Some(call.branch()),
            None,
        )
        .await?;
    let st = standing(&call.tx.conn, &call.slug).await?;
    let r = call.tx.fresh(r.rid).await?;
    rewait(&mut call.tx, &call.slug, &r, &st, &note).await?;
    call.tx.fresh(r.rid).await
}

/// Clear a wait by hand, and every dependency still holding the item.
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
    let st = standing(&call.tx.conn, &call.slug).await?;
    let mut holding = Vec::new();
    for on in depends_on(&call.tx.conn, r.rid).await? {
        if !docket_core::stall::satisfied(on, &st.targets) {
            call.tx
                .execute(
                    "DELETE FROM dependencies WHERE rid=? AND on_rid=?",
                    vec![r.rid.into(), on.into()],
                )
                .await?;
            holding.push(st.id(on));
        }
    }
    let row = call.tx.update(r.rid, &cols).await?;
    let dropped =
        (!holding.is_empty()).then(|| format!("no longer depends on {}", holding.join(", ")));
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "resumed",
            given(req.note.as_deref()).or(dropped.as_deref()),
            Some(call.branch()),
            None,
        )
        .await?;
    let item = item_view(&call.tx.conn, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

/// Refuses handing the owner one more item when they already hold the project's `owner_limit`.
pub async fn ensure_owner_room(call: &Call) -> Result<(), Failure> {
    let conn = &call.tx.conn;
    let skills = crate::facts::stored(conn, &call.slug).await?;
    let owner = crate::facts::owner(conn).await?;
    let limit = docket_core::fact::layered(&skills, &owner, "owner_limit")
        .and_then(|(v, _)| v.parse().ok())
        .unwrap_or(docket_core::queue::OWNER_LIMIT);
    let held: Option<i64> = crate::store::scalar(
        conn,
        "SELECT COUNT(*) FROM items WHERE project=? AND state='open' AND turn='user'",
        vec![call.slug.clone().into()],
    )
    .await?;
    let held = usize::try_from(held.unwrap_or(0)).unwrap_or(0);
    match docket_core::queue::owner_limit_refusal(held, limit) {
        Some(why) => Err(Failure::Refused(why)),
        None => Ok(()),
    }
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
    if let Some(need) = req.need.as_deref()
        && !docket_core::queue::NEEDS.contains(&need)
    {
        return Err(Failure::Refused(format!(
            "need is one of {}, not '{need}'.",
            docket_core::queue::NEEDS.join(", ")
        )));
    }
    if r.turn.as_deref() != Some("user") {
        ensure_owner_room(&call).await?;
    }
    let data = req.need.as_ref().map(|need| json!({ "need": need }));
    let twins = similar_rows(&call.tx.conn, &call.slug, &r, 3, Some("done")).await?;
    let row = call.tx.update(r.rid, &cols).await?;
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "asked",
            Some(&req.note),
            Some(call.branch()),
            data.as_ref(),
        )
        .await?;
    let out = Asked {
        item: item_view(&call.tx.conn, &row).await?,
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
    let item = item_view(&call.tx.conn, &row).await?;
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
    let kind = r.item_type.kind();
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
    let mut released = release_waiters(&mut call.tx, &call.project, &row, "decided").await?;
    let row = if carriers.is_empty() {
        row
    } else {
        let ids: Vec<&str> = carriers.iter().map(|c| c.id.as_str()).collect();
        let resolution = format!("carried by {}", ids.join(", "));
        let cols = rules::close(&row, &call.ctx, Some(&resolution), kind, &[])?;
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
            call.tx.set_link(c.rid, "origin", r.rid, false).await?;
        }
        released.extend(release_waiters(&mut call.tx, &call.project, &row, "closed").await?);
        row
    };
    let out = Answered {
        item: item_view(&call.tx.conn, &row).await?,
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
    if r.item_type.kind() == Kind::Decision {
        return Err(Failure::Refused(format!(
            "{} is a question: decide only records a choice in the body. Use docket answer {} \"the choice\" --derived \"the basis\", which settles it and unblocks what waited on it.",
            req.id, req.id
        )));
    }
    let mut data = json!({ "derived": chars(req.basis.trim(), 400) });
    let mut choice = req.choice.trim().to_string();
    if let Some(given) = req.area.as_deref() {
        if r.state != "open" {
            return Err(Failure::Refused(format!(
                "{} is closed: closed items with no area go to unsorted by rule",
                req.id
            )));
        }
        let name = area::placement_name(given)?;
        if !areas::listed(&call.tx.conn, &call.slug)
            .await?
            .rows
            .is_empty()
        {
            return Err(Failure::Refused(format!(
                "{} has areas already: place it with docket edit {} --area {name}",
                call.slug, req.id
            )));
        }
        data["area"] = json!(name);
        if let Some(about) = req
            .about
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
        {
            data["about"] = json!(chars(about, 400));
        }
        if choice.is_empty() {
            choice = format!("area {name}");
        }
    } else if req.about.is_some() {
        return Err(Failure::Refused(
            "--about describes an area: give --area with it".to_string(),
        ));
    }
    if choice.is_empty() {
        return Err(Failure::Refused(
            "decide needs the choice, or --area NAME".to_string(),
        ));
    }
    let body = format!(
        "{}\n\n**Decided, {}.** {}. Basis: {}",
        r.body.trim_end_matches('\n'),
        call.stamp(),
        choice.trim_end_matches('.'),
        req.basis.trim()
    );
    let row = call
        .tx
        .update(
            r.rid,
            &[Field::Body(body.trim_start_matches('\n').to_string())],
        )
        .await?;
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "decided",
            Some(&chars(&choice, 400)),
            Some(call.branch()),
            Some(&data),
        )
        .await?;
    let item = item_view(&call.tx.conn, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}
