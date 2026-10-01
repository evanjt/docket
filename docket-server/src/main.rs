use std::env;

use docket_server::auth::Keys;
use docket_server::{app, connect, migrate};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = connect(&env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is not set")?).await?;
    let ready = ready(&migrate(&db).await?);
    let keys = Keys::parse(&std::fs::read_to_string(env::var("DOCKET_KEYS")?)?)?;
    let listen = env::var("DOCKET_LISTEN").unwrap_or_else(|_| "127.0.0.1:7878".to_string());
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    println!(
        "docket-server listening on {listen} (clients reach it at the server in their client config, \
         through Traefik in compose); {ready}"
    );
    axum::serve(listener, app(&db, keys)).await?;
    Ok(())
}

/// What the migrations did, named without the database's address or credentials.
fn ready(applied: &[String]) -> String {
    match applied {
        [] => "database ready, no migrations applied".to_string(),
        [one] => format!("database ready, 1 migration applied ({one})"),
        many => format!(
            "database ready, {} migrations applied ({})",
            many.len(),
            many.join(", ")
        ),
    }
}

#[cfg(test)]
#[path = "tests/main.rs"]
mod tests;
