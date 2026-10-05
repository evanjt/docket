//! `priority`, `rate`, `edit` and `link`: the fields and ties of items.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::DatabaseConnection;

use docket_core::api::{
    EditRequest, LinkRequest, Linked, Moved, MovedMany, PriorityRequest, RateRequest,
};
use docket_core::item::{Field, Item};
use docket_core::rules::{GATE, prioritise, rate as rate_rule, set_tags};
use docket_core::word::Kind;

use crate::auth::Caller;
use crate::verbs::graph::{
    holds_of, keys_of, refuse_later, refuse_later_after_theme, release_list, settle_audits,
    standing,
};
use crate::verbs::view::{item_view, item_views};
use crate::verbs::{Call, Failure};

const EDITABLE: [&str; 6] = ["title", "complexity", "theme", "group", "tags", "turn_note"];

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
    if let (Some(_), Some(seen)) = (&req.body, &req.expect_updated_at)
        && *seen != r.updated_at
    {
        return Err(Failure::Refused(format!(
            "{} changed since it was read; reload it before replacing its body.",
            req.id
        )));
    }
    let mut cols = Vec::new();
    let mut notes = Vec::new();
    for kv in &req.set {
        let col = set_field(&r, &kv.field, &kv.value)?;
        if let Field::Theme(theme) = &col
            && !call.ctx.force
        {
            refuse_later_after_theme(&call.tx, &call.slug, &r, theme.as_deref()).await?;
        }
        cols.push(col);
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
            "{k} is not editable; fields are {}. State and turn move with their own verbs. Also priority: docket priority, release: --set theme=<release>.",
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
/// 409 when the kind is not related or opened, or an item would link to itself.
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
    if req.kind == "opened" && !req.remove && b.state == "open" {
        let listed = release_list(&call.tx.conn, &call.slug).await?;
        let held = (b.wait_on.is_none() && keys_of(&call.project, &[Kind::Audit]).contains(&b.key))
            || b.wait_ref.as_deref() == Some(GATE);
        for a in many.iter().filter(|a| a.state == "open" && held) {
            refuse_later(&listed, &b, a, call.ctx.force)?;
        }
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
    if req.kind == "opened" && !req.remove && b.state == "open" {
        let st = standing(&call.tx.conn, &call.slug).await?;
        let holds = holds_of(&call.tx.conn, &call.project, &st).await?;
        for a in many.iter().filter(|a| a.state == "open") {
            if let Some(path) = docket_core::stall::path(&holds, a.rid, b.rid) {
                let mut around = vec![b.rid];
                around.extend(&path[..path.len() - 1]);
                return Err(Failure::Refused(format!(
                    "{} cannot hold {}, which would close the cycle {}, so {} would wait forever. Remove a dependency on it first.",
                    b.id,
                    a.id,
                    st.cycle(&around),
                    a.id
                )));
            }
        }
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
