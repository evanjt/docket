//! Scenario: the server runs as its own process, as PID 1 does in a container, and is sent SIGTERM.
//! Expected behaviour: it exits at once with success, rather than waiting to be killed.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use docket_migration::scratch::Scratch;

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn listening(port: u16) -> bool {
    let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    s.write_all(b"GET /health HTTP/1.0\r\n\r\n").is_ok()
}

#[tokio::test]
async fn test_server_exits_promptly_on_sigterm() {
    let db = Scratch::bare(1).await;
    let keys = std::env::temp_dir().join(format!("docket-shutdown-keys-{}", std::process::id()));
    std::fs::write(&keys, "box owner k\n").unwrap();
    let port = free_port();
    let mut server = Command::new(env!("CARGO_BIN_EXE_docket-server"))
        .env("DATABASE_URL", db.url())
        .env("DOCKET_KEYS", &keys)
        .env("DOCKET_LISTEN", format!("127.0.0.1:{port}"))
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !listening(port) {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the server did not start"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let sent = Command::new("kill")
        .args(["-TERM", &server.id().to_string()])
        .status()
        .unwrap();
    assert!(sent.success());
    let asked = Instant::now();
    let status = loop {
        if let Some(status) = server.try_wait().unwrap() {
            break status;
        }
        if asked.elapsed() > Duration::from_secs(3) {
            server.kill().ok();
            panic!("the server was still running 3 s after SIGTERM");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    std::fs::remove_file(&keys).ok();
    assert!(status.success(), "{status}");
}
