use std::env;

use docket_server::auth::Keys;
use docket_server::{app, connect};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::var("DOCKET_DB")?;
    let keys = Keys::parse(&std::fs::read_to_string(env::var("DOCKET_KEYS")?)?)?;
    let listen = env::var("DOCKET_LISTEN").unwrap_or_else(|_| "127.0.0.1:7878".to_string());

    let db = connect(&path).await?;
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    println!("docket-server on {listen}, over {path}");
    axum::serve(listener, app(&db, keys)).await?;
    Ok(())
}
