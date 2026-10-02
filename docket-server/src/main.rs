use std::env;

use clap::{Parser, Subcommand};
use tokio::signal::unix::{SignalKind, signal};

use docket_server::auth::Keys;
use docket_server::{connect, import, migrate, serve};

/// The docket server over the Postgres database `DATABASE_URL` names. It applies any migration the
/// database lacks before it listens on `DOCKET_LISTEN`, with the keys in the file `DOCKET_KEYS`, and
/// serves the web client at `/ui/` from the directory `DOCKET_WEB` when it is set.
#[derive(Parser)]
#[command(name = "docket-server", version)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Copy a SQLite docket database into the empty Postgres one, then exit.
    Import {
        /// The SQLite file, a copy taken with `sqlite3 docket.db ".backup copy.db"`.
        #[arg(long, value_name = "SQLITE_FILE")]
        from: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let db = connect(&env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is not set")?).await?;
    let ready = ready(&migrate(&db).await?);
    if let Some(Command::Import { from }) = args.command {
        println!("docket-server: {ready}");
        return run_import(&from, &db).await;
    }
    let keys = Keys::parse(&std::fs::read_to_string(env::var("DOCKET_KEYS")?)?)?;
    let listen = env::var("DOCKET_LISTEN").unwrap_or_else(|_| "127.0.0.1:7878".to_string());
    let web = env::var_os("DOCKET_WEB").map(std::path::PathBuf::from);
    if let Some(dir) = web.as_ref().filter(|d| !d.join("index.html").is_file()) {
        return Err(format!("DOCKET_WEB is {}, which holds no index.html", dir.display()).into());
    }
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    println!(
        "docket-server listening on {listen} (clients reach it at the server in their client config, \
         through Traefik in compose); {ready}"
    );
    if let Some(dir) = &web {
        println!(
            "docket-server: the web client at /ui/, from {}",
            dir.display()
        );
    }
    serve(listener, &db, keys, web.as_deref(), stop_signal()?).await?;
    db.close().await?;
    println!("docket-server stopped");
    Ok(())
}

/// Resolves on the first SIGTERM or SIGINT. The handlers are in place once this returns.
fn stop_signal() -> std::io::Result<impl Future<Output = ()>> {
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    Ok(async move {
        tokio::select! {
            _ = term.recv() => {}
            _ = int.recv() => {}
        }
    })
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

/// The copy, and its rows by table and project.
async fn run_import(
    from: &str,
    db: &sea_orm::DatabaseConnection,
) -> Result<(), Box<dyn std::error::Error>> {
    let counts = import::import(from, db).await?;
    for ((table, project), n) in &counts {
        let project = if project.is_empty() { "-" } else { project };
        println!("{table:<14} {project:<40} {n:>8}");
    }
    println!("docket-server: imported {from}; every count matches");
    Ok(())
}

#[cfg(test)]
#[path = "tests/main.rs"]
mod tests;
