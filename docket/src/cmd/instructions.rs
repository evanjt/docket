//! `docket instructions`: the docket block in the AGENTS.md at the project's root.

use std::path::Path;

use crate::cmd::skills::{home_relative, list, root_of, write_steps};
use crate::ctx::Ctx;
use crate::fail::{Fail, Result};
use crate::templates;

/// # Errors
/// The project has no root on this machine, nobody can be asked, or a file cannot be written.
pub fn instructions(ctx: &mut Ctx, what: &str, yes: bool) -> Result<i32> {
    let slug = ctx.project()?;
    let Some(root) = root_of(ctx, &slug) else {
        return Err(Fail::refused(format!(
            "{slug} has no root on this machine: run docket from its checkout once, or docket bind"
        )));
    };
    let home = std::env::var("HOME").unwrap_or_default();
    let steps = templates::instruction_steps(Path::new(&root));
    println!("{slug} at {}:", home_relative(&root, &home));
    if what == "diff" {
        list(&steps, &home);
        return Ok(0);
    }
    write_steps(&steps, &home, yes)
}
