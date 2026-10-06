//! `priority`, `rate`, `edit`, `link` and `parent`: the fields and ties of items.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::DatabaseConnection;

use docket_core::api::{
    EditRequest, LinkRequest, Linked, Moved, MovedMany, ParentRequest, Parented, PriorityRequest,
    RateRequest,
};
use docket_core::item::{Field, Item};
use docket_core::label;
use docket_core::rules::{prioritise, rate as rate_rule, set_tags};

use crate::auth::Caller;
use crate::store::id_of;
use crate::verbs::graph::{Order, holds_of, parent_ties, refuse_later, release_list, standing};
use crate::verbs::releases::{name_of, release_id};
use crate::verbs::view::{item_view, item_views};
use crate::verbs::{Call, Failure};
use crate::verbs::{areas, labels};

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
        out.push(call.tx.update(r.rid, &prioritise(&req.tier)?).await?);
        let note = format!("priority {}", req.tier);
        call.tx
            .event(&call.slug, Some(r.rid), "edited", Some(&note), None, None)
            .await?;
    }
    let items = item_views(&call.tx.conn, &out).await?;
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
    let item = item_view(&call.tx.conn, &row).await?;
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
    if req.set.is_empty()
        && req.append.is_none()
        && req.body.is_none()
        && req.release.is_none()
        && req.area.is_none()
    {
        return Err(Failure::Refused(
            "edit needs --set field=value, --release NAME, --area NAME, --append \"text\" or --body FILE|-."
                .to_string(),
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
        if !set_labels(&call, r.rid, &kv.field, &kv.value).await? {
            if let Some(col) = set_field(&kv.field, &kv.value)? {
                cols.push(col);
            } else {
                let note = Some(kv.value.as_str()).filter(|v| !v.is_empty());
                call.tx.note_ask(r.rid, note).await?;
            }
        }
        notes.push(format!("{}={}", kv.field, kv.value));
    }
    if let Some(given) = &req.release {
        let to = release_id(&call.tx.conn, &call.slug, given).await?;
        let order = Order::load(&call.tx.conn, &call.slug).await?;
        let name = name_of(&call.tx.conn, to).await?;
        if req.carry {
            let label = name.as_deref().unwrap_or("the backlog");
            let note = format!("release {label} (carried with {})", r.id);
            for (rid, _, release) in order.carried(r.rid, to) {
                call.tx.update(rid, &[Field::ReleaseId(release)]).await?;
                call.tx
                    .event(&call.slug, Some(rid), "edited", Some(&note), None, None)
                    .await?;
            }
        } else if !call.ctx.force {
            order.refuse(&order.moving(r.rid, to))?;
        }
        cols.push(Field::ReleaseId(to));
        notes.push(format!(
            "release {}",
            name.as_deref().unwrap_or("the backlog")
        ));
    }
    if let Some(given) = &req.area {
        let (col, note) = move_area(&mut call, &r, given).await?;
        cols.push(col);
        notes.push(note);
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
    let item = item_view(&call.tx.conn, &row).await?;
    call.tx.commit().await?;
    Ok(Json(Moved { item }))
}

/// The area an edit gives an item, every item under it moved with it. Refused on an item under a
/// plan, whose area is the plan's.
async fn move_area(call: &mut Call, r: &Item, given: &str) -> Result<(Field, String), Failure> {
    let to = areas::area_id(&call.tx.conn, &call.slug, given).await?;
    if let Some(plan) = r.parent_rid
        && to != r.area_id
    {
        let plan = id_of(&call.tx.conn, plan).await?.unwrap_or_default();
        return Err(Failure::Refused(format!(
            "{} is under {plan}, and its area is {plan}'s: edit the area of {plan} instead",
            r.id
        )));
    }
    if let Some(to) = to
        && r.state == "open"
    {
        areas::open_into(&call.tx.conn, &call.slug, to).await?;
    }
    let name = areas::name_of(&call.tx.conn, to).await?;
    let label = areas::label(name.as_deref());
    let why = format!("area {label} (with {})", r.id);
    areas::carry_under(&mut call.tx, &call.slug, r.rid, to, &why).await?;
    Ok((Field::AreaId(to), format!("area {label}")))
}

/// `--set theme=`, `group=` and `tags=`, which are labels: the theme is given as a label, the group
/// becomes the item's one group label (blank takes it away), and the tags become the item's own
/// labels other than its group's. False for any other field.
async fn set_labels(call: &Call, rid: i64, field: &str, value: &str) -> Result<bool, Failure> {
    let (c, slug, value) = (&call.tx.conn, call.slug.as_str(), value.trim());
    let is_group = |name: &str| name.starts_with(label::GROUP);
    match field {
        "theme" if value.is_empty() => {
            return Err(Failure::Refused(
                "a theme is a label: take it away with docket label rm ID NAME.".to_string(),
            ));
        }
        "theme" => labels::give(c, slug, rid, value).await?,
        "group" => {
            let names: Vec<String> = Some(value)
                .filter(|v| !v.is_empty())
                .map(label::of_group)
                .into_iter()
                .collect();
            labels::set(c, slug, rid, &names, is_group).await?;
        }
        "tags" => labels::set(c, slug, rid, &set_tags(value), |n| !is_group(n)).await?,
        _ => return Ok(false),
    }
    Ok(true)
}

/// The column an edit sets; none for the turn note, which is the note of the owner's open ask.
fn set_field(k: &str, v: &str) -> Result<Option<Field>, Failure> {
    if !EDITABLE.contains(&k) {
        return Err(Failure::Refused(format!(
            "{k} is not editable; fields are {}. State and turn move with their own verbs. Also priority: docket priority, release: --release NAME, area: --area NAME.",
            EDITABLE.join(", ")
        )));
    }
    Ok(Some(match k {
        "complexity" => rate_rule(v)?.remove(0),
        "title" => {
            if v.trim().is_empty() {
                return Err(Failure::Refused("a title cannot be empty".to_string()));
            }
            Field::Title(v.trim().to_string())
        }
        _ => return Ok(None),
    }))
}

/// A related B, or B the origin of A; several A at once.
///
/// # Errors
/// 409 when the kind is not related or origin, or an item would link to itself.
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
    if req.kind != "related" && req.kind != "origin" {
        let parent = if req.kind == "opened" {
            format!(" A plan's children are set with docket parent ID {}.", b.id)
        } else {
            String::new()
        };
        return Err(Failure::Refused(format!(
            "link kinds are related and origin; waits are set with docket wait.{parent}"
        )));
    }
    if many.iter().any(|a| a.rid == b.rid) {
        return Err(Failure::Refused(format!(
            "{} cannot be linked to itself.",
            b.id
        )));
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
    call.tx.commit().await?;
    Ok(Json(Linked {
        items: many.into_iter().map(|a| a.id).collect(),
        kind: req.kind,
        to: b.id,
        removed: req.remove,
    }))
}

/// Why `plan` cannot hold `a` as a child, when it cannot: it is no plan, it is `a`, it is closed while
/// `a` is open, it ships before `a`, or it sits under `a` already, or waits on it.
async fn refuse_parent(
    call: &Call,
    plan: &Item,
    many: &[Item],
    listed: &docket_core::release::Listed,
) -> Result<(), Failure> {
    let kind = plan.item_type.kind();
    if !kind.is_plan() {
        return Err(Failure::Refused(format!(
            "{} is a {}, and only a plan holds children. Record what spawned an item with docket link ID origin {}.",
            plan.id,
            kind.as_str(),
            plan.id
        )));
    }
    if let Some(a) = many.iter().find(|a| a.rid == plan.rid) {
        return Err(Failure::Refused(format!(
            "{} cannot be its own parent.",
            a.id
        )));
    }
    let open: Vec<&Item> = many.iter().filter(|a| a.state == "open").collect();
    if !open.is_empty()
        && let Some(id) = plan.area_id
        && areas::open_into(&call.tx.conn, &call.slug, id)
            .await
            .is_err()
    {
        let theirs = areas::name_of(&call.tx.conn, Some(id)).await?;
        return Err(Failure::Refused(format!(
            "{} is in {}, which holds closed items only, so no open item goes under it: put {} under a plan in another area",
            plan.id,
            theirs.unwrap_or_default(),
            open.iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    if plan.state != "open" && !open.is_empty() {
        return Err(Failure::Refused(format!(
            "{} is {}, and a closed plan holds no open children. Put {} under an open plan, or link it with docket link ID origin {}.",
            plan.id,
            plan.state,
            open.iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            plan.id
        )));
    }
    for a in &open {
        refuse_later(listed, plan, a, call.ctx.force)?;
    }
    let st = standing(&call.tx.conn, &call.slug).await?;
    let mut holds = holds_of(&call.tx.conn, &call.project, &st).await?;
    for t in parent_ties(&call.tx.conn, &call.slug).await? {
        let held = holds.entry(t.to).or_default();
        if !held.contains(&t.rid) {
            held.push(t.rid);
        }
    }
    for a in many {
        if let Some(path) = docket_core::stall::path(&holds, a.rid, plan.rid) {
            let mut around = vec![plan.rid];
            around.extend(&path[..path.len() - 1]);
            return Err(Failure::Refused(format!(
                "{} cannot be the parent of {}: that closes the cycle {}, and a plan finishes only after its children. Move {} out from under {} first.",
                plan.id,
                a.id,
                st.cycle(&around),
                plan.id,
                a.id
            )));
        }
    }
    Ok(())
}

/// Each of `many`, and every item under each to any depth, given the area of the plan they are under,
/// with an edit event on each that moves naming the plan.
async fn take_area(call: &mut Call, plan: &Item, many: &[Item]) -> Result<(), Failure> {
    let name = areas::name_of(&call.tx.conn, plan.area_id).await?;
    let why = format!(
        "area {} (from plan {})",
        areas::label(name.as_deref()),
        plan.id
    );
    for a in many {
        if a.area_id != plan.area_id {
            call.tx
                .update(a.rid, &[Field::AreaId(plan.area_id)])
                .await?;
            call.tx
                .event(&call.slug, Some(a.rid), "edited", Some(&why), None, None)
                .await?;
        }
        areas::carry_under(&mut call.tx, &call.slug, a.rid, plan.area_id, &why).await?;
    }
    Ok(())
}

/// Each of A under the plan, or under no plan; several A at once.
///
/// # Errors
/// 409 when the plan is no plan, is one of A, is closed while one of A is open, ships before one of
/// A, or sits under one of A, named with the path.
pub async fn parent(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<ParentRequest>,
) -> Result<Json<Parented>, Failure> {
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let mut many = Vec::new();
    for id in &req.a {
        many.push(call.item(id).await?);
    }
    let plan = match req.plan.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(id) => Some(call.item(id).await?),
        None => None,
    };
    if let Some(b) = &plan {
        let listed = release_list(&call.tx.conn, &call.slug).await?;
        refuse_parent(&call, b, &many, &listed).await?;
    }
    let to = plan.as_ref().map(|b| b.rid);
    let mut touched: Vec<i64> = to.into_iter().collect();
    for a in many.iter().filter(|a| a.parent_rid != to) {
        let was = match a.parent_rid {
            Some(rid) => id_of(&call.tx.conn, rid).await?,
            None => None,
        };
        touched.extend(a.parent_rid);
        call.tx.update(a.rid, &[Field::ParentRid(to)]).await?;
        let note = match (&plan, was) {
            (Some(b), _) => format!("parent {}", b.id),
            (None, Some(w)) => format!("no parent (was {w})"),
            (None, None) => "no parent".to_string(),
        };
        call.tx
            .event(&call.slug, Some(a.rid), "edited", Some(&note), None, None)
            .await?;
    }
    if let Some(b) = &plan {
        take_area(&mut call, b, &many).await?;
    }
    call.tx.commit().await?;
    Ok(Json(Parented {
        items: many.into_iter().map(|a| a.id).collect(),
        plan: plan.map(|b| b.id),
    }))
}
