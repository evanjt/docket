//! `docket`: the work register's command line, a client of the docket server.

mod args;
mod cmd;
mod ctx;
mod dispatch;
mod fail;
mod http;
mod job;
mod local;
mod py;
mod row;
mod templates;

use std::io::IsTerminal;
use std::process::ExitCode;

use clap::Parser;

use crate::args::{Cli, Cmd};
use crate::ctx::Ctx;
use crate::fail::Fail;

/// The bare command on a terminal: the screen, opened on the project this directory resolves to.
fn screen(cli: &Cli) -> ExitCode {
    let config = match docket_client::Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("docket: {e}");
            return ExitCode::from(1);
        }
    };
    let project = Ctx::new(false, cli.project.clone(), cli.branch.clone())
        .and_then(|mut ctx| ctx.project())
        .ok();
    docket_tui::run::main(config, project, "docket")
}

/// Ends the process quietly when the reader of its output goes away, as a pipe into `head` does.
#[cfg(unix)]
fn restore_sigpipe() {
    // SAFETY: runs once at startup, before any thread exists or anything is written.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

fn main() -> ExitCode {
    restore_sigpipe();
    let cli = Cli::parse();
    if cli.cmd.is_none() && !cli.json && !cli.plain && std::io::stdout().is_terminal() {
        return screen(&cli);
    }
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
    if let Some(Cmd::Job { what }) = &cli.cmd {
        let flags = cmd::job::Flags {
            json: cli.json,
            project: cli.project.clone(),
            branch: cli.branch.clone(),
        };
        return finish(cmd::job::job(&flags, what));
    }
    finish(
        Ctx::new(cli.json, cli.project.clone(), cli.branch.clone())
            .and_then(|mut ctx| cmd::run(&mut ctx, cli.cmd.as_ref())),
    )
}

/// The exit code a command ends on, with its refusal on stderr.
fn finish(code: fail::Result<i32>) -> ExitCode {
    match code {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(Fail::Refused(text)) => {
            eprintln!("{text}");
            ExitCode::from(1)
        }
    }
}
