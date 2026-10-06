//! `docket-dump`: the docket database written one way into a git checkout, and rebuilt from one.

mod client;
mod git;
mod restore;
mod run;
mod tree;

use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::thread::sleep;
use std::time::Duration;

use clap::Parser;

use docket_client::{Api, Config};

use crate::client::Source;
use crate::git::Pushed;

/// Writes the rows changed since the last pass into a dump checkout, commits them and pushes.
/// The server and key come from `DOCKET_SERVER` and `DOCKET_KEY`, or `~/.config/docket/client`.
// Each flag is a switch on the command line.
#[allow(clippy::struct_excessive_bools)]
#[derive(Parser)]
#[command(name = "docket-dump", version)]
struct Args {
    /// The dump checkout. Its cursor is kept in its own git config, never committed.
    #[arg(long)]
    repo: PathBuf,
    /// One pass, then exit; the default.
    #[arg(long, conflicts_with = "every")]
    once: bool,
    /// A pass every this many seconds until stopped; a failed pass is reported and retried.
    #[arg(long, value_name = "SECONDS", value_parser = clap::value_parser!(u64).range(1..))]
    every: Option<u64>,
    /// Commit, and leave the commits for a later push.
    #[arg(long)]
    no_push: bool,
    /// Every row rather than those after the cursor, for a fresh checkout or a check.
    #[arg(long)]
    full: bool,
    /// Rebuild an empty database from the checkout, for disaster recovery only.
    #[arg(long, conflicts_with_all = ["once", "every", "no_push", "full"])]
    restore: bool,
    /// The Postgres database a restore fills, migrated first; `DATABASE_URL` when not given.
    #[arg(long, requires = "restore", value_name = "URL")]
    database_url: Option<String>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let done = if args.restore {
        restore(&args)
    } else {
        dump(&args)
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("docket-dump: {e}");
            ExitCode::FAILURE
        }
    }
}

fn dump(args: &Args) -> Result<(), String> {
    let source = Api::new(&Config::load()?).map_err(|e| e.to_string())?;
    let host = host();
    let Some(every) = args.every else {
        return once(&source, args, args.full, &host);
    };
    let mut full = args.full;
    loop {
        match once(&source, args, full, &host) {
            Ok(()) => full = false,
            Err(e) => eprintln!("docket-dump: {e}"),
        }
        sleep(Duration::from_secs(every));
    }
}

/// One pass and its push. A push that fails leaves its commits in the checkout for the next one.
fn once(source: &impl Source, args: &Args, full: bool, host: &str) -> Result<(), String> {
    let report = run::pass(source, &args.repo, full, host)?;
    println!("{report}");
    if args.no_push {
        return Ok(());
    }
    let pushed = git::push(&args.repo)
        .map_err(|e| format!("{e}; the commits stay in the checkout for the next push"))?;
    match pushed {
        Pushed::NoOrigin => println!("no origin remote: committed only"),
        Pushed::UpToDate => {}
        Pushed::Commits(n) => println!("pushed {n} commits"),
    }
    Ok(())
}

fn restore(args: &Args) -> Result<(), String> {
    let url = args
        .database_url
        .clone()
        .or_else(|| env::var("DATABASE_URL").ok())
        .ok_or("--restore needs --database-url URL or DATABASE_URL")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let counts = runtime.block_on(restore::restore(&args.repo, &url))?;
    git::forget_cursor(&args.repo)?;
    println!(
        "restored {} projects, {} items and {} events; the checkout's cursor is cleared, so its next pass writes every row",
        counts.projects, counts.items, counts.events
    );
    Ok(())
}

/// The name a full dump's commit carries.
fn host() -> String {
    if let Some(h) = env::var("DOCKET_HOST").ok().filter(|h| !h.is_empty()) {
        return h;
    }
    Command::new("hostname")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}
