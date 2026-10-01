//! The lists whose order or content is a rule, each in the shape its Python command's `--json` prints.

pub mod lists;
pub(crate) mod public;
pub mod queue;
pub mod search;

use axum::Router;
use axum::routing::get;
use sea_orm::DatabaseConnection;

/// Every list route; each takes the project as `?project=SLUG`.
pub fn router() -> Router<DatabaseConnection> {
    Router::new()
        .route("/next", get(queue::next))
        .route("/status", get(queue::status))
        .route("/todo", get(lists::todo))
        .route("/wip", get(lists::wip))
        .route("/waiting", get(lists::waiting))
        .route("/questions", get(lists::questions))
        .route("/research", get(lists::research))
        .route("/derived", get(lists::derived))
        .route("/done", get(lists::done))
        .route("/dropped", get(lists::dropped))
        .route("/groups", get(lists::groups))
        .route("/search", get(search::search))
        .route("/similar/{id}", get(search::similar))
}

#[cfg(test)]
#[path = "../tests/reads.rs"]
mod tests;
