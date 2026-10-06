//! The verbs that move an item. Each takes the row as stored and the caller's context, and returns
//! the columns to set or the refusal the caller reads. The claim and the owner's turn are the item's
//! open assignment, which the verb's event moves, so no rule sets them.

use crate::item::{Ctx, Field, Item, Refused};
use crate::stall::Wait;
use crate::word::{ItemType, Kind, PRIORITIES};

pub const COMPLEXITIES: [&str; 3] = ["high", "medium", "low"];
pub const DERIVED: &str = "Derived from ";

fn chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// The turn a new item of the type opens on.
#[must_use]
pub fn default_turn(item_type: ItemType) -> &'static str {
    if item_type == ItemType::Question {
        "user"
    } else {
        "agent"
    }
}

/// Whether the caller may act on a claimed item: same branch, or force.
#[must_use]
pub fn holds(row: &Item, ctx: &Ctx) -> bool {
    match row.claim() {
        None => true,
        Some(c) => ctx.force || c.branch == ctx.branch,
    }
}

/// # Errors
/// Refused when another branch holds the item and force is not given.
pub fn require_hold(row: &Item, ctx: &Ctx, verb: &str) -> Result<(), Refused> {
    match row.claim() {
        Some(c) if !holds(row, ctx) => Err(Refused(format!(
            "{} is held by {} on {} since {}. {verb} would take it out from under that agent. Merge or unclaim the branch first, or pass --force.",
            row.id, c.branch, c.host, c.since
        ))),
        _ => Ok(()),
    }
}

/// # Errors
/// Refused when the item is done or dropped.
pub fn require_open(row: &Item, verb: &str) -> Result<(), Refused> {
    if row.state == "open" {
        return Ok(());
    }
    Err(Refused(format!(
        "{} is {} ({}); {verb} needs an open item. reopen it first if that is what you mean.",
        row.id,
        row.state,
        opt(row.resolution.as_ref())
    )))
}

/// An optional column printed as its value or `None`.
fn opt(v: Option<&String>) -> &str {
    v.map_or("None", String::as_str)
}

/// # Errors
/// Refused when the item is not open, held elsewhere, waiting or parked. `wait` is what
/// its dependencies hold it on.
pub fn start(row: &Item, ctx: &Ctx, wait: Option<&Wait>) -> Result<(), Refused> {
    require_open(row, "start")?;
    if let Some(c) = row.claim() {
        if c.branch == ctx.branch && c.host == ctx.host {
            return Err(Refused(format!(
                "{} is already yours, claimed {}.",
                row.id, c.since
            )));
        }
        if !ctx.force {
            return Err(Refused(format!(
                "{} is held by {} on {} since {}. Pick another, or --force if that claim is abandoned.",
                row.id, c.branch, c.host, c.since
            )));
        }
    }
    if let Some(w) = wait {
        let what = if w.is_condition() {
            format!("{}, a task on the owner's turn,", w.id)
        } else {
            format!("item {}", w.id)
        };
        return Err(Refused(format!(
            "{} is waiting on {what} since {}. resume it first.",
            row.id, w.since
        )));
    }
    if let Some(ask) = row.ask() {
        let note = ask
            .note
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("asked {}", ask.since));
        return Err(Refused(format!(
            "{} is the owner's turn: {note}. reply to it first if you are taking it back.",
            row.id
        )));
    }
    Ok(())
}

/// # Errors
/// Refused when the item is not open, not claimed, or held elsewhere.
pub fn release(row: &Item, ctx: &Ctx) -> Result<(), Refused> {
    require_open(row, "unclaim")?;
    if row.claim().is_none() {
        return Err(Refused(format!("{} is not claimed.", row.id)));
    }
    require_hold(row, ctx, "unclaim")
}

/// The refusal for a plan with work still open under it, naming up to ten of it, then what follows.
#[must_use]
pub fn open_under_plan(row: &Item, open_members: &[String], then: &str) -> Refused {
    let shown: Vec<&str> = open_members.iter().take(10).map(String::as_str).collect();
    let more = if open_members.len() > 10 { " ..." } else { "" };
    Refused(format!(
        "{} has work under it that is still open: {}{more}. {then}",
        row.id,
        shown.join(", ")
    ))
}

/// A plan closes once nothing under it is open, or while its audit holds it: the gaps that audit
/// files are worked as tickets after the plan closes. `open_members` are the ids of the open items
/// under a plan, any depth, and are empty for any other kind.
///
/// # Errors
/// Refused when the item is not open, held elsewhere, an undecided question, given no resolution, or
/// a plan nobody holds with work under it still open.
pub fn close(
    row: &Item,
    ctx: &Ctx,
    resolution: Option<&str>,
    kind: Kind,
    open_members: &[String],
) -> Result<Vec<Field>, Refused> {
    require_open(row, "close")?;
    require_hold(row, ctx, "close")?;
    if row.claim().is_none() && !open_members.is_empty() {
        return Err(open_under_plan(
            row,
            open_members,
            "It closes only when nothing under it is open; unclaim it and it comes back when they close.",
        ));
    }
    if kind == Kind::Decision && row.decision.is_none() {
        return Err(Refused(format!(
            "{} is a decision and has none yet. answer it first, or drop it if it no longer needs one.",
            row.id
        )));
    }
    let Some(resolution) = resolution.filter(|r| !r.is_empty()) else {
        return Err(Refused(
            "close needs a resolution: the sha, or what the work opened.".to_string(),
        ));
    };
    Ok(vec![
        Field::State("done".to_string()),
        Field::Resolution(Some(resolution.to_string())),
    ])
}

/// The priority column set to one tier; the labels stay as they are.
///
/// # Errors
/// Refused when the tier is not one of the four.
pub fn prioritise(tier: &str) -> Result<Vec<Field>, Refused> {
    if !PRIORITIES.contains(&tier) {
        return Err(Refused(format!(
            "priority is one of {}, not {}",
            PRIORITIES.join(", "),
            crate::text::quoted(tier)
        )));
    }
    Ok(vec![Field::Priority(tier.to_string())])
}

/// # Errors
/// Refused when the item is not open, held elsewhere, or given no reason.
pub fn drop(
    row: &Item,
    ctx: &Ctx,
    why: Option<&str>,
    superseded_rid: Option<i64>,
) -> Result<Vec<Field>, Refused> {
    require_open(row, "drop")?;
    require_hold(row, ctx, "drop")?;
    let Some(why) = why.filter(|w| !w.is_empty()) else {
        return Err(Refused("drop needs a reason.".to_string()));
    };
    Ok(vec![
        Field::State("dropped".to_string()),
        Field::Resolution(Some(why.to_string())),
        Field::SupersededBy(superseded_rid),
    ])
}

/// # Errors
/// Refused when the item is open already, or given no reason.
pub fn reopen(row: &Item, why: Option<&str>) -> Result<Vec<Field>, Refused> {
    if row.state == "open" {
        return Err(Refused(format!("{} is already open.", row.id)));
    }
    if why.is_none_or(str::is_empty) {
        return Err(Refused(
            "reopen needs a reason, it is recorded.".to_string(),
        ));
    }
    Ok(vec![
        Field::State("open".to_string()),
        Field::Resolution(None),
        Field::SupersededBy(None),
    ])
}

/// An item may depend on any number of others, never on itself.
///
/// # Errors
/// Refused when the item is not open, held elsewhere, or would depend on itself.
pub fn depend(row: &Item, ctx: &Ctx, on_rid: i64) -> Result<(), Refused> {
    require_open(row, "wait")?;
    require_hold(row, ctx, "wait")?;
    if on_rid == row.rid {
        return Err(Refused(format!("{} cannot depend on itself.", row.id)));
    }
    Ok(())
}

/// # Errors
/// Refused when the item is not open or not waiting. `wait` is what its dependencies hold it on.
pub fn resume(row: &Item, wait: Option<&Wait>) -> Result<(), Refused> {
    require_open(row, "resume")?;
    if wait.is_none() {
        return Err(Refused(format!("{} is not waiting.", row.id)));
    }
    Ok(())
}

/// # Errors
/// Refused when the item is not open, held elsewhere, or given no note.
pub fn ask(row: &Item, ctx: &Ctx, note: Option<&str>) -> Result<(), Refused> {
    require_open(row, "ask")?;
    require_hold(row, ctx, "ask")?;
    if note.is_none_or(str::is_empty) {
        return Err(Refused(
            "ask needs a note saying what is needed.".to_string(),
        ));
    }
    Ok(())
}

/// # Errors
/// Refused when the item is not open, already the agent's turn, or given no note.
pub fn reply(row: &Item, note: Option<&str>) -> Result<(), Refused> {
    require_open(row, "reply")?;
    if row.ask().is_none() {
        return Err(Refused(format!("{} is already the agent's turn.", row.id)));
    }
    if note.is_none_or(str::is_empty) {
        return Err(Refused(
            "reply needs a note saying what happened.".to_string(),
        ));
    }
    Ok(())
}

/// An earlier decision paragraph stops standing once a new one is written, so a reader going
/// top-down never acts on the overturned choice. Returns the body and how many were relabelled.
///
/// # Panics
/// Never: the pattern is a literal.
#[must_use]
pub fn supersede_decisions(body: &str) -> (String, usize) {
    static LABEL: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let label = LABEL.get_or_init(|| {
        regex::Regex::new(r"(?m)^\*\*Decision, (\d{4}-\d{2}-\d{2})(?:, derived)?\.\*\*")
            .expect("static pattern")
    });
    let mut n = 0;
    let out = label.replace_all(body, |c: &regex::Captures| {
        n += 1;
        format!("**Superseded decision, {}.**", &c[1])
    });
    (out.into_owned(), n)
}

/// A plain answer that repeats the stored decision adds nothing to the body.
#[must_use]
pub fn is_repeat_answer(row: &Item, decision: &str, basis: Option<&str>) -> bool {
    basis.is_none() && row.decision.as_deref() == Some(decision.trim())
}

/// A derived decision names its basis in the decision itself, so every view and the dump carry it.
///
/// # Errors
/// Refused when the item is not open, not a decision, given no decision, a derived answer names no
/// basis, or would replace the owner's own decision.
pub fn answer(
    row: &Item,
    ctx: &Ctx,
    decision: Option<&str>,
    kind: Kind,
    basis: Option<&str>,
) -> Result<Vec<Field>, Refused> {
    require_open(row, "answer")?;
    if kind != Kind::Decision {
        return Err(Refused(format!(
            "{} is {}, not a decision. reply to it, or close it.",
            row.id,
            kind.as_str()
        )));
    }
    let Some(decision) = decision.filter(|d| !d.is_empty()) else {
        return Err(Refused("answer needs the decision, in words.".to_string()));
    };
    let mut decision = decision.to_string();
    if let Some(basis) = basis {
        if basis.trim().is_empty() {
            return Err(Refused(
                "a derived answer needs its basis: the decision, central idea or practice it follows."
                    .to_string(),
            ));
        }
        if let Some(prior) = row.decision.as_deref().filter(|d| !d.starts_with(DERIVED)) {
            return Err(Refused(format!(
                "{} was decided by the owner on {}: {}. A derived answer never replaces that.",
                row.id,
                opt(row.decided_at.as_ref()),
                chars(prior, 100)
            )));
        }
        decision = format!("{DERIVED}{}: {decision}", basis.trim());
    }
    Ok(vec![
        Field::Decision(Some(decision)),
        Field::DecidedAt(Some(ctx.now.clone())),
    ])
}

/// # Errors
/// Refused when the level is not one of the three.
pub fn rate(level: &str) -> Result<Vec<Field>, Refused> {
    if !COMPLEXITIES.contains(&level) {
        return Err(Refused(format!(
            "complexity is one of {}, not {}",
            COMPLEXITIES.join(", "),
            crate::text::quoted(level)
        )));
    }
    Ok(vec![Field::Complexity(Some(level.to_string()))])
}

/// The labels `--set tags=` gives: the list as written, sorted and each once.
#[must_use]
pub fn set_tags(value: &str) -> Vec<String> {
    let mut new: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    new.sort();
    new.dedup();
    new
}

/// An item the loop parked or sent back, given a fresh start: back to the agents when it is the owner's
/// turn, else only recorded.
///
#[cfg(test)]
#[path = "tests/rules.rs"]
mod tests;
