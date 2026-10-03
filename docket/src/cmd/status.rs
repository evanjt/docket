//! `docket status`: the progress, what is claimed now, what waits on the owner, the plans due for
//! audit, the plans under way, the check, the queue and how work starts, as text; and `docket check`.

use serde_json::Value;

use docket_core::flow::GET_GOING;
use docket_core::pace::{Pace, duration};

use crate::ctx::Ctx;
use crate::fail::{Fail, Result};
use crate::py::{Py, cut, or_none};

const FLOW: [&str; 3] = ["ready", "building", "done"];
const ASIDE: [&str; 3] = ["checking", "blocked", "parked"];
const WIDTH: usize = 160;
/// Rows each block shows before it says how many more.
const SHOWN: usize = 21;

/// `{word: count}` from the ordered pairs the flow route answers with.
fn counts(pairs: &Value) -> Vec<(String, u64)> {
    pairs
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p[0].as_str()?.to_string(), p[1].as_u64()?)))
        .collect()
}

fn count(total: &[(String, u64)], word: &str) -> u64 {
    total.iter().find(|(w, _)| w == word).map_or(0, |(_, n)| *n)
}

fn as_dict(pairs: &[(String, u64)]) -> Py {
    Py::Dict(
        pairs
            .iter()
            .map(|(w, n)| (w.clone(), Py::Int(i64::try_from(*n).unwrap_or(0))))
            .collect(),
    )
}

/// Everything the status text is made from, read once.
pub struct Read {
    pub total: Vec<(String, u64)>,
    pub summary: Value,
    pub yours: Value,
    pub next: Value,
    pub now: i64,
}

/// # Errors
/// The project cannot be resolved, or the server refuses.
pub fn status(ctx: &mut Ctx) -> Result<i32> {
    let flow = ctx.read("/flow", &[])?;
    let total = counts(&flow["by_word"]);
    if ctx.json {
        let by_key = flow["by_key"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| {
                (
                    p[0].as_str().unwrap_or_default().to_string(),
                    as_dict(&counts(&p[1])),
                )
            })
            .collect();
        ctx.emit(&Py::Dict(vec![
            ("project".into(), Py::from_value(&flow["project"])),
            ("host".into(), Py::from_value(&flow["host"])),
            ("total".into(), Py::from_value(&flow["total"])),
            ("by_word".into(), as_dict(&total)),
            ("by_key".into(), Py::Dict(by_key)),
        ]));
        return Ok(0);
    }
    let slug = ctx.project()?;
    let read = Read {
        total,
        summary: ctx.read("/summary", &[])?,
        yours: ctx.read("/todo", &[])?,
        next: ctx.read("/next", &[("n", Some("8".into()))])?,
        now: now(),
    };
    for line in render(&slug, &read) {
        println!("{}", cut(&line, WIDTH - 1));
    }
    Ok(0)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// The status text, line by line.
#[must_use]
pub fn render(slug: &str, r: &Read) -> Vec<String> {
    let s = &r.summary;
    let mut out = vec![head(slug, s, &r.total), flow_line(&r.total)];
    let closed = count(&r.total, "done");
    let open: u64 = r
        .total
        .iter()
        .filter(|(w, _)| !matches!(w.as_str(), "done" | "dropped" | "standing"))
        .map(|(_, n)| n)
        .sum();
    #[allow(clippy::cast_precision_loss)]
    out.push(format!(
        "Progress: {}  {closed} of {} closed",
        bar(closed as f64, (closed + open) as f64),
        closed + open
    ));
    out.push(String::new());
    out.extend(claim_lines(&s["claims"], r.now));
    out.extend(titled_lines(
        "YOURS",
        "  (docket todo)",
        "nothing waits on you",
        &r.yours,
    ));
    out.extend(titled_lines("AUDITS DUE", "", "no plan is due", &s["due"]));
    out.extend(plan_lines(&s["plans"]));
    let problems = problem_lines(&s["problems"]);
    if !problems.is_empty() {
        out.push("\nCheck:".into());
        out.extend(problems.into_iter().map(|p| format!("  {p}")));
    }
    out.extend(next_lines(&r.next));
    out.push("\nTo get going, a session per role:".into());
    for (skill, what) in GET_GOING {
        out.push(format!("  {skill:<8}{what}"));
    }
    out.push("Claim: docket start ID. Search: docket search <words>.".into());
    out
}

/// The project, with the pace of closes and how long the open work takes at it.
fn head(slug: &str, s: &Value, total: &[(String, u64)]) -> String {
    let mut parts = vec![slug.to_string()];
    let pace = Pace {
        closed: s["pace"]["closed"].as_u64().unwrap_or(0),
        opened: 0,
        working: s["pace"]["working"].as_i64().unwrap_or(0),
    };
    if let Some(per_hour) = pace.per_hour() {
        parts.push(format!("closing {per_hour}/h"));
        let todo = count(total, "ready") + count(total, "building");
        if todo > 0 {
            parts.push(format!(
                "clear in {}",
                duration(i64::try_from(todo * 3600 / per_hour).unwrap_or(0))
            ));
        }
    }
    parts.join("   ")
}

/// `ready 212 > building 18 > done 141    blocked 3  parked 9`.
#[must_use]
pub fn flow_line(total: &[(String, u64)]) -> String {
    let head: Vec<String> = FLOW
        .iter()
        .map(|w| format!("{w} {}", count(total, w)))
        .collect();
    let mut line = head.join(" > ");
    let aside: Vec<String> = ASIDE
        .iter()
        .filter(|w| count(total, w) > 0)
        .map(|w| format!("{w} {}", count(total, w)))
        .collect();
    if !aside.is_empty() {
        line.push_str("    ");
        line.push_str(&aside.join("  "));
    }
    line
}

/// `##########` filled in proportion, ten cells, rounded half to even.
#[must_use]
pub fn bar(part: f64, whole: f64) -> String {
    if whole == 0.0 {
        return ".".repeat(10);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (10.0 * part / whole).round_ties_even().clamp(0.0, 10.0) as usize;
    format!("{}{}", "#".repeat(n), ".".repeat(10 - n))
}

fn rows(list: &Value) -> Vec<Value> {
    list.as_array().cloned().unwrap_or_default()
}

fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or_default()
}

fn shown(lines: &mut Vec<String>, n: usize) {
    if n > SHOWN {
        lines.truncate(SHOWN + 1);
        lines.push(format!("  ... {} more", n - SHOWN));
    }
}

/// CLAIMED NOW: every claim, its branch, the host it was claimed on, how long ago, and a flag when
/// nothing has moved on it lately.
fn claim_lines(claims: &Value, now: i64) -> Vec<String> {
    let rows = rows(claims);
    if rows.is_empty() {
        return vec!["CLAIMED NOW  nothing claimed".into()];
    }
    let mut lines = vec![format!("CLAIMED NOW  {}", rows.len())];
    for c in &rows {
        let since = c["since"]
            .as_i64()
            .filter(|t| *t != 0)
            .map_or(String::new(), |t| format!("  {}", duration(now - t)));
        let flag = c["flag"]
            .as_str()
            .filter(|f| !f.is_empty())
            .map_or(String::new(), |f| format!("  [{f}]"));
        lines.push(format!(
            " {:<6} {} on {}{since}{flag} {}",
            text(c, "id"),
            text(c, "branch"),
            text(c, "host"),
            text(c, "title")
        ));
    }
    shown(&mut lines, rows.len());
    lines
}

/// A heading with its count, then a row per item with its title.
fn titled_lines(name: &str, hint: &str, none: &str, list: &Value) -> Vec<String> {
    let rows = rows(list);
    if rows.is_empty() {
        return vec![format!("{name}  {none}")];
    }
    let mut lines = vec![format!("{name}  {}{hint}", rows.len())];
    for r in &rows {
        lines.push(format!(" {:<6} {}", text(r, "id"), text(r, "title")));
    }
    shown(&mut lines, rows.len());
    lines
}

/// PLANS: those with tickets being worked first, then the nearest done.
fn plan_lines(plans: &Value) -> Vec<String> {
    let rows = rows(plans);
    if rows.is_empty() {
        return vec!["PLANS  none under way".into()];
    }
    let n = |p: &Value, k: &str| p[k].as_u64().unwrap_or(0);
    let mut lines = vec![format!("PLANS  {} under way", rows.len())];
    for p in &rows {
        #[allow(clippy::cast_precision_loss)]
        lines.push(format!(
            " {:<7}{}  {:>3}/{:<3}{:>4}  {}",
            text(p, "id"),
            bar(n(p, "done") as f64, n(p, "total") as f64),
            n(p, "done"),
            n(p, "total"),
            n(p, "live"),
            text(p, "title")
        ));
    }
    shown(&mut lines, rows.len());
    lines
}

fn next_lines(next: &Value) -> Vec<String> {
    let rows = rows(next);
    let mut lines = vec![format!("\nNext {}:", rows.len())];
    for (i, r) in rows.iter().enumerate() {
        let s = |k: &str| r[k].as_str().filter(|v| !v.is_empty());
        let cx = s("complexity").map_or(String::new(), |c| format!(" [{c}]"));
        let g = s("group").map_or(String::new(), |g| format!("  {{{g}}}"));
        lines.push(format!(
            "  {:>2}. {:<6} {}{cx}{g}",
            i + 1,
            s("id").unwrap_or_default(),
            cut(s("title").unwrap_or_default(), 80)
        ));
    }
    lines
}

/// Each problem the check route found, in the words `docket check` prints.
#[must_use]
pub fn problem_lines(problems: &Value) -> Vec<String> {
    problems
        .as_array()
        .into_iter()
        .flatten()
        .map(problem_line)
        .collect()
}

fn problem_line(p: &Value) -> String {
    let s = |k: &str| or_none(p[k].as_str()).to_string();
    let n = p["n"].as_i64().unwrap_or(0);
    match p["kind"].as_str().unwrap_or_default() {
        "conflict" => format!(
            "{} carries a sync conflict in its body: docket edit {} --body FILE",
            s("id"),
            s("id")
        ),
        "undefined_key" => format!(
            "{n} {} filed under {}, which the project does not define",
            if n == 1 { "item is" } else { "items are" },
            s("key")
        ),
        "integrity" => format!("integrity_check: {}", s("result")),
        "foreign_keys" => format!("{n} foreign key violations"),
        "cycle" => format!("{} waits in a cycle", s("id")),
        "held_later" => format!(
            "{} ({}) is held by {}, in the later release {}",
            s("id"),
            s("release"),
            s("by"),
            s("later")
        ),
        "held_gate" => format!(
            "{} is held although everything it opened is closed: docket resume {}",
            s("id"),
            s("id")
        ),
        "open_audit" => format!(
            "{} is open for audit while {n} items it opened are open",
            s("id")
        ),
        "stale_wait" => format!(
            "{} has waited since {} until: {}",
            s("id"),
            s("since"),
            cut(&s("until"), 60)
        ),
        "no_body" => format!("{} is open with no body", s("id")),
        other => format!("{} {other}", s("id")),
    }
}

/// # Errors
/// The project cannot be resolved, or `--deep` asks for the dump.
pub fn check(ctx: &mut Ctx, deep: bool) -> Result<i32> {
    if deep {
        return Err(Fail::refused(
            "check --deep compares every row with its dump file, and the dump is written by the server: \
             read it where the dump is.",
        ));
    }
    let problems = problem_lines(&ctx.read("/check", &[])?);
    if ctx.json {
        ctx.emit(&Py::strs(&problems));
    } else if problems.is_empty() {
        println!("clean");
    } else {
        for p in &problems {
            println!("{p}");
        }
    }
    Ok(i32::from(!problems.is_empty()))
}

#[cfg(test)]
#[path = "../tests/status.rs"]
mod tests;
