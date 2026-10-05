//! The machines jobs run on: `GET /machines` reads them for any key, `POST /do/machine` sets or
//! removes one on the owner's key, under the rules of `docket_core::machine`.

use axum::extract::{Extension, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult};

use docket_core::api::{MachineRequest, Machines};
use docket_core::machine::{self, Limit, Machine};

use crate::auth::Caller;
use crate::store::{Tx, json, sql};
use crate::verbs::Failure;

const ALL: &str = "SELECT name, ssh, slots, runners, note, updated_at, limits, path_prefix FROM machines ORDER BY name";
const ONE: &str = "SELECT name, ssh, slots, runners, note, updated_at, limits, path_prefix FROM machines WHERE name=? FOR UPDATE";
const WRITE: &str = "INSERT INTO machines (name, ssh, slots, runners, note, updated_at, path_prefix) \
                     VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT (name) DO UPDATE SET ssh=EXCLUDED.ssh, \
                     slots=EXCLUDED.slots, runners=EXCLUDED.runners, note=EXCLUDED.note, \
                     updated_at=EXCLUDED.updated_at, path_prefix=EXCLUDED.path_prefix";
const LIMITS: &str = "UPDATE machines SET limits=? WHERE name=?";
const REMOVE: &str = "DELETE FROM machines WHERE name=?";

pub fn router() -> Router<DatabaseConnection> {
    Router::new()
        .route("/machines", get(read))
        .route("/do/machine", post(set))
        .route("/do/limit", post(limit))
}

#[derive(FromQueryResult)]
struct Row {
    name: String,
    ssh: String,
    slots: i64,
    runners: serde_json::Value,
    note: Option<String>,
    updated_at: String,
    limits: serde_json::Value,
    path_prefix: Option<String>,
}

impl From<Row> for Machine {
    fn from(r: Row) -> Self {
        Machine {
            name: r.name,
            ssh: r.ssh,
            slots: r.slots,
            runners: serde_json::from_value(r.runners).unwrap_or_default(),
            note: r.note,
            updated_at: r.updated_at,
            limits: serde_json::from_value(r.limits).unwrap_or_default(),
            path: r.path_prefix,
        }
    }
}

async fn rows<C: ConnectionTrait>(
    c: &C,
    text: &str,
    values: Vec<sea_orm::Value>,
) -> Result<Vec<Machine>, Failure> {
    let found = c.query_all_raw(sql(text, values)).await?;
    Ok(found
        .iter()
        .map(|r| Row::from_query_result(r, "").map(Machine::from))
        .collect::<Result<_, _>>()?)
}

/// `docket machines`: every machine, by name.
///
/// # Errors
/// The database fails.
pub async fn read(State(db): State<DatabaseConnection>) -> Result<Json<Machines>, Failure> {
    Ok(Json(Machines {
        machines: rows(&db, ALL, vec![]).await?,
    }))
}

/// `docket machine set NAME ...` or `docket machine remove NAME`: the machines as they stand after.
///
/// # Errors
/// 403 on an agent's key, 409 for a machine the rules refuse, 404 for removing an unknown one.
pub async fn set(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<MachineRequest>,
) -> Result<Json<Machines>, Failure> {
    if !caller.owner {
        return Err(Failure::Forbidden(
            "docket machine is refused on an agent's key: the machines are the owner's to set"
                .into(),
        ));
    }
    let tx = Tx::begin(&db, &caller.host).await?;
    let name = req.set.name.clone();
    let stored = rows(&tx.conn, ONE, vec![name.clone().into()]).await?;
    if req.remove {
        if stored.is_empty() {
            return Err(Failure::NotFound(format!("no machine {name}")));
        }
        tx.execute(REMOVE, vec![name.into()]).await?;
    } else {
        let m = machine::merged(stored.first(), &req.set, &tx.now)?;
        tx.execute(
            WRITE,
            vec![
                m.name.into(),
                m.ssh.into(),
                m.slots.into(),
                json(serde_json::json!(m.runners)),
                m.note.into(),
                m.updated_at.into(),
                m.path.into(),
            ],
        )
        .await?;
    }
    let machines = rows(&tx.conn, ALL, vec![]).await?;
    tx.commit().await?;
    Ok(Json(Machines { machines }))
}

/// A runner's usage limit, as a job reported it: the reset kept is the later of it and the one
/// stored, and resets already past are dropped. Any key may report one.
///
/// # Errors
/// 404 for an unknown machine, 409 for a runner the machine does not have or a reset that is not a
/// stamp.
pub async fn limit(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<Limit>,
) -> Result<Json<Machines>, Failure> {
    let tx = Tx::begin(&db, &caller.host).await?;
    let stored = rows(&tx.conn, ONE, vec![req.machine.clone().into()]).await?;
    let Some(m) = stored.first() else {
        return Err(Failure::NotFound(format!("no machine {}", req.machine)));
    };
    if !m.runners.contains(&req.runner) {
        return Err(Failure::Refused(format!(
            "{} has no {}",
            req.machine, req.runner
        )));
    }
    let valid = req.until.len() == 20
        && req.until.ends_with('Z')
        && req.until.as_bytes().get(10) == Some(&b'T');
    if !valid {
        return Err(Failure::Refused(format!(
            "a reset is a stamp like 2026-10-07T18:29:00Z, not {}",
            req.until
        )));
    }
    let mut limits = m.limits.clone();
    let later = machine::later(limits.get(&req.runner).map(String::as_str), &req.until);
    limits.insert(req.runner, later);
    limits.retain(|_, until| *until > tx.now);
    tx.execute(
        LIMITS,
        vec![json(serde_json::json!(limits)), req.machine.into()],
    )
    .await?;
    let machines = rows(&tx.conn, ALL, vec![]).await?;
    tx.commit().await?;
    Ok(Json(Machines { machines }))
}

#[cfg(test)]
#[path = "tests/machines.rs"]
mod tests;
