//! `defer`, `pull`, `priority`, `rate`, `edit`, `link` and `fold`: the fields and ties of items.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::DatabaseConnection;

use docket_core::api::{
    EditRequest, FoldRequest, LinkRequest, Linked, Moved, MovedMany, PriorityRequest, RateRequest,
    ScopeRequest,
};
use docket_core::item::{Field, Item};
use docket_core::rules::{self, prioritise, rate as rate_rule, require_open, set_tags};
use docket_core::word::{PRIORITIES, priority as priority_of};

use crate::auth::Caller;
use crate::store::column;
use crate::verbs::graph::{is_package, package_members, packages_of, settle_audits};
use crate::verbs::view::{item_view, item_views};
use crate::verbs::{Call, Failure, given};

const EDITABLE: [&str; 7] = [
    "title",
    "complexity",
    "theme",
    "rank",
    "group",
    "tags",
    "turn_note",
];

/// Move items to the later backlog, out of the release.
///
/// # Errors
/// 409 when any of them cannot move.
pub async fn defer(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<ScopeRequest>,
) -> Result<Json<MovedMany>, Failure> {
    scope(&db, &caller, req, Some("later")).await.map(Json)
}

/// Move items from the inbox or later into the release.
///
/// # Errors
/// 409 when any of them cannot move.
pub async fn pull(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<ScopeRequest>,
) -> Result<Json<MovedMany>, Failure> {
    scope(&db, &caller, req, None).await.map(Json)
}

async fn scope(
    db: &DatabaseConnection,
    caller: &Caller,
    req: ScopeRequest,
    to: Option<&str>,
) -> Result<MovedMany, Failure> {
    let mut call = Call::begin(db, caller, &req.common).await?;
    let mut rows = Vec::new();
    for id in &req.ids {
        rows.push(call.item(id).await?);
    }
    let mut cols = Vec::new();
    for r in &rows {
        cols.push(rules::scope(r, to)?);
    }
    let place = to.map_or("the release".to_string(), |t| format!("the {t}"));
    let mut out = Vec::new();
    for (r, c) in rows.iter().zip(cols) {
        out.push(call.tx.update(r.rid, &c).await?);
        let note = match given(req.why.as_deref()) {
            Some(why) => format!("to {place}: {why}"),
            None => format!("to {place}"),
        };
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
    }
    let items = item_views(&call.tx.conn, &call.project, &out).await?;
    call.tx.commit().await?;
    Ok(MovedMany { items })
}

/// Set how soon items are worked.
///
/// # Errors
/// 409 when the tier is not one of the four.
pub async fn priority(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<PriorityRequest>,
) -> Result<Json<MovedMany>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let mut rows = Vec::new();
    for id in &req.ids {
        rows.push(call.item(id).await?);
    }
    let mut out = Vec::new();
    for r in &rows {
        out.push(
            call.tx
                .update(r.rid, &prioritise(&r.tags, &req.tier)?)
                .await?,
        );
        let note = format!("priority {}", req.tier);
        call.tx
            .event(&call.slug, Some(r.rid), "edited", Some(&note), None, None)
            .await?;
    }
    let items = item_views(&call.tx.conn, &call.project, &out).await?;
    call.tx.commit().await?;
    Ok(Json(MovedMany { items }))
}

/// Set complexity.
///
/// # Errors
/// 409 when the level is not one of the three.
pub async fn rate(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<RateRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    let row = call.tx.update(r.rid, &rate_rule(&req.level)?).await?;
    let note = format!("complexity {}", req.level);
    call.tx
        .event(&call.slug, Some(r.rid), "edited", Some(&note), None, None)
        .await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

/// Fields, an appended note, or the whole body.
///
/// # Errors
/// 409 when nothing is given, a field is not editable, or a title is empty.
pub async fn edit(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<EditRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let r = call.item(&req.id).await?;
    if req.set.is_empty() && req.append.is_none() && req.body.is_none() {
        return Err(Failure::Refused(
            "edit needs --set field=value, --append \"text\" or --body FILE|-.".to_string(),
        ));
    }
    let mut cols = Vec::new();
    let mut notes = Vec::new();
    for kv in &req.set {
        cols.push(set_field(&r, &kv.field, &kv.value)?);
        notes.push(format!("{}={}", kv.field, kv.value));
    }
    let mut body = r.body.clone();
    if let Some(append) = &req.append {
        body = format!(
            "{}\n\n**Note, {}.** {}",
            body.trim_end_matches('\n'),
            call.stamp(),
            append.trim()
        );
        notes.push("appended a note".to_string());
    }
    if let Some(whole) = &req.body {
        body = whole.trim_end_matches('\n').to_string();
        notes.push("body replaced".to_string());
    }
    if body != r.body {
        cols.push(Field::Body(body.trim_start_matches('\n').to_string()));
        if r.conflict != 0 && req.body.is_some() {
            cols.push(Field::Conflict(0));
        }
    }
    let row = call.tx.update(r.rid, &cols).await?;
    call.tx
        .event(
            &call.slug,
            Some(r.rid),
            "edited",
            Some(&notes.join(", ")),
            Some(call.branch()),
            None,
        )
        .await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

fn set_field(r: &Item, k: &str, v: &str) -> Result<Field, Failure> {
    if !EDITABLE.contains(&k) {
        return Err(Failure::Refused(format!(
            "{k} is not editable; fields are {}. State and turn move with their own verbs.",
            EDITABLE.join(", ")
        )));
    }
    let blank = |v: &str| {
        if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        }
    };
    Ok(match k {
        "complexity" => rate_rule(v)?.remove(0),
        "rank" => Field::Rank(match v {
            "" => None,
            n => Some(n.parse().map_err(|_| {
                Failure::Invalid(format!(
                    "rank takes a whole number, not {}",
                    docket_core::text::py_repr(n)
                ))
            })?),
        }),
        "group" => Field::GroupName(blank(v)),
        "tags" => Field::Tags(set_tags(&r.tags, v)),
        "title" => {
            if v.trim().is_empty() {
                return Err(Failure::Refused("a title cannot be empty".to_string()));
            }
            Field::Title(v.trim().to_string())
        }
        "theme" => Field::Theme(blank(v)),
        _ => Field::TurnNote(blank(v)),
    })
}

/// A related B, or A opened-by B; several A at once.
///
/// # Errors
/// 409 when the kind is not related or opened, an item would link to itself, or a package rule refuses it.
pub async fn link(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<LinkRequest>,
) -> Result<Json<Linked>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let mut many = Vec::new();
    for id in &req.a {
        many.push(call.item(id).await?);
    }
    let b = call.item(&req.b).await?;
    if req.kind != "related" && req.kind != "opened" {
        return Err(Failure::Refused(
            "link kinds are related and opened; waits are set with docket wait.".to_string(),
        ));
    }
    if many.iter().any(|a| a.rid == b.rid) {
        return Err(Failure::Refused(format!(
            "{} cannot be linked to itself.",
            b.id
        )));
    }
    let into_package = req.kind == "opened" && !req.remove && is_package(&call.project, &b.key);
    if into_package {
        check_membership(&call, &many, &b).await?;
    }
    for a in &many {
        call.tx
            .set_link(a.rid, &req.kind, b.rid, req.remove)
            .await?;
        if req.kind == "related" && !req.remove {
            call.tx.set_link(b.rid, "related", a.rid, false).await?;
        }
        let note = format!(
            "{} {} {}",
            if req.remove { "unlink" } else { "link" },
            req.kind,
            b.id
        );
        call.tx
            .event(&call.slug, Some(a.rid), "edited", Some(&note), None, None)
            .await?;
    }
    if req.kind == "opened" {
        settle_audits(&mut call.tx, &call.project, &[b.rid]).await?;
    }
    call.tx.commit().await?;
    Ok(Json(Linked {
        items: many.into_iter().map(|a| a.id).collect(),
        kind: req.kind,
        to: b.id,
        removed: req.remove,
    }))
}

/// A ticket sits in at most one open package, and a package in none.
async fn check_membership(call: &Call, many: &[Item], b: &Item) -> Result<(), Failure> {
    let b = call.tx.fresh(b.rid).await?;
    if let Some(branch) = &b.claim_branch
        && *branch != call.ctx.branch
        && !call.ctx.force
    {
        return Err(Failure::Refused(format!(
            "{} is under review by {branch} on {}. Add members once that review is closed or released.",
            b.id,
            b.claim_host.unwrap_or_default()
        )));
    }
    for a in many {
        let a = call.tx.fresh(a.rid).await?;
        if is_package(&call.project, &a.key) {
            return Err(Failure::Refused(format!(
                "{} is a package, and a package cannot sit inside another.",
                a.id
            )));
        }
        let other: Vec<Item> = packages_of(&call.tx.conn, &call.project, a.rid)
            .await?
            .into_iter()
            .filter(|p| p.rid != b.rid)
            .collect();
        if let Some(o) = other.first() {
            return Err(Failure::Refused(format!(
                "{} already sits in package {}. One fact has one owner: docket link {} opened {} --remove first.",
                a.id, o.id, a.id, o.id
            )));
        }
    }
    Ok(())
}

/// Several packages become one: their open members, plans, concepts and waiters move to the one
/// that stays, the rest are dropped as superseded by it.
///
/// # Errors
/// 409 when any of them is not an open package, held elsewhere, or the owner's turn.
pub async fn fold(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<FoldRequest>,
) -> Result<Json<Moved>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let into = call.item(&req.into).await?;
    let mut parts = Vec::new();
    for id in &req.ids {
        parts.push(call.item(id).await?);
    }
    if !is_package(&call.project, &into.key) {
        return Err(Failure::Refused(format!(
            "{} is not a package: fold gathers packages into one.",
            into.id
        )));
    }
    check_foldable(&call, &into, &parts).await?;
    if parts.iter().any(|p| p.rid == into.rid) {
        return Err(Failure::Refused(format!(
            "{} cannot fold into itself.",
            into.id
        )));
    }
    let tier_of = |r: &Item| PRIORITIES.iter().position(|t| *t == priority_of(&r.tags));
    let mut tiers = vec![tier_of(&into)];
    let mut folded = Vec::new();
    for p in &parts {
        let p = call.tx.fresh(p.rid).await?;
        tiers.push(tier_of(&p));
        fold_one(&mut call, &into, &p).await?;
        folded.push(p);
    }
    let into = call.tx.fresh(into.rid).await?;
    let note: Vec<String> = folded
        .iter()
        .map(|p| format!("- {}: {}", p.id, p.title))
        .collect();
    let body = format!(
        "{}\n\n**Folded, {}.** Their Principles are this package's to merge:\n{}",
        into.body.trim_end_matches('\n'),
        call.stamp(),
        note.join("\n")
    );
    let mut cols = vec![Field::Body(body.trim_start_matches('\n').to_string())];
    let lowest = tiers.into_iter().flatten().min().unwrap_or(2);
    cols.extend(prioritise(&into.tags, PRIORITIES[lowest])?);
    let row = call.tx.update(into.rid, &cols).await?;
    let ids: Vec<&str> = folded.iter().map(|p| p.id.as_str()).collect();
    let note = format!("folded {}", ids.join(", "));
    call.tx
        .event(
            &call.slug,
            Some(into.rid),
            "edited",
            Some(&note),
            Some(call.branch()),
            None,
        )
        .await?;
    let item = item_view(&call.tx.conn, &call.project, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

async fn check_foldable(call: &Call, into: &Item, parts: &[Item]) -> Result<(), Failure> {
    let mut all = vec![call.tx.fresh(into.rid).await?];
    for p in parts {
        all.push(call.tx.fresh(p.rid).await?);
    }
    for p in &all {
        require_open(p, "fold")?;
        if !is_package(&call.project, &p.key) {
            return Err(Failure::Refused(format!(
                "{} is not a package. Add an item with docket link {} opened {}.",
                p.id, p.id, into.id
            )));
        }
        if let Some(branch) = &p.claim_branch
            && *branch != call.ctx.branch
            && !call.ctx.force
        {
            return Err(Failure::Refused(format!(
                "{} is held by {branch} on {}. Fold it once that claim is closed or released.",
                p.id,
                p.claim_host.clone().unwrap_or_default()
            )));
        }
        if p.rid != into.rid && p.turn.as_deref() == Some("user") && !call.ctx.force {
            return Err(Failure::Refused(format!(
                "{} is the owner's turn ({}). Fold it when it comes back, or --force to fold it and leave its members waiting.",
                p.id,
                p.turn_note
                    .as_deref()
                    .filter(|n| !n.is_empty())
                    .unwrap_or("asked")
            )));
        }
    }
    Ok(())
}

/// One package's members, plans, concepts and waiters moved to `into`, then the package dropped.
async fn fold_one(call: &mut Call, into: &Item, p: &Item) -> Result<(), Failure> {
    for m in package_members(&call.tx.conn, p.rid, true).await? {
        call.tx.set_link(m.rid, "opened", p.rid, true).await?;
        call.tx.set_link(m.rid, "opened", into.rid, false).await?;
        let mut cols = Vec::new();
        if p.wait_on.is_some() && m.wait_on.is_none() {
            cols.extend([
                Field::WaitOn(p.wait_on.clone()),
                Field::WaitItem(p.wait_item),
                Field::WaitRef(p.wait_ref.clone()),
                Field::WaitSince(p.wait_since.clone()),
            ]);
        }
        if p.turn.as_deref() == Some("user") && m.turn.as_deref() != Some("user") {
            cols.extend([
                Field::Turn(Some("user".into())),
                Field::TurnNote(p.turn_note.clone()),
                Field::AskedAt(p.asked_at.clone()),
            ]);
        }
        if !cols.is_empty() {
            call.tx.update(m.rid, &cols).await?;
        }
        let note = format!("moved from {} to {}", p.id, into.id);
        call.tx
            .event(
                &call.slug,
                Some(m.rid),
                "edited",
                Some(&note),
                Some(call.branch()),
                None,
            )
            .await?;
    }
    let plans: Vec<i64> = column(
        &call.tx.conn,
        "SELECT to_rid FROM links WHERE rid=? AND kind='opened' AND to_rid IS NOT NULL ORDER BY to_rid",
        vec![p.rid.into()],
    )
    .await?;
    for to in plans {
        call.tx.set_link(into.rid, "opened", to, false).await?;
    }
    let related: Vec<i64> = column(
        &call.tx.conn,
        "SELECT to_rid FROM links WHERE rid=? AND kind='related' AND to_rid IS NOT NULL ORDER BY to_rid",
        vec![p.rid.into()],
    )
    .await?;
    for to in related {
        if to != into.rid {
            call.tx.set_link(into.rid, "related", to, false).await?;
            call.tx.set_link(to, "related", into.rid, false).await?;
        }
    }
    let waiters: Vec<i64> = column(
        &call.tx.conn,
        "SELECT rid FROM items WHERE wait_item=? AND state='open' ORDER BY rid",
        vec![p.rid.into()],
    )
    .await?;
    for x in waiters {
        if x == into.rid {
            let cols = rules::resume(into)?;
            call.tx.update(into.rid, &cols).await?;
        } else {
            call.tx
                .update(
                    x,
                    &[
                        Field::WaitItem(Some(into.rid)),
                        Field::WaitRef(Some(into.id.clone())),
                    ],
                )
                .await?;
        }
    }
    let why = format!("folded into {}", into.id);
    let forced = docket_core::item::Ctx {
        force: true,
        ..call.ctx.clone()
    };
    let cols = rules::drop(p, &forced, Some(&why), Some(into.rid))?;
    call.tx.update(p.rid, &cols).await?;
    call.tx
        .event(
            &call.slug,
            Some(p.rid),
            "dropped",
            Some(&why),
            Some(call.branch()),
            None,
        )
        .await?;
    settle_audits(&mut call.tx, &call.project, &[p.rid]).await?;
    Ok(())
}
