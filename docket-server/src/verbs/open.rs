//! `new`, `add` and `key`: what opens an item or a key.

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};

use docket_core::api::{AddRequest, Decided, KeyRequest, KeySet, NewRequest, Opened};
use docket_core::item::Field;
use docket_core::rules::{COMPLEXITIES, default_turn, key_spec, prioritise};
use docket_core::text::py_repr;
use docket_core::word::{Kind, PRIORITIES};

use crate::auth::Caller;
use crate::store::NewItem;
use crate::verbs::graph::{decided_like, keys_of};
use crate::verbs::releases::release_id;
use crate::verbs::view::item_view;
use crate::verbs::{Call, Failure, KINDS, TURNS, chars, choice, require_owner};

/// # Errors
/// 403 on an agent's key, 409 on an unknown key or a tier outside the four.
pub async fn new(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<NewRequest>,
) -> Result<Json<Opened>, Failure> {
    require_owner(&caller, "new")?;
    let call = Call::begin(&db, &caller, &req.common).await?;
    open_item(call, req, None).await.map(Json)
}

/// A side finding filed as a low-priority ticket.
///
/// # Errors
/// 403 on an agent's key, 409 when the project has no work key.
pub async fn add(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<AddRequest>,
) -> Result<Json<Opened>, Failure> {
    require_owner(&caller, "new")?;
    let call = Call::begin(&db, &caller, &req.common).await?;
    let key = match req.key {
        Some(k) => k,
        None => default_work_key(&call)?,
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
    };
    open_item(call, as_new, req.from).await.map(Json)
}

fn default_work_key(call: &Call) -> Result<String, Failure> {
    let work = keys_of(&call.project, &[Kind::Work]);
    if work.is_empty() {
        return Err(Failure::Refused(format!(
            "{} has no work key to file under: docket add --key KEY \"title\"",
            call.slug
        )));
    }
    Ok(if work.iter().any(|k| k == "B") {
        "B".to_string()
    } else {
        work[0].clone()
    })
}

async fn open_item(
    mut call: Call,
    req: NewRequest,
    seen_by: Option<String>,
) -> Result<Opened, Failure> {
    let key = req.key.to_uppercase();
    let spec = key_spec(&call.project.rules, &key)?.clone();
    if spec.kind.is_read_only() {
        let plan = keys_of(&call.project, &[Kind::Audit])
            .into_iter()
            .next()
            .unwrap_or_else(|| "KEY".to_string());
        return Err(Failure::Refused(format!(
            "{key} is kept to read: plans group the work now. File a plan with docket new {plan} \"goal\"."
        )));
    }
    choice("turn", req.turn.as_deref(), &TURNS)?;
    choice("complexity", req.complexity.as_deref(), &COMPLEXITIES)?;
    choice("priority", req.priority.as_deref(), &PRIORITIES)?;
    let turn = match req.turn {
        Some(t) => t,
        None => default_turn(&call.project.rules, &key)?,
    };
    let body = req.body.unwrap_or_default();
    let tags = match req.priority.as_deref() {
        Some(tier) => match prioritise(&[], tier)?.pop() {
            Some(Field::Tags(t)) => t,
            _ => Vec::new(),
        },
        None => Vec::new(),
    };
    if turn == "user" {
        crate::verbs::turn::ensure_owner_room(&call).await?;
    }
    let release_id = match req.release.as_deref() {
        Some(given) => release_id(&call.tx.conn, &call.slug, given).await?,
        None => None,
    };
    let title = req.title.trim().to_string();
    let num = call.tx.next_num(&call.slug, &key).await?;
    let r = call
        .tx
        .insert(NewItem {
            project: call.slug.clone(),
            key: key.clone(),
            num,
            title: title.clone(),
            turn: turn.clone(),
            body: body.trim_end_matches('\n').to_string(),
            complexity: req.complexity,
            theme: req.theme,
            release_id,
            group_name: req.group,
            scope: None,
            tags,
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
    if turn == "user" {
        call.tx
            .update(r.rid, &[Field::AskedAt(Some(call.ctx.now.clone()))])
            .await?;
    }
    let r = call.tx.fresh(r.rid).await?;
    let decided = decided_like(&call.tx.conn, &call.project, &r).await?;
    let out = Opened {
        item: item_view(&call.tx.conn, &call.project, &r).await?,
        kind: spec.kind.as_str().to_string(),
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

/// Add a key to the project's matrix, or change what it means.
///
/// # Errors
/// 409 when the key is malformed or already holds items under another kind.
pub async fn key(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<KeyRequest>,
) -> Result<Json<KeySet>, Failure> {
    choice("kind", Some(&req.kind), &KINDS)?;
    choice("turn", req.turn.as_deref(), &TURNS)?;
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let key = req.key.to_uppercase();
    if key.is_empty() || key.len() > 3 || !key.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(Failure::Refused(format!(
            "{} is not a key: one to three capitals.",
            py_repr(&req.key)
        )));
    }
    let was = call.project.keys.iter().position(|s| s["key"] == key);
    if let Some(i) = was
        && call.project.keys[i]["kind"] != req.kind
    {
        let n: i64 = crate::store::scalar(
            &call.tx.conn,
            "SELECT COUNT(*) FROM items WHERE project=? AND key=?",
            vec![call.slug.clone().into(), key.clone().into()],
        )
        .await?
        .unwrap_or(0);
        if n > 0 {
            return Err(Failure::Refused(format!(
                "{key} is {} and holds {n} items; changing its kind changes the rules they were filed under.",
                call.project.keys[i]["kind"].as_str().unwrap_or("")
            )));
        }
    }
    let turn = req.turn.unwrap_or_else(|| {
        if req.kind == "decision" {
            "user".to_string()
        } else {
            "agent".to_string()
        }
    });
    let meaning = req.meaning.trim().to_string();
    let spec = json!({ "key": key, "kind": req.kind, "meaning": meaning, "turn": turn });
    let mut keys = call.project.keys.clone();
    match was {
        Some(i) => keys[i] = spec,
        None => keys.push(spec),
    }
    call.tx
        .execute(
            "UPDATE projects SET keys=?, updated_at=? WHERE slug=?",
            vec![
                crate::store::json(Value::Array(keys)),
                call.ctx.now.clone().into(),
                call.slug.clone().into(),
            ],
        )
        .await?;
    call.tx.touch_project(&call.slug);
    call.tx.commit().await?;
    Ok(Json(KeySet {
        key,
        kind: req.kind,
        meaning,
        turn,
    }))
}
