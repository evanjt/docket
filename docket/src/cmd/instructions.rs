//! `docket instructions`: the docket block in the AGENTS.md at the project's root.

use std::path::Path;

use crate::cmd::skills::{home_relative, list, write_steps};
use crate::ctx::{Ctx, root_at};
use crate::fail::{Fail, Result};
use crate::local;
use crate::templates;

/// # Errors
/// The project has no root on this machine, nobody can be asked, or a file cannot be written.
pub fn instructions(ctx: &mut Ctx, what: &str, yes: bool) -> Result<i32> {
    let slug = ctx.project()?;
    let real = local::realpath(&ctx.cwd);
    let (top, _) = local::outermost_repo(&ctx.cwd);
    let top = top.map(|t| t.display().to_string());
    let Some(root) = root_at(&ctx.roots.roots, &slug, &real, top.as_deref()) else {
        return Err(Fail::refused(format!(
            "{real} is in none of {slug}'s roots on this machine: run docket from one of its checkouts, or docket bind"
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
