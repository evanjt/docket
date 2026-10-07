//! The lists whose order or content is a rule, each in the shape the matching command's `--json` prints.

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

use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::Response;
use axum::routing::get;
use sea_orm::DatabaseConnection;
use tokio::sync::Semaphore;

/// Every list route; each takes the project as `?project=SLUG`. The reads that build a whole board
/// or more share `gate`: one waits for a permit before it touches the database.
pub fn router(gate: &Arc<Semaphore>) -> Router<DatabaseConnection> {
    wide(gate).merge(narrow())
}

/// A project-wide read holds a permit from before its first query until it has answered, so the
/// reads in flight, and the rows they hold, stay bounded however many requests arrive. A request that
/// waits holds no rows.
pub(crate) async fn hold(
    State(gate): State<Arc<Semaphore>>,
    request: Request,
    next: Next,
) -> Response {
    let _permit = gate.acquire().await;
    next.run(request).await
}

/// The reads that load a whole board, or the whole queue, for one request.
fn wide(gate: &Arc<Semaphore>) -> Router<DatabaseConnection> {
    Router::new()
        .route("/next", get(queue::next))
        .route("/metrics", get(metrics::metrics))
        .route("/status", get(queue::status))
        .route("/summary", get(board::summary))
        .route("/check", get(board::check))
        .route("/graph", get(board::graph))
        .layer(from_fn_with_state(Arc::clone(gate), hold))
}

fn narrow() -> Router<DatabaseConnection> {
    Router::new()
        .route("/next/halt", get(queue::halt))
        .route("/releases", get(crate::verbs::releases::list))
        .route("/areas", get(crate::verbs::areas::list))
        .route("/labels", get(crate::verbs::labels::list))
        .route("/changelog", get(changelog::changelog_of))
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
        .route("/shares", get(board::shares))
        .route("/held", get(board::held))
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
