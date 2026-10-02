//! A docket server on a free port over a seeded scratch database, for tests that drive the built
//! client against it. Dropping it stops the server and drops the database.

use std::net::SocketAddr;
use std::sync::mpsc;

use docket_migration::scratch::Scratch;
use docket_server::app;
use docket_server::auth::Keys;

pub struct Server {
    pub addr: SocketAddr,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.send(()).ok();
        }
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
    }
}

/// A server over a database seeded with `seed`, answering the keys file `keys`.
#[must_use]
pub fn serve(seed: &'static str, keys: &'static str) -> Server {
    let (tx, rx) = mpsc::channel::<SocketAddr>();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let db = Scratch::new(2).await;
            db.seed(seed).await;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            tx.send(listener.local_addr().unwrap()).unwrap();
            axum::serve(listener, app(&db.db, Keys::parse(keys).unwrap()))
                .with_graceful_shutdown(async {
                    stopped.await.ok();
                })
                .await
                .unwrap();
        });
    });
    let addr = rx.recv().unwrap();
    Server {
        addr,
        stop: Some(stop),
        thread: Some(thread),
    }
}
