//! `docket`: the work register's command line, a client of the docket server.

mod args;
mod cmd;
mod ctx;
mod fail;
mod http;
mod local;
mod py;
mod row;

use std::process::ExitCode;

use clap::Parser;

use crate::args::{Cli, Cmd};
use crate::ctx::Ctx;
use crate::fail::Fail;

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Some(Cmd::Link { words, .. }) = &cli.cmd
        && let Some(kind) = words.get(words.len().saturating_sub(2))
        && kind != "related"
        && kind != "opened"
    {
        eprintln!(
            "docket link: error: argument kind: invalid choice: '{kind}' (choose from 'related', 'opened')"
        );
        return ExitCode::from(2);
    }
    let code = Ctx::new(cli.json, cli.project.clone(), cli.branch.clone())
        .and_then(|mut ctx| cmd::run(&mut ctx, cli.cmd.as_ref()));
    match code {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(Fail::Refused(text)) => {
            eprintln!("{text}");
            ExitCode::from(1)
        }
    }
}
