//! The lists: the queue, the owner's turn, claims, waits, questions, the digest, search and files.

use std::collections::BTreeMap;

use serde_json::Value;

use docket_core::rows::Row;

use crate::args::{OwnerQueue, Queue, Recent};
use crate::ctx::{Ctx, id};
use crate::fail::Result;
use crate::py::{Py, cut, or_none};

use crate::row::{fmt_row, items_json, row_of, rows_of};

/// Rows as `--json`, or a line each, or the empty text.
pub fn print_rows(ctx: &Ctx, rows: &Value, empty: &str) {
    if ctx.json {
        ctx.emit(&items_json(rows, &[], &[]));
        return;
    }
    let rows = rows_of(rows);
    if rows.is_empty() {
        println!("{empty}");
        return;
    }
    for r in rows {
        println!("{}", fmt_row(&r, None));
    }
}

fn opt_id(text: Option<&String>) -> Result<Option<String>> {
    text.map(|t| id(t)).transpose()
}

/// # Errors
/// The project cannot be resolved, or the server refuses.
pub fn next(ctx: &mut Ctx, q: &Queue) -> Result<i32> {
    let rows = ctx.read(
        "/next",
        &[
            ("n", Some(q.n.unwrap_or(10).to_string())),
            ("complexity", q.complexity.clone()),
            ("key", q.key.clone()),
            ("theme", q.theme.clone()),
            ("under", opt_id(q.under.as_ref())?),
            ("role", (!q.role.is_empty()).then(|| q.role.join(","))),
            ("priority", q.priority.clone()),
            (
                "current_release",
                q.current_release.then(|| "true".to_string()),
            ),
        ],
    )?;
    print_rows(ctx, &rows, "Nothing for an agent right now.");
    Ok(0)
}

/// # Errors
/// As `next`.
pub fn todo(ctx: &mut Ctx, q: &OwnerQueue) -> Result<i32> {
    let pairs = [
        ("n", q.n.map(|n| n.to_string())),
        ("key", q.key.clone()),
        ("theme", q.theme.clone()),
        ("priority", q.priority.clone()),
    ];
    let rows = ctx.read("/todo", &pairs)?;
    if ctx.json {
        ctx.emit(&items_json(&rows, &[], &["owner_group"]));
        return Ok(0);
    }
    let waiting = ctx.read("/todo/waiting", &pairs)?["waiting"]
        .as_u64()
        .unwrap_or(0);
    let list = rows.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        println!("Nothing waiting on you.");
    }
    let mut group = "";
    for v in &list {
        let now = v["owner_group"].as_str().unwrap_or_default();
        if now != group {
            println!("{}:", group_title(now));
            group = now;
        }
        println!("{}", fmt_row(&row_of(v), None));
    }
    if waiting > 0 {
        println!("\n{waiting} more wait on something first.");
    }
    Ok(0)
}

fn group_title(group: &str) -> &str {
    match group {
        "derived" => "Derived by agents, to confirm or overturn with answer",
        "question" => "Questions",
        "hold" => "A device or thing in hand",
        "access" => "An account or store",
        "act" => "An action from your machine",
        "judge" => "A judgement",
        _ => "Not said what is needed",
    }
}

/// # Errors
/// As `next`.
pub fn wip(ctx: &mut Ctx, host: Option<&String>) -> Result<i32> {
    let pairs = [("host", host.cloned())];
    let rows = ctx.read("/wip", &pairs)?;
    if ctx.json {
        print_rows(ctx, &rows, "Nothing claimed.");
        return Ok(0);
    }
    let rows = rows_of(&rows);
    if rows.is_empty() {
        println!("Nothing claimed.");
        return Ok(0);
    }
    let shares = ctx.read("/shares", &pairs)?;
    for r in rows {
        let about = shares
            .as_array()
            .and_then(|a| a.iter().find(|s| s["id"] == r.id.as_str()))
            .cloned()
            .unwrap_or_default();
        println!("{}", fmt_row(&r, about["flag"].as_str()));
        for s in about["shares"].as_array().into_iter().flatten() {
            println!("       {}", shares_text(s));
        }
    }
    Ok(0)
}

/// `shares a.rs, b.rs with B3, held by x on y: the later landing rebases onto the earlier`.
#[must_use]
pub fn shares_text(s: &Value) -> String {
    let paths: Vec<&str> = s["paths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let shown = paths.iter().take(5).copied().collect::<Vec<_>>().join(", ");
    let rest = if paths.len() > 5 {
        format!(" and {} more", paths.len() - 5)
    } else {
        String::new()
    };
    format!(
        "shares {shown}{rest} with {}, held by {} on {}: the later landing rebases onto the earlier",
        s["holder"].as_str().unwrap_or_default(),
        or_none(s["branch"].as_str()),
        or_none(s["host"].as_str())
    )
}

/// # Errors
/// As `next`.
pub fn waiting(ctx: &mut Ctx, on: Option<&String>) -> Result<i32> {
    let rows = ctx.read("/waiting", &[("on", on.cloned())])?;
    if ctx.json {
        print_rows(ctx, &rows, "Nothing.");
        return Ok(0);
    }
    let targets: Vec<Value> = rows
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| r["wait_target"].clone())
        .collect();
    let rows = rows_of(&rows);
    if rows.is_empty() {
        println!("Nothing waiting.");
        return Ok(0);
    }
    let mut last: Option<String> = None;
    for (r, t) in rows.iter().zip(&targets) {
        let wref = or_none(r.wait_ref.as_deref()).to_string();
        let item = r.wait_on.as_deref() == Some("item");
        let head = if item {
            wref.clone()
        } else {
            format!("until: {wref}")
        };
        if last.as_ref() != Some(&head) {
            if item {
                println!(
                    "\n{head}  {}  {}",
                    t["word"].as_str().unwrap_or_default(),
                    cut(t["title"].as_str().unwrap_or_default(), 70)
                );
            } else {
                println!("\n{head}");
            }
            last = Some(head);
        }
        println!(
            "  {:<6} {}  (since {})",
            r.id,
            cut(&r.title, 80),
            or_none(r.wait_since.as_deref())
        );
    }
    Ok(0)
}

/// The keys of a project row that hold one kind, in the project's order.
#[must_use]
pub fn keys_of(project: &Value, kind: &str) -> Vec<String> {
    project["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|k| k["kind"] == kind)
        .filter_map(|k| k["key"].as_str().map(str::to_string))
        .collect()
}

/// Decided questions, open or done, closest to an undecided question: at most n.
///
/// # Errors
/// The server refuses.
pub fn decided_like(ctx: &mut Ctx, r: &Row, qkeys: &[String], n: usize) -> Result<Vec<Row>> {
    if !qkeys.contains(&r.key) || r.decision.as_deref().is_some_and(|d| !d.is_empty()) {
        return Ok(Vec::new());
    }
    let near = ctx.read(
        &format!("/similar/{}", r.id),
        &[("n", Some((n * 5).to_string()))],
    )?;
    Ok(rows_of(&near)
        .into_iter()
        .filter(|x| qkeys.contains(&x.key) && x.decision.as_deref().is_some_and(|d| !d.is_empty()))
        .take(n)
        .collect())
}

/// The prior decisions close to an open question, and how to answer it from one.
///
/// # Errors
/// The server refuses.
pub fn print_decided_like(ctx: &mut Ctx, r: &Row, qkeys: &[String]) -> Result<()> {
    let twins = decided_like(ctx, r, qkeys, 3)?;
    if twins.is_empty() {
        return Ok(());
    }
    println!(
        "\nDecided questions close to this one. When one settles it, answer it:\n  docket answer {} \"the choice\" --derived \"Q<n>, what it decided\"",
        r.id
    );
    for t in twins {
        println!(
            "  {:<6} {}\n         decided: {}",
            t.id,
            cut(&t.title, 70),
            cut(t.decision.as_deref().unwrap_or_default(), 110)
        );
    }
    Ok(())
}

/// # Errors
/// As `next`.
pub fn questions(ctx: &mut Ctx, theme: Option<&String>) -> Result<i32> {
    let slug = ctx.project()?;
    let project = ctx.project_row(&slug)?;
    let qkeys = keys_of(&project, "decision");
    if qkeys.is_empty() {
        println!("{slug} has no decision key.");
        return Ok(0);
    }
    let rows = ctx.read("/questions", &[("theme", theme.cloned())])?;
    if ctx.json {
        print_rows(ctx, &rows, "Nothing.");
        return Ok(0);
    }
    let twins: Vec<Option<Row>> = rows
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| serde_json::from_value(r["close_to"].clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let rows = rows_of(&rows);
    if rows.is_empty() {
        println!("No open questions.");
        return Ok(0);
    }
    let mut waiters: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for w in rows_of(&ctx.read("/waiting", &[("on", Some("item".into()))])?) {
        waiters
            .entry(w.wait_ref.clone().unwrap_or_default())
            .or_default()
            .push(w.id);
    }
    println!(
        "{} open questions, by theme. These are decisions, not work:",
        rows.len()
    );
    let mut last: Option<Option<String>> = None;
    for (i, r) in rows.iter().enumerate() {
        if last.as_ref() != Some(&r.theme) {
            last = Some(r.theme.clone());
            println!(
                "\n  -- {}",
                r.theme
                    .as_deref()
                    .filter(|t| !t.is_empty())
                    .unwrap_or("no theme")
            );
            if let Some(note) = theme_note(&project, r.theme.as_deref()) {
                println!("     {note}");
            }
        }
        print_question(i + 1, r, twins.get(i).and_then(Option::as_ref), &waiters);
    }
    println!(
        "\nAnswer one with: docket answer Q7 \"the decision\", or --derived \"basis\" when a prior decision settles it."
    );
    Ok(0)
}

fn print_question(i: usize, r: &Row, twin: Option<&Row>, waiters: &BTreeMap<String, Vec<String>>) {
    let flag = if r.turn.as_deref() == Some("agent") {
        format!(
            "   <- agent's turn: {}",
            cut(
                r.turn_note
                    .as_deref()
                    .filter(|n| !n.is_empty())
                    .unwrap_or("being prepared"),
                60
            )
        )
    } else if r.wait_on.as_deref().is_some_and(|w| !w.is_empty()) {
        format!("   <- waits on {}", or_none(r.wait_ref.as_deref()))
    } else {
        String::new()
    };
    println!("  {i:>2}. {:<5} {}{flag}", r.id, cut(&r.title, 75));
    if let Some(held) = waiters.get(&r.id) {
        let mut held = held.clone();
        held.sort();
        println!("          holds {}", held.join(", "));
    }
    if let Some(twin) = twin {
        println!(
            "          close to decided {}: {}",
            twin.id,
            cut(twin.decision.as_deref().unwrap_or_default(), 70)
        );
    }
}

fn theme_note(project: &Value, theme: Option<&str>) -> Option<String> {
    let theme = theme?;
    project["themes"]
        .as_array()?
        .iter()
        .rev()
        .find(|t| t["name"] == theme)?["note"]
        .as_str()
        .filter(|n| !n.is_empty())
        .map(str::to_string)
}

/// # Errors
/// As `next`.
pub fn research(ctx: &mut Ctx) -> Result<i32> {
    let rows = ctx.read("/research", &[])?;
    print_rows(ctx, &rows, "No decided question owes work items.");
    Ok(0)
}

/// # Errors
/// As `next`.
pub fn derived(ctx: &mut Ctx, n: Option<i64>) -> Result<i32> {
    let rows = ctx.read("/derived", &[("n", Some(n.unwrap_or(40).to_string()))])?;
    let list = rows.as_array().cloned().unwrap_or_default();
    if ctx.json {
        let keys = ["id", "state", "at", "title", "chose", "basis"];
        ctx.emit(&Py::List(list.iter().map(|d| Py::pick(d, &keys)).collect()));
        return Ok(0);
    }
    if list.is_empty() {
        println!("No derived decisions.");
        return Ok(0);
    }
    println!(
        "{} derived decisions, newest first. Overturn a question's with docket answer, a ticket's by reopening it:",
        list.len()
    );
    for d in list {
        let s = |k: &str| d[k].as_str().unwrap_or_default().to_string();
        println!(
            "\n  {:<5} {:<7} {}  {}",
            s("id"),
            s("state"),
            cut(&s("at"), 10),
            cut(&s("title"), 70)
        );
        println!("        chose: {}", cut(&s("chose"), 110));
        println!("        basis: {}", cut(&s("basis"), 110));
    }
    Ok(0)
}

/// `done` and `dropped`.
///
/// # Errors
/// As `next`.
pub fn recent(ctx: &mut Ctx, r: &Recent, state: &str) -> Result<i32> {
    let rows = ctx.read(
        &format!("/{state}"),
        &[
            ("n", Some(r.n.unwrap_or(20).to_string())),
            ("key", r.key.clone().filter(|k| !k.is_empty())),
        ],
    )?;
    print_rows(ctx, &rows, &format!("Nothing {state}."));
    Ok(0)
}

/// # Errors
/// As `next`.
pub fn groups(ctx: &mut Ctx, name: Option<&String>) -> Result<i32> {
    let rows = ctx.read(
        "/groups",
        &[("name", name.cloned().filter(|n| !n.is_empty()))],
    )?;
    if ctx.json {
        print_rows(ctx, &rows, "Nothing.");
        return Ok(0);
    }
    let rows = rows_of(&rows);
    if rows.is_empty() {
        println!("No groups.");
        return Ok(0);
    }
    let mut last: Option<Option<String>> = None;
    for r in rows {
        if last.as_ref() != Some(&r.group) {
            println!("\n{}", or_none(r.group.as_deref()));
            last = Some(r.group.clone());
        }
        println!("  {:<6} {:<8} {}", r.id, r.word, cut(&r.title, 75));
    }
    Ok(0)
}

/// `B14    ready    Title  (resolution)` for a match, its resolution shown once it is closed.
fn match_line(r: &Row, width: usize) -> String {
    let res = match r.resolution.as_deref() {
        Some(res) if r.state != "open" && !res.is_empty() => format!("  ({})", cut(res, 40)),
        _ => String::new(),
    };
    format!("{:<6} {:<8} {}{res}", r.id, r.word, cut(&r.title, width))
}

/// # Errors
/// As `next`, or the query does not parse.
#[allow(clippy::too_many_arguments)]
pub fn search(
    ctx: &mut Ctx,
    words: &[String],
    key: Option<&String>,
    state: &str,
    n: i64,
    raw: bool,
    theme: Option<&String>,
    without_theme: Option<&String>,
) -> Result<i32> {
    let rows = ctx.read(
        "/search",
        &[
            ("q", Some(words.join(" "))),
            ("key", key.cloned().filter(|k| !k.is_empty())),
            ("state", Some(state.to_string())),
            ("n", Some(n.to_string())),
            ("raw", raw.then(|| "true".to_string())),
            ("theme", theme.cloned().filter(|t| !t.is_empty())),
            (
                "without_theme",
                without_theme.cloned().filter(|t| !t.is_empty()),
            ),
        ],
    )?;
    if ctx.json {
        ctx.emit(&items_json(&rows, &["score", "snip"], &["snippet"]));
        return Ok(0);
    }
    let list = rows.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        println!("No match.");
        return Ok(0);
    }
    for v in list {
        println!("{}", match_line(&row_of(&v), 70));
        let snip = v["snip"]
            .as_str()
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !snip.is_empty() {
            println!("       {}", cut(&snip, 160));
        }
    }
    Ok(0)
}

/// # Errors
/// As `next`, or the item is unknown.
pub fn similar(ctx: &mut Ctx, item: &str, n: Option<i64>, state: &str) -> Result<i32> {
    let item = id(item)?;
    let rows = ctx.read(
        &format!("/similar/{item}"),
        &[
            ("n", Some(n.unwrap_or(10).to_string())),
            ("state", Some(state.to_string())),
        ],
    )?;
    if ctx.json {
        ctx.emit(&items_json(&rows, &["score", "snip"], &[]));
        return Ok(0);
    }
    let rows = rows_of(&rows);
    if rows.is_empty() {
        println!("Nothing similar.");
        return Ok(0);
    }
    for r in rows {
        println!("{}", match_line(&r, 80));
    }
    Ok(0)
}

/// # Errors
/// As `next`.
pub fn files(ctx: &mut Ctx, prefix: &str, state: &str) -> Result<i32> {
    let rows = ctx.read(
        "/files",
        &[
            ("prefix", Some(prefix.to_string())),
            ("state", Some(state.to_string())),
        ],
    )?;
    if ctx.json {
        ctx.emit(&items_json(&rows, &["path", "line"], &[]));
        return Ok(0);
    }
    let list = rows.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        println!("No item cites that.");
        return Ok(0);
    }
    let mut shown: Option<String> = None;
    for v in list {
        let path = v["path"].as_str().unwrap_or_default().to_string();
        if shown.as_ref() != Some(&path) {
            println!("\n{path}");
            shown = Some(path);
        }
        let ln = match v["line"].as_i64() {
            Some(l) if l != 0 => format!(":{l}"),
            _ => String::new(),
        };
        let r = row_of(&v);
        println!("  {:<6} {:<8} {}{ln}", r.id, r.word, cut(&r.title, 70));
    }
    Ok(0)
}
