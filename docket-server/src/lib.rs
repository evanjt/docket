//! The docket server: one process over one database, read by every client through these routes.

pub mod auth;
pub mod changes;
pub mod entities;
pub mod reads;
pub mod show;
mod store;
mod verbs;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use sea_orm::sqlx::sqlite::SqliteJournalMode;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};
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
    let mut options = ConnectOptions::new(format!("sqlite://{path}?mode=rw"));
    options.max_connections(CONNECTIONS);
    options.map_sqlx_sqlite_opts(|o| {
        o.journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(10))
    });
    Database::connect(options).await
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
