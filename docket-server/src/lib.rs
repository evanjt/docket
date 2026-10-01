//! The docket server: one process over one database, read by every client through these routes.

pub mod auth;
pub mod changes;
pub mod entities;
mod facts;
pub mod import;
pub mod reads;
pub mod show;
mod store;
mod verbs;

use std::future::Future;
use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend, DbErr};
use tokio::net::TcpListener;
use tokio::sync::watch;
use utoipa_axum::router::OpenApiRouter;

use crate::auth::{Keys, require_key};
use crate::entities::{event::Event, item::Item, link::Link, project::Project};

pub use docket_migration::migrate;

const CONNECTIONS: u32 = 8;

/// A pool over the Postgres database at `url`. Each write holds one connection for its transaction.
///
/// # Errors
/// The database cannot be reached, or the URL names another kind of database.
pub async fn connect(url: &str) -> Result<DatabaseConnection, DbErr> {
    let mut options = ConnectOptions::new(docket_migration::postgres_url(url));
    options.max_connections(CONNECTIONS);
    let db = Database::connect(options).await?;
    if db.get_database_backend() != DbBackend::Postgres {
        return Err(DbErr::Custom(
            "the URL does not name a Postgres database".into(),
        ));
    }
    Ok(db)
}

/// Every route. All but `/health` sit behind a key, and only the verbs under `/do` write.
pub fn app(db: &DatabaseConnection, keys: Keys) -> Router {
    routes(db, keys, watch::channel(false).1)
}

/// The routes served until `stop` resolves, then every request in flight finished and every change
/// stream ended.
///
/// # Errors
/// The listener fails.
pub async fn serve(
    listener: TcpListener,
    db: &DatabaseConnection,
    keys: Keys,
    stop: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let (stopping, watched) = watch::channel(false);
    let shutdown = async move {
        stop.await;
        stopping.send_replace(true);
    };
    axum::serve(listener, routes(db, keys, watched))
        .with_graceful_shutdown(shutdown)
        .await
}

fn routes(db: &DatabaseConnection, keys: Keys, stopping: watch::Receiver<bool>) -> Router {
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
        .merge(changes::router(db, stopping))
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
