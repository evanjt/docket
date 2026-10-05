//! `docket label` and `docket labels`: a label given to an item or taken from it, and a project's
//! labels with their descriptions.

use docket_core::api::{LabelDone, LabelRequest, LabelRow};
use docket_core::label;

use crate::ctx::{Ctx, id};
use crate::fail::Result;

/// One label a line: its name, how many items were given it and what it means.
#[must_use]
pub fn line(r: &LabelRow) -> String {
    let mut parts = vec![r.label.name.clone(), format!("({})", r.items)];
    parts.extend(r.label.description.clone());
    parts.join("  ")
}

/// # Errors
/// The server refuses.
pub fn label(
    ctx: &mut Ctx,
    action: &str,
    item: &str,
    name: &str,
    about: Option<&str>,
) -> Result<i32> {
    let req = LabelRequest {
        common: ctx.common(false)?,
        action: action.to_string(),
        id: id(item)?,
        name: name.to_string(),
        about: about.map(str::to_string),
    };
    let out: LabelDone = ctx.api.post("label", &req)?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return Ok(0);
    }
    let names: Vec<String> = out.labels.iter().map(label::line).collect();
    if names.is_empty() {
        println!("{}: no labels", out.id);
    } else {
        println!("{}: {}", out.id, names.join("; "));
    }
    Ok(0)
}

/// # Errors
/// The server refuses.
pub fn list(ctx: &mut Ctx) -> Result<i32> {
    let slug = ctx.project()?;
    let found = ctx.api.get("/labels", &[("project", slug)])?;
    let rows: Vec<LabelRow> = serde_json::from_value(found).unwrap_or_default();
    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
    } else if rows.is_empty() {
        println!("no labels: docket label add ID NAME --about \"what it means\"");
    }
    for r in &rows {
        if !ctx.json {
            println!("{}", line(r));
        }
    }
    Ok(0)
}

#[cfg(test)]
#[path = "../tests/labels.rs"]
mod tests;
