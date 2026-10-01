//! A push stream that says when the database changed, so a client refetches on a move, not a clock.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use futures_util::stream::{self, Stream};
use sea_orm::DatabaseConnection;
use sea_orm::sqlx::postgres::PgListener;
use tokio::sync::{OnceCell, watch};

/// The channel every write transaction notifies as it commits.
pub const CHANNEL: &str = "docket_changes";

/// How long the watcher waits before listening again after its connection fails.
const RETRY: Duration = Duration::from_secs(1);

/// One listener per server, started by the first subscriber and shared by every later one.
pub struct Changes {
    db: DatabaseConnection,
    seen: OnceCell<watch::Receiver<u64>>,
    stopping: watch::Receiver<bool>,
}

/// `GET /changes`: an event stream, `hello` on connecting and `change` after each commit. Every
/// stream ends once `stopping` turns true, so a server shutting down is not held open by them.
pub fn router(db: &DatabaseConnection, stopping: watch::Receiver<bool>) -> Router {
    let state = Arc::new(Changes {
        db: db.clone(),
        seen: OnceCell::new(),
        stopping,
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
            let listener = listen(&state.db).await;
            tokio::spawn(relay(state.db.clone(), listener, tx));
            rx
        })
        .await
        .clone();
    let hello = Event::default()
        .event("hello")
        .data(rx.borrow().to_string());
    let rest = stream::unfold(
        (rx, state.stopping.clone()),
        |(mut rx, mut stopping)| async move {
            tokio::select! {
                () = stopped(&mut stopping) => None,
                changed = rx.changed() => {
                    if changed.is_err() {
                        stopped(&mut stopping).await;
                        return None;
                    }
                    let n = *rx.borrow_and_update();
                    let event = Event::default().event("change").data(n.to_string());
                    Some((Ok(event), (rx, stopping)))
                }
            }
        },
    );
    let all = futures_util::StreamExt::chain(stream::once(async { Ok(hello) }), rest);
    Sse::new(all).keep_alive(KeepAlive::default())
}

/// Resolves once the server is stopping; never, when nothing can stop it.
async fn stopped(stopping: &mut watch::Receiver<bool>) {
    if stopping.wait_for(|s| *s).await.is_err() {
        std::future::pending::<()>().await;
    }
}

/// A connection of its own listening on the channel, or none when the database cannot be reached.
async fn listen(db: &DatabaseConnection) -> Option<PgListener> {
    let mut listener = PgListener::connect_with(db.get_postgres_connection_pool())
        .await
        .ok()?;
    listener.listen(CHANNEL).await.ok()?;
    Some(listener)
}

/// Counts each notice into the watch. A notice lost while the connection is down is not counted.
async fn relay(db: DatabaseConnection, mut listener: Option<PgListener>, tx: watch::Sender<u64>) {
    loop {
        let Some(l) = listener.as_mut() else {
            tokio::time::sleep(RETRY).await;
            listener = listen(&db).await;
            continue;
        };
        match l.try_recv().await {
            Ok(Some(_)) => tx.send_modify(|n| *n += 1),
            Ok(None) => {}
            Err(_) => {
                tokio::time::sleep(RETRY).await;
                listener = None;
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/changes.rs"]
mod tests;
