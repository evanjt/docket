//! The docket server: one process over one database, read by every client through these routes.

pub mod auth;
pub mod changes;
pub mod entities;
mod facts;
pub mod reads;
pub mod show;
mod store;
mod verbs;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::Request;
use axum::http::{Method, StatusCode};
use axum::middleware::{Next, from_fn, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use sea_orm::sqlx::sqlite::SqliteJournalMode;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};
use serde_json::json;
use utoipa_axum::router::OpenApiRouter;

use crate::auth::{Keys, require_key};
use crate::entities::{event::Event, item::Item, link::Link, project::Project};

const CONNECTIONS: u32 = 8;

/// Opens the database file read-write, in WAL mode, with foreign keys on and a busy timeout. Readers run
/// side by side on a pool; each write takes the database's one write lock for its whole transaction.
///
/// # Errors
/// The file cannot be opened as SQLite.
pub async fn connect(path: &str) -> Result<DatabaseConnection, DbErr> {
    open(path, false).await
}

/// Opens the database file for reading only, leaving its journal mode to whoever writes it.
///
/// # Errors
/// The file cannot be opened as SQLite.
pub async fn connect_read_only(path: &str) -> Result<DatabaseConnection, DbErr> {
    open(path, true).await
}

async fn open(path: &str, read_only: bool) -> Result<DatabaseConnection, DbErr> {
    let mode = if read_only { "ro" } else { "rw" };
    let mut options = ConnectOptions::new(format!("sqlite://{path}?mode={mode}"));
    options.max_connections(CONNECTIONS);
    options.map_sqlx_sqlite_opts(move |o| {
        let o = o.foreign_keys(true).busy_timeout(Duration::from_secs(10));
        if read_only {
            o
        } else {
            o.journal_mode(SqliteJournalMode::Wal)
        }
    });
    Database::connect(options).await
}

/// The routes with every request but a read refused, for a server over a database another program writes.
pub fn read_only(router: Router) -> Router {
    router.layer(from_fn(refuse_writes))
}

async fn refuse_writes(req: Request, next: Next) -> Response {
    if req.method() == Method::GET || req.method() == Method::HEAD {
        return next.run(req).await;
    }
    let refusal = Json(json!({ "refused": "this server is read-only" }));
    (StatusCode::METHOD_NOT_ALLOWED, refusal).into_response()
}

/// Every route. All but `/health` sit behind a key, and only the verbs under `/do` write.
pub fn app(db: &DatabaseConnection, keys: Keys) -> Router {
    let (resources, _) = OpenApiRouter::new()
        .nest("/projects", Project::read_only_router(db))
        .nest("/items", Item::read_only_router(db))
        .nest("/events", Event::read_only_router(db))
        .nest("/links", Link::read_only_router(db))
        .split_for_parts();
    Router::new()
        .route("/show/{id}", get(show::show))
        .merge(reads::router())
        .merge(facts::router())
        .nest("/do", verbs::router())
        .with_state(db.clone())
        .merge(changes::router(db))
        .merge(resources)
        .layer(from_fn_with_state(Arc::new(keys), require_key))
        .route("/health", get(health))
}

/// Answers the probes of whatever runs the server. It reads nothing.
async fn health() -> &'static str {
    "ok"
}

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/race.rs"]
mod race;

#[cfg(test)]
#[path = "tests/read_only.rs"]
mod read_only_tests;
