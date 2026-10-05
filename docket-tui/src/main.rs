use std::process::ExitCode;

use docket_client::Config;

/// `-p SLUG`, `--project SLUG` or `DOCKET_PROJECT`: the project to open on.
fn project_arg() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "-p" || a == "--project" {
            return args.next();
        }
    }
    std::env::var("DOCKET_PROJECT")
        .ok()
        .filter(|p| !p.is_empty())
}

fn main() -> ExitCode {
    match Config::load() {
        Ok(config) => docket_tui::run::main(config, project_arg(), "docket-tui", None),
        Err(e) => {
            eprintln!("docket-tui: {e}");
            ExitCode::from(2)
        }
    }
}
