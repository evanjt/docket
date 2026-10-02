//! The machines jobs run on: `GET /machines` reads them for any key, `POST /do/machine` sets or
//! removes one on the owner's key, under the rules of `docket_core::machine`.

use axum::extract::{Extension, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult};

use docket_core::api::{MachineRequest, Machines};
use docket_core::machine::{self, Machine};

use crate::auth::Caller;
use crate::store::{Tx, json, sql};
use crate::verbs::Failure;

const ALL: &str = "SELECT name, ssh, slots, runners, note, updated_at FROM machines ORDER BY name";
const ONE: &str =
    "SELECT name, ssh, slots, runners, note, updated_at FROM machines WHERE name=? FOR UPDATE";
const WRITE: &str = "INSERT INTO machines (name, ssh, slots, runners, note, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT (name) DO UPDATE SET ssh=EXCLUDED.ssh, \
                     slots=EXCLUDED.slots, runners=EXCLUDED.runners, note=EXCLUDED.note, \
                     updated_at=EXCLUDED.updated_at";
const REMOVE: &str = "DELETE FROM machines WHERE name=?";

pub fn router() -> Router<DatabaseConnection> {
    Router::new()
        .route("/machines", get(read))
        .route("/do/machine", post(set))
}

#[derive(FromQueryResult)]
struct Row {
    name: String,
    ssh: String,
    slots: i64,
    runners: serde_json::Value,
    note: Option<String>,
    updated_at: String,
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
            ],
        )
        .await?;
    }
    let machines = rows(&tx.conn, ALL, vec![]).await?;
    tx.commit().await?;
    Ok(Json(Machines { machines }))
}

#[cfg(test)]
#[path = "tests/machines.rs"]
mod tests;
