use std::env;

use docket_server::auth::Keys;
use docket_server::{app, connect, connect_read_only, read_only};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::var("DOCKET_DB")?;
    let keys = Keys::parse(&std::fs::read_to_string(env::var("DOCKET_KEYS")?)?)?;
    let listen = env::var("DOCKET_LISTEN").unwrap_or_else(|_| "127.0.0.1:7878".to_string());

    let reading = matches!(env::var("DOCKET_READ_ONLY").as_deref(), Ok("1" | "true"));

    let db = if reading {
        connect_read_only(&path).await?
    } else {
        connect(&path).await?
    };
    let routes = if reading {
        read_only(app(&db, keys))
    } else {
        app(&db, keys)
    };
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    let how = if reading { "reading" } else { "over" };
    println!("docket-server on {listen}, {how} {path}");
    axum::serve(listener, routes).await?;
    Ok(())
}
