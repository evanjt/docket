//! A push stream that says when the database changed, so a client refetches on a move, not a clock.

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use futures_util::stream::{self, Stream};
use sea_orm::DatabaseConnection;
use sea_orm::sqlx::postgres::PgListener;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{OnceCell, broadcast, watch};

/// The channel every write transaction notifies as it commits.
pub const CHANNEL: &str = "docket_changes";

/// How long the watcher waits before listening again after its connection fails.
const RETRY: Duration = Duration::from_secs(1);

/// One listener per server, started by the first subscriber and shared by every later one.
pub struct Changes {
    db: DatabaseConnection,
    feed: OnceCell<Feed>,
    stopping: watch::Receiver<bool>,
}

/// The commits so far, counted, and each one as it is heard.
struct Feed {
    count: Arc<AtomicU64>,
    sent: broadcast::Sender<Change>,
}

/// One commit: its number and the projects it wrote to. No project listed means the commit did not
/// say, or the stream fell behind, and a client takes it as a change to any of them.
#[derive(Clone)]
struct Change {
    n: u64,
    projects: Vec<String>,
}

impl Change {
    fn data(&self) -> String {
        serde_json::json!({"n": self.n, "projects": self.projects}).to_string()
    }
}

/// The notice payload for the projects a commit wrote to; empty when the list would not fit in one.
#[must_use]
pub fn payload(projects: &std::collections::BTreeSet<String>) -> String {
    let text = serde_json::json!(projects).to_string();
    if text.len() > PAYLOAD_MAX {
        String::new()
    } else {
        text
    }
}

/// The most a notice payload may carry; Postgres refuses 8000 bytes or more.
const PAYLOAD_MAX: usize = 7000;

/// `GET /changes`: an event stream, `hello` on connecting and `change` after each commit. Every
/// stream ends once `stopping` turns true, so a server shutting down is not held open by them.
pub fn router(db: &DatabaseConnection, stopping: watch::Receiver<bool>) -> Router {
    let state = Arc::new(Changes {
        db: db.clone(),
        feed: OnceCell::new(),
        stopping,
    });
    Router::new()
        .route("/changes", get(changes))
        .with_state(state)
}

async fn changes(
    State(state): State<Arc<Changes>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let feed = state
        .feed
        .get_or_init(|| async {
            let (sent, _) = broadcast::channel(64);
            let count = Arc::new(AtomicU64::new(0));
            let listener = listen(&state.db).await;
            tokio::spawn(relay(
                listener,
                state.db.clone(),
                count.clone(),
                sent.clone(),
            ));
            Feed { count, sent }
        })
        .await;
    let rx = feed.sent.subscribe();
    let count = feed.count.clone();
    let hello = Event::default()
        .event("hello")
        .data(count.load(Ordering::SeqCst).to_string());
    let rest = stream::unfold(
        (rx, count, state.stopping.clone()),
        |(mut rx, count, mut stopping)| async move {
            let change = tokio::select! {
                () = stopped(&mut stopping) => return None,
                got = rx.recv() => match got {
                    Ok(change) => change,
                    Err(RecvError::Lagged(_)) => Change {
                        n: count.load(Ordering::SeqCst),
                        projects: Vec::new(),
                    },
                    Err(RecvError::Closed) => {
                        stopped(&mut stopping).await;
                        return None;
                    }
                },
            };
            let event = Event::default().event("change").data(change.data());
            Some((Ok(event), (rx, count, stopping)))
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

/// Numbers each notice and passes it on with the projects it names. A notice lost while the
/// connection is down is not counted.
async fn relay(
    mut listener: Option<PgListener>,
    db: DatabaseConnection,
    count: Arc<AtomicU64>,
    sent: broadcast::Sender<Change>,
) {
    loop {
        let Some(l) = listener.as_mut() else {
            tokio::time::sleep(RETRY).await;
            listener = listen(&db).await;
            continue;
        };
        match l.try_recv().await {
            Ok(Some(notice)) => {
                let projects = serde_json::from_str(notice.payload()).unwrap_or_default();
                let n = count.fetch_add(1, Ordering::SeqCst) + 1;
                let _ = sent.send(Change { n, projects });
            }
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
