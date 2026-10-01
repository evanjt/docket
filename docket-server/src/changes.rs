//! A push stream that says when the database changed, so a client refetches on a move, not a clock.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use futures_util::stream::{self, Stream};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use tokio::sync::{OnceCell, watch};

/// How often the watcher asks SQLite whether another connection committed.
pub const EVERY: Duration = Duration::from_millis(500);

/// One watcher per server, started by the first subscriber and shared by every later one.
pub struct Changes {
    db: DatabaseConnection,
    seen: OnceCell<watch::Receiver<u64>>,
}

/// `GET /changes`: an event stream, `hello` on connecting and `change` after each commit.
pub fn router(db: &DatabaseConnection) -> Router {
    let state = Arc::new(Changes {
        db: db.clone(),
        seen: OnceCell::new(),
    });
    Router::new()
        .route("/changes", get(changes))
        .with_state(state)
}

async fn changes(
    State(state): State<Arc<Changes>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state
        .seen
        .get_or_init(|| async {
            let (tx, rx) = watch::channel(0);
            tokio::spawn(watch_file(state.db.clone(), tx));
            rx
        })
        .await
        .clone();
    let hello = Event::default()
        .event("hello")
        .data(rx.borrow().to_string());
    let rest = stream::unfold(rx, |mut rx| async move {
        if rx.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
        let n = *rx.borrow_and_update();
        Some((Ok(Event::default().event("change").data(n.to_string())), rx))
    });
    let all = futures_util::StreamExt::chain(stream::once(async { Ok(hello) }), rest);
    Sse::new(all).keep_alive(KeepAlive::default())
}

/// The file behind the main schema, `None` for an in-memory database.
async fn main_file(db: &DatabaseConnection) -> Option<String> {
    let rows = db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA database_list",
        ))
        .await
        .ok()?;
    rows.iter()
        .find(|r| r.try_get::<String>("", "name").is_ok_and(|n| n == "main"))
        .and_then(|r| r.try_get::<String>("", "file").ok())
        .filter(|f| !f.is_empty())
}

/// `PRAGMA data_version` is per connection, so the watcher keeps one connection of its own: the value
/// moves whenever any other connection, this server's or another process's, commits.
async fn watch_file(db: DatabaseConnection, tx: watch::Sender<u64>) {
    let Some(path) = main_file(&db).await else {
        return;
    };
    let mut options = ConnectOptions::new(format!("sqlite://{path}?mode=ro"));
    options
        .max_connections(1)
        .min_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .sqlx_logging(false);
    let Ok(own) = Database::connect(options).await else {
        return;
    };
    let mut last = None;
    let mut tick = tokio::time::interval(EVERY);
    loop {
        tick.tick().await;
        let version = data_version(&own).await;
        if version.is_some() && last.is_some() && version != last {
            tx.send_modify(|n| *n += 1);
        }
        if version.is_some() {
            last = version;
        }
    }
}

async fn data_version(db: &DatabaseConnection) -> Option<i64> {
    db.query_one_raw(Statement::from_string(
        DbBackend::Sqlite,
        "PRAGMA data_version",
    ))
    .await
    .ok()
    .flatten()
    .and_then(|r| r.try_get_by_index::<i64>(0).ok())
}

#[cfg(test)]
#[path = "tests/changes.rs"]
mod tests;
