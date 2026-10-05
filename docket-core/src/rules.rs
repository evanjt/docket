//! The verbs that move an item. Each takes the row as stored and the caller's context, and returns
//! the columns to set or the refusal the caller reads.

use crate::item::{Ctx, Field, Item, KeySpec, Project, Refused};
use crate::word::{Kind, PRIORITIES};

pub const COMPLEXITIES: [&str; 3] = ["high", "medium", "low"];
pub const DERIVED: &str = "Derived from ";
/// The wait a plan holds while anything it opened is still open.
pub const GATE: &str = "everything it opened is closed";

/// Every column a claim sets, cleared together.
#[must_use]
pub fn unclaimed() -> Vec<Field> {
    vec![
        Field::ClaimBranch(None),
        Field::ClaimHost(None),
        Field::ClaimSince(None),
        Field::ClaimRunner(None),
        Field::ClaimJob(None),
        Field::ClaimOn(None),
    ]
}

fn unwaiting() -> Vec<Field> {
    vec![
        Field::WaitOn(None),
        Field::WaitItem(None),
        Field::WaitRef(None),
        Field::WaitSince(None),
    ]
}

fn chars(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// The spec of one of a project's keys.
///
/// # Errors
/// Refused when the project has no such key.
pub fn key_spec<'a>(project: &'a Project, key: &str) -> Result<&'a KeySpec, Refused> {
    project.keys.iter().find(|s| s.key == key).ok_or_else(|| {
        let keys: Vec<&str> = project.keys.iter().map(|s| s.key.as_str()).collect();
        Refused(format!(
            "{} has no key {key}; its keys are {}",
            project.slug,
            keys.join(", ")
        ))
    })
}

/// # Errors
/// Refused when the project has no such key.
pub fn kind_of(project: &Project, key: &str) -> Result<Kind, Refused> {
    Ok(key_spec(project, key)?.kind)
}

/// The turn a new item under the key opens on.
///
/// # Errors
/// Refused when the project has no such key.
pub fn default_turn(project: &Project, key: &str) -> Result<String, Refused> {
    let spec = key_spec(project, key)?;
    Ok(spec.turn.clone().unwrap_or_else(|| {
        if spec.kind == Kind::Decision {
            "user".to_string()
        } else {
            "agent".to_string()
        }
    }))
}

/// Whether the caller may act on a claimed item: same branch, or force.
#[must_use]
pub fn holds(row: &Item, ctx: &Ctx) -> bool {
    match &row.claim_branch {
        None => true,
        Some(branch) => ctx.force || *branch == ctx.branch,
    }
}

/// # Errors
/// Refused when another branch holds the item and force is not given.
pub fn require_hold(row: &Item, ctx: &Ctx, verb: &str) -> Result<(), Refused> {
    if holds(row, ctx) {
        return Ok(());
    }
    Err(Refused(format!(
        "{} is held by {} on {} since {}. {verb} would take it out from under that agent. Merge or release the branch first, or pass --force.",
        row.id,
        opt(row.claim_branch.as_ref()),
        opt(row.claim_host.as_ref()),
        opt(row.claim_since.as_ref())
    )))
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

/// An optional column as Python prints it.
fn opt(v: Option<&String>) -> &str {
    v.map_or("None", String::as_str)
}

/// # Errors
/// Refused when the item is not open, held elsewhere, waiting, parked or in conflict.
pub fn start(row: &Item, ctx: &Ctx) -> Result<Vec<Field>, Refused> {
    require_open(row, "start")?;
    if let Some(branch) = &row.claim_branch {
        if *branch == ctx.branch && row.claim_host.as_deref() == Some(ctx.host.as_str()) {
            return Err(Refused(format!(
                "{} is already yours, claimed {}.",
                row.id,
                opt(row.claim_since.as_ref())
            )));
        }
        if !ctx.force {
            return Err(Refused(format!(
                "{} is held by {branch} on {} since {}. Pick another, or --force if that claim is abandoned.",
                row.id,
                opt(row.claim_host.as_ref()),
                opt(row.claim_since.as_ref())
            )));
        }
    }
    if let Some(on) = &row.wait_on {
        let what = if on == "condition" {
            opt(row.wait_ref.as_ref()).to_string()
        } else {
            format!("item {}", opt(row.wait_ref.as_ref()))
        };
        return Err(Refused(format!(
            "{} is waiting on {what} since {}. resume it first.",
            row.id,
            opt(row.wait_since.as_ref())
        )));
    }
    if row.turn.as_deref() == Some("user") {
        let note = row
            .turn_note
            .clone()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("asked {}", opt(row.asked_at.as_ref())));
        return Err(Refused(format!(
            "{} is the owner's turn: {note}. reply to it first if you are taking it back.",
            row.id
        )));
    }
    if row.conflict != 0 {
        return Err(Refused(format!(
            "{} carries a sync conflict in its body. resolve it first.",
            row.id
        )));
    }
    Ok(vec![
        Field::ClaimBranch(Some(ctx.branch.clone())),
        Field::ClaimHost(Some(ctx.host.clone())),
        Field::ClaimSince(Some(ctx.now.clone())),
    ])
}

/// # Errors
/// Refused when the item is not open, not claimed, or held elsewhere.
pub fn release(row: &Item, ctx: &Ctx) -> Result<Vec<Field>, Refused> {
    require_open(row, "release")?;
    if row.claim_branch.is_none() {
        return Err(Refused(format!("{} is not claimed.", row.id)));
    }
    require_hold(row, ctx, "release")?;
    Ok(unclaimed())
}

/// # Errors
/// Refused when the item is not open, held elsewhere, an undecided question, or given no resolution.
pub fn close(
    row: &Item,
    ctx: &Ctx,
    resolution: Option<&str>,
    kind: Kind,
) -> Result<Vec<Field>, Refused> {
    require_open(row, "close")?;
    require_hold(row, ctx, "close")?;
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
    let mut out = vec![
        Field::State("done".to_string()),
        Field::Turn(None),
        Field::TurnNote(None),
        Field::Resolution(Some(resolution.to_string())),
    ];
    out.extend(unclaimed());
    out.extend(unwaiting());
    Ok(out)
}

/// Whether a plan's audit is due or under way: its last gate event, `(kind, note)` oldest first, is the
/// resume that came when everything it opened closed.
#[must_use]
pub fn came_due<'a>(gate_events: impl IntoIterator<Item = (&'a str, &'a str)>) -> bool {
    gate_events
        .into_iter()
        .filter(|(kind, note)| {
            (*kind == "resumed" && *note == GATE)
                || (*kind == "waited" && note.starts_with(&format!("until: {GATE}")))
        })
        .last()
        .is_some_and(|(kind, _)| kind == "resumed")
}

/// A plan closes once its audit came due, the gaps that audit filed under it still open: one round, and
/// the gaps are worked as tickets. Before that, anything it opened that is open holds it.
///
/// # Errors
/// Refused when the plan never came due and opened work that is still open.
pub fn close_plan(row: &Item, open_under: &[String], due: bool) -> Result<(), Refused> {
    if due || open_under.is_empty() {
        return Ok(());
    }
    let shown: Vec<&str> = open_under.iter().take(10).map(String::as_str).collect();
    let more = if open_under.len() > 10 { " ..." } else { "" };
    Err(Refused(format!(
        "{} opened work that is still open: {}{more}. It closes only when nothing it opened is open; release it and it comes back when they close.",
        row.id,
        shown.join(", ")
    )))
}

/// The tags with one tier in place of any other; normal carries no tag.
///
/// # Errors
/// Refused when the tier is not one of the four.
pub fn prioritise(tags: &[String], tier: &str) -> Result<Vec<Field>, Refused> {
    if !PRIORITIES.contains(&tier) {
        return Err(Refused(format!(
            "priority is one of {}, not {}",
            PRIORITIES.join(", "),
            crate::text::py_repr(tier)
        )));
    }
    let mut out: Vec<String> = tags
        .iter()
        .filter(|t| !PRIORITIES.contains(&t.as_str()))
        .cloned()
        .collect();
    if tier != "normal" {
        out.push(tier.to_string());
    }
    out.sort();
    out.dedup();
    Ok(vec![Field::Tags(out)])
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
    let mut out = vec![
        Field::State("dropped".to_string()),
        Field::Turn(None),
        Field::TurnNote(None),
        Field::Resolution(Some(why.to_string())),
        Field::SupersededBy(superseded_rid),
    ];
    out.extend(unclaimed());
    out.extend(unwaiting());
    Ok(out)
}

/// # Errors
/// Refused when the item is open already, or given no reason.
pub fn reopen(row: &Item, why: Option<&str>, turn: &str) -> Result<Vec<Field>, Refused> {
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
        Field::Turn(Some(turn.to_string())),
        Field::Resolution(None),
        Field::SupersededBy(None),
    ])
}

/// # Errors
/// Refused when the item is not open, held elsewhere, waiting already, or would wait on itself.
pub fn wait(
    row: &Item,
    ctx: &Ctx,
    on: &str,
    target_rid: Option<i64>,
    reference: &str,
) -> Result<Vec<Field>, Refused> {
    require_open(row, "wait")?;
    require_hold(row, ctx, "wait")?;
    if row.wait_on.is_some() {
        return Err(Refused(format!(
            "{} already waits on {} since {}. resume it first.",
            row.id,
            opt(row.wait_ref.as_ref()),
            opt(row.wait_since.as_ref())
        )));
    }
    if on == "item" && target_rid == Some(row.rid) {
        return Err(Refused(format!("{} cannot wait on itself.", row.id)));
    }
    let mut out = vec![
        Field::WaitOn(Some(on.to_string())),
        Field::WaitItem(target_rid),
        Field::WaitRef(Some(reference.to_string())),
        Field::WaitSince(Some(ctx.now.clone())),
    ];
    out.extend(unclaimed());
    Ok(out)
}

/// # Errors
/// Refused when the item is not open or not waiting.
pub fn resume(row: &Item) -> Result<Vec<Field>, Refused> {
    require_open(row, "resume")?;
    if row.wait_on.is_none() {
        return Err(Refused(format!("{} is not waiting.", row.id)));
    }
    let mut out = unwaiting();
    out.push(Field::Turn(Some("agent".to_string())));
    Ok(out)
}

/// # Errors
/// Refused when the item is not open, held elsewhere, or given no note.
pub fn ask(row: &Item, ctx: &Ctx, note: Option<&str>) -> Result<Vec<Field>, Refused> {
    require_open(row, "ask")?;
    require_hold(row, ctx, "ask")?;
    let Some(note) = note.filter(|n| !n.is_empty()) else {
        return Err(Refused(
            "ask needs a note saying what is needed.".to_string(),
        ));
    };
    let mut out = vec![
        Field::Turn(Some("user".to_string())),
        Field::TurnNote(Some(note.to_string())),
        Field::AskedAt(Some(ctx.now.clone())),
    ];
    out.extend(unclaimed());
    Ok(out)
}

/// # Errors
/// Refused when the item is not open, already the agent's turn, or given no note.
pub fn reply(row: &Item, note: Option<&str>) -> Result<Vec<Field>, Refused> {
    require_open(row, "reply")?;
    if row.turn.as_deref() != Some("user") {
        return Err(Refused(format!("{} is already the agent's turn.", row.id)));
    }
    let Some(note) = note.filter(|n| !n.is_empty()) else {
        return Err(Refused(
            "reply needs a note saying what happened.".to_string(),
        ));
    };
    Ok(vec![
        Field::Turn(Some("agent".to_string())),
        Field::TurnNote(Some(note.to_string())),
    ])
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
    let mut out = vec![
        Field::Turn(Some("agent".to_string())),
        Field::TurnNote(Some(decision.clone())),
        Field::Decision(Some(decision)),
        Field::DecidedAt(Some(ctx.now.clone())),
    ];
    out.extend(unclaimed());
    Ok(out)
}

/// # Errors
/// Refused when the level is not one of the three.
pub fn rate(level: &str) -> Result<Vec<Field>, Refused> {
    if !COMPLEXITIES.contains(&level) {
        return Err(Refused(format!(
            "complexity is one of {}, not {}",
            COMPLEXITIES.join(", "),
            crate::text::py_repr(level)
        )));
    }
    Ok(vec![Field::Complexity(Some(level.to_string()))])
}

/// The tags `--set tags=` gives: the list as written, keeping the priority unless it names one.
#[must_use]
pub fn set_tags(tags: &[String], value: &str) -> Vec<String> {
    let mut new: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    if !new.iter().any(|t| PRIORITIES.contains(&t.as_str())) {
        new.extend(
            tags.iter()
                .filter(|t| PRIORITIES.contains(&t.as_str()))
                .cloned(),
        );
    }
    new.sort();
    new.dedup();
    new
}

/// An item the loop parked or sent back, given a fresh start: back to the agents when it is the owner's
/// turn, else only recorded.
///
/// # Errors
/// Refused when the item is not open, is held, or is the owner's turn with no note.
pub fn retry(row: &Item, note: Option<&str>) -> Result<Vec<Field>, Refused> {
    require_open(row, "retry")?;
    if let Some(branch) = &row.claim_branch {
        return Err(Refused(format!(
            "{} is held by {branch}: docket kill {} first if its job is stuck.",
            row.id, row.id
        )));
    }
    if row.turn.as_deref() == Some("user") {
        return reply(row, note);
    }
    Ok(Vec::new())
}

#[cfg(test)]
#[path = "tests/rules.rs"]
mod tests;
