//! The lists whose order or content is a rule, each in the shape its Python command's `--json` prints.

pub mod audit;
pub mod board;
pub mod changelog;
pub mod detail;
pub mod dump;
pub mod lists;
pub mod metrics;
pub(crate) mod public;
pub mod queue;
pub(crate) mod rows;
pub mod search;

use axum::Router;
use axum::routing::get;
use sea_orm::DatabaseConnection;

/// Every list route; each takes the project as `?project=SLUG`.
pub fn router() -> Router<DatabaseConnection> {
    Router::new()
        .route("/next", get(queue::next))
        .route("/next/halt", get(queue::halt))
        .route("/releases", get(crate::verbs::releases::list))
        .route("/areas", get(crate::verbs::areas::list))
        .route("/labels", get(crate::verbs::labels::list))
        .route("/changelog", get(changelog::changelog_of))
        .route("/metrics", get(metrics::metrics))
        .route("/status", get(queue::status))
        .route("/todo", get(lists::todo))
        .route("/todo/waiting", get(lists::todo_waiting))
        .route("/wip", get(lists::wip))
        .route("/waiting", get(lists::waiting))
        .route("/questions", get(lists::questions))
        .route("/research", get(lists::research))
        .route("/derived", get(lists::derived))
        .route("/done", get(lists::done))
        .route("/dropped", get(lists::dropped))
        .route("/groups", get(lists::groups))
        .route("/under", get(lists::under))
        .route("/search", get(search::search))
        .route("/similar/{id}", get(search::similar))
        .route("/dump", get(dump::dump))
        .route("/whoami", get(board::whoami))
        .route("/counts", get(board::counts))
        .route("/summary", get(board::summary))
        .route("/check", get(board::check))
        .route("/graph", get(board::graph))
        .route("/shares", get(board::shares))
        .route("/log/{id}", get(detail::log))
        .route("/deps/{id}", get(detail::deps))
        .route("/context/{id}", get(detail::context))
        .route("/files", get(detail::files))
        .route("/citations", get(detail::citations))
        .route("/open_bodies", get(detail::open_bodies))
        .route("/audit", get(audit::audit))
}

#[cfg(test)]
#[path = "../tests/reads.rs"]
mod tests;
