//! The write verbs: `POST /do/{verb}`, each one rule run inside one transaction.

pub mod claim;
pub mod fields;
pub mod graph;
pub mod open;
pub mod project;
pub mod retry;
pub mod turn;
pub mod view;

use axum::Json;
use axum::Router;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use sea_orm::{DatabaseConnection, DbErr};
use serde_json::json;

use docket_core::api::Common;
use docket_core::item::{Ctx, Item};

use crate::auth::Caller;
use crate::store::{ProjectRow, Tx};

pub const TURNS: [&str; 2] = ["agent", "user"];
pub const RUNNERS: [&str; 3] = ["codex", "claude", "remote"];
pub const ROLES: [&str; 4] = ["build", "rebase", "review", "plan"];
pub const KINDS: [&str; 8] = [
    "work", "decision", "research", "audit", "story", "concept", "idea", "package",
];

/// Why a verb did not apply, and the status that says so.
#[derive(Debug)]
pub enum Failure {
    /// 409: the rules refuse it, in the words the person reads.
    Refused(String),
    /// 404: no such project or item.
    NotFound(String),
    /// 403: the owner's key is needed.
    Forbidden(String),
    /// 400: an argument outside its choices.
    Invalid(String),
    /// 500.
    Db(DbErr),
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        match self {
            Failure::Refused(text) => {
                (StatusCode::CONFLICT, Json(json!({ "refused": text }))).into_response()
            }
            Failure::NotFound(text) => {
                (StatusCode::NOT_FOUND, Json(json!({ "error": text }))).into_response()
            }
            Failure::Forbidden(text) => {
                (StatusCode::FORBIDDEN, Json(json!({ "error": text }))).into_response()
            }
            Failure::Invalid(text) => {
                (StatusCode::BAD_REQUEST, Json(json!({ "error": text }))).into_response()
            }
            Failure::Db(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    }
}

/// One verb's transaction, project and context, opened together.
pub struct Call {
    pub tx: Tx,
    pub ctx: Ctx,
    pub slug: String,
    pub project: ProjectRow,
}

impl Call {
    pub async fn begin(
        db: &DatabaseConnection,
        caller: &Caller,
        common: &Common,
    ) -> Result<Self, Failure> {
        let tx = Tx::begin(db, &caller.host).await?;
        let project = tx.project(&common.project).await?;
        let ctx = Ctx {
            host: caller.host.clone(),
            branch: common.branch.clone().unwrap_or_else(|| "main".to_string()),
            now: tx.now.clone(),
            force: common.force,
        };
        Ok(Self {
            tx,
            ctx,
            slug: common.project.clone(),
            project,
        })
    }

    pub async fn item(&self, id: &str) -> Result<Item, Failure> {
        self.tx.item(&self.slug, id).await
    }

    pub fn branch(&self) -> &str {
        self.ctx.branch.as_str()
    }

    pub fn stamp(&self) -> String {
        chars(&self.ctx.now, 10)
    }
}

/// A value held to its choices, as argparse holds the flag.
pub fn choice(name: &str, value: Option<&str>, allowed: &[&str]) -> Result<(), Failure> {
    match value {
        Some(v) if !allowed.contains(&v) => Err(Failure::Invalid(format!(
            "{name} is one of {}, not {}",
            allowed.join(", "),
            docket_core::text::py_repr(v)
        ))),
        _ => Ok(()),
    }
}

/// The first n characters, as a Python slice reads them.
#[must_use]
pub fn chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// The ids of the first n rows, with a trailing mark when there are more.
#[must_use]
pub fn some_ids(rows: &[Item], n: usize) -> String {
    let shown: Vec<&str> = rows.iter().take(n).map(|r| r.id.as_str()).collect();
    format!(
        "{}{}",
        shown.join(", "),
        if rows.len() > n { " ..." } else { "" }
    )
}

/// `Some` for a string that says something, like a Python truth test.
#[must_use]
pub fn given(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.is_empty())
}

/// The agent's key refused for a verb the owner keeps.
pub fn require_owner(caller: &Caller, verb: &str) -> Result<(), Failure> {
    if caller.owner {
        return Ok(());
    }
    Err(Failure::Forbidden(format!(
        "docket {verb} is refused on an agent's key. Put what you found under Observations in your report; the loop files it to the inbox."
    )))
}

pub fn router() -> Router<DatabaseConnection> {
    Router::new()
        .route("/new", post(open::new))
        .route("/add", post(open::add))
        .route("/key", post(open::key))
        .route("/defer", post(fields::defer))
        .route("/pull", post(fields::pull))
        .route("/start", post(claim::start))
        .route("/release", post(claim::release))
        .route("/close", post(claim::close))
        .route("/drop", post(claim::drop))
        .route("/reopen", post(claim::reopen))
        .route("/wait", post(turn::wait))
        .route("/resume", post(turn::resume))
        .route("/ask", post(turn::ask))
        .route("/park", post(turn::ask))
        .route("/reply", post(turn::reply))
        .route("/retry", post(retry::retry))
        .route("/answer", post(turn::answer))
        .route("/decide", post(turn::decide))
        .route("/priority", post(fields::priority))
        .route("/rate", post(fields::rate))
        .route("/edit", post(fields::edit))
        .route("/link", post(fields::link))
        .route("/fold", post(fields::fold))
        .route("/project", post(project::project))
        .route("/reindex", post(project::reindex))
}

#[cfg(test)]
#[path = "../tests/verbs.rs"]
mod tests;
