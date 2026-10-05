//! A project's lead claim: `GET /lead` reads it, `POST /do/lead` takes, renews or gives it back under
//! the rules of `docket_core::lead`. A take, a takeover and a give each write an event; a renewal
//! writes none.

use axum::extract::{Extension, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult};
use serde_json::{Value, json};

use docket_core::api::{LeadRequest, LeadState};
use docket_core::fact;
use docket_core::lead::{self, Act, Holder, Lead, Outcome};
use docket_core::pace::epoch;

use crate::auth::Caller;
use crate::facts::{InProject, stored};
use crate::store::{Tx, sql};
use crate::verbs::{Failure, choice};

const READ: &str =
    "SELECT project, host, session, branch, since, renewed_at FROM leads WHERE project=?";
const TAKE: &str = "INSERT INTO leads (project, host, session, branch, since, renewed_at) \
                    VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT (project) DO UPDATE SET host=EXCLUDED.host, \
                    session=EXCLUDED.session, branch=EXCLUDED.branch, since=EXCLUDED.since, \
                    renewed_at=EXCLUDED.renewed_at";
const RENEW: &str = "UPDATE leads SET renewed_at=?, branch=? WHERE project=?";
const GIVE: &str = "DELETE FROM leads WHERE project=?";

pub fn router() -> Router<DatabaseConnection> {
    Router::new()
        .route("/lead", get(read))
        .route("/do/lead", post(act))
}

#[derive(FromQueryResult)]
struct Row {
    project: String,
    host: String,
    session: String,
    branch: Option<String>,
    since: String,
    renewed_at: String,
}

async fn held<C: ConnectionTrait>(c: &C, slug: &str) -> Result<Option<Lead>, Failure> {
    let row = c.query_one_raw(sql(READ, vec![slug.into()])).await?;
    Ok(match row {
        Some(r) => {
            let r = Row::from_query_result(&r, "")?;
            Some(Lead {
                project: r.project,
                host: r.host,
                session: r.session,
                branch: r.branch,
                since: r.since,
                renewed_at: r.renewed_at,
            })
        }
        None => None,
    })
}

/// The project's `lead_lapse` in minutes, its default when unset or unreadable. A 404 for no project.
async fn lapse<C: ConnectionTrait>(c: &C, slug: &str) -> Result<i64, Failure> {
    let skills = fact::merged(&stored(c, slug).await?, &crate::facts::owner(c).await?);
    Ok(fact::effective(&skills, "lead_lapse")
        .and_then(|v| v.parse().ok())
        .filter(|m: &i64| *m > 0)
        .unwrap_or(lead::LAPSE))
}

fn state(slug: &str, lead: Option<Lead>, now: &str, lapse_minutes: i64) -> LeadState {
    let now = epoch(now).unwrap_or_default();
    LeadState {
        project: slug.to_string(),
        lapsed: lead
            .as_ref()
            .is_some_and(|l| lead::lapsed(l, now, lapse_minutes)),
        lapses_at: lead
            .as_ref()
            .and_then(|l| lead::lapses_at(l, lapse_minutes)),
        lead,
        lapse_minutes,
        outcome: None,
        previous: None,
    }
}

/// `docket lead show`: who leads the project, and whether the claim lapsed.
///
/// # Errors
/// 404 for an unknown project.
pub async fn read(
    State(db): State<DatabaseConnection>,
    Query(q): Query<InProject>,
) -> Result<Json<LeadState>, Failure> {
    let slug = q.project;
    let minutes = lapse(&db, &slug).await?;
    let lead = held(&db, &slug).await?;
    Ok(Json(state(
        &slug,
        lead,
        &docket_core::clock::now(),
        minutes,
    )))
}

/// `docket lead take|renew|give`: the claim as it stands after the act, with what it did.
///
/// # Errors
/// 400 for an unknown act or no session, 404 for an unknown project, 409 when the rules refuse it.
pub async fn act(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<LeadRequest>,
) -> Result<Json<LeadState>, Failure> {
    choice("act", Some(&req.act), &["take", "renew", "give"])?;
    let act = Act::parse(&req.act).unwrap_or(Act::Take);
    let session = req.session.trim().to_string();
    if session.is_empty() {
        return Err(Failure::Invalid("a lead names its session".into()));
    }
    let slug = req.common.project.clone();
    let branch = req.common.branch.clone();
    let tx = Tx::begin(&db, &caller.host).await?;
    let minutes = lapse(&tx.conn, &slug).await?;
    let lead = held(&tx.conn, &slug).await?;
    let who = Holder {
        host: caller.host.clone(),
        session: session.clone(),
    };
    let now = epoch(&tx.now).unwrap_or_default();
    let outcome = lead::decide(&slug, lead.as_ref(), &who, act, now, minutes)?;
    let event = |act: &str, extra: Value| {
        let mut data = json!({ "act": act, "session": session });
        if let (Some(d), Value::Object(more)) = (data.as_object_mut(), extra) {
            d.extend(more);
        }
        data
    };
    match &outcome {
        Outcome::Took | Outcome::TookOver(_) => {
            tx.execute(
                TAKE,
                vec![
                    slug.clone().into(),
                    caller.host.clone().into(),
                    session.clone().into(),
                    branch.clone().into(),
                    tx.now.clone().into(),
                    tx.now.clone().into(),
                ],
            )
            .await?;
            let data = match &outcome {
                Outcome::TookOver(was) => event(
                    "takeover",
                    json!({ "from_host": was.host, "from_session": was.session }),
                ),
                _ => event("take", json!({})),
            };
            tx.event(&slug, None, "lead", None, branch.as_deref(), Some(&data))
                .await?;
        }
        Outcome::Renewed => {
            tx.execute(
                RENEW,
                vec![
                    tx.now.clone().into(),
                    branch.clone().into(),
                    slug.clone().into(),
                ],
            )
            .await?;
        }
        Outcome::Gave => {
            tx.execute(GIVE, vec![slug.clone().into()]).await?;
            let data = event("give", json!({}));
            tx.event(&slug, None, "lead", None, branch.as_deref(), Some(&data))
                .await?;
        }
    }
    let after = held(&tx.conn, &slug).await?;
    let mut out = state(&slug, after, &tx.now, minutes);
    tx.commit().await?;
    out.outcome = Some(outcome.word().to_string());
    if let Outcome::TookOver(was) = outcome {
        out.previous = Some(was);
    }
    Ok(Json(out))
}

#[cfg(test)]
#[path = "tests/lead.rs"]
mod tests;
