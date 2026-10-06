//! `docket status`: the progress, what is claimed now, what waits on the owner, the plans due for
//! audit, the plans under way, the check, the queue and how work starts, as text; and `docket check`.

use serde_json::Value;

use docket_core::check;
use docket_core::flow::GET_GOING;
use docket_core::metrics::{Forecast, Whole};
use docket_core::pace::duration;

use crate::ctx::Ctx;
use crate::fail::{Fail, Result};
use crate::jsonout::{Json, cut, or_none};

const FLOW: [&str; 3] = ["ready", "in progress", "done"];
const ASIDE: [&str; 4] = ["checking", "blocked", "waiting on owner", "parked"];
const WIDTH: usize = 160;
/// Rows each block shows before it says how many more.
const SHOWN: usize = 21;

/// `(word, count)` pairs from the `{word: count}` map the status route answers with.
fn counts(map: &Value) -> Vec<(String, u64)> {
    map.as_object()
        .into_iter()
        .flatten()
        .filter_map(|(w, n)| Some((w.clone(), n.as_u64()?)))
        .collect()
}

fn count(total: &[(String, u64)], word: &str) -> u64 {
    total.iter().find(|(w, _)| w == word).map_or(0, |(_, n)| *n)
}

/// Everything the status text is made from, read once.
pub struct Read {
    pub total: Vec<(String, u64)>,
    pub summary: Value,
    /// The `/metrics` body of the current release, null when the server has none.
    pub metrics: Value,
    /// The whole project's progress, as the server works it out.
    pub whole: Whole,
    pub yours: Value,
    pub next: Value,
    pub now: i64,
    /// The line offering a squash, when one is due.
    pub squash: Option<String>,
}

/// # Errors
/// The project cannot be resolved, or the server refuses.
pub fn status(ctx: &mut Ctx) -> Result<i32> {
    crate::cmd::published::settle_push(ctx);
    let status = ctx.read("/status", &[])?;
    let total = counts(&status["by_word"]);
    let slug = ctx.project()?;
    let read = Read {
        total,
        summary: ctx.read("/summary", &[])?,
        metrics: ctx
            .read("/metrics", &[("scope", Some("release".into()))])
            .unwrap_or(Value::Null),
        whole: ctx
            .read("/metrics", &[("scope", Some("project".into()))])
            .ok()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        yours: ctx.read("/todo", &[])?,
        next: ctx.read("/next", &[("n", Some("8".into()))])?,
        now: now(),
        squash: crate::cmd::published::offer(ctx),
    };
    if ctx.json {
        let mut out = json_status(&read);
        for k in ["project", "host", "total"] {
            out[k] = status[k].clone();
        }
        out["by_key"] = status["by_key"].clone();
        ctx.emit(&Json::from_value(&out));
        return Ok(0);
    }
    for line in render(&slug, &read) {
        println!("{}", cut(&line, WIDTH - 1));
    }
    Ok(0)
}

/// The sections the status text is made from, as JSON: the word counts, the claims, yours, due
/// audits, plans, problem counts by kind, metrics and next.
#[must_use]
pub fn json_status(r: &Read) -> Value {
    let s = &r.summary;
    serde_json::json!({
        "by_word": Value::Object(r.total.iter().map(|(w, n)| (w.clone(), (*n).into())).collect()),
        "claims": s["claims"],
        "yours": r.yours,
        "due": s["due"],
        "plans": s["plans"],
        "plan_count": s["plan_count"],
        "problems": Value::Object(
            check::counts(&s["problems"]).into_iter().map(|(k, n)| (k, n.into())).collect()
        ),
        "metrics": r.metrics,
        "next": r.next,
    })
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
    let mut out = vec![slug.to_string()];
    if let Ok(forecast) = serde_json::from_value::<Forecast>(r.metrics["forecast"].clone()) {
        out.push(forecast.line());
    }
    out.push(flow_line(&r.total));
    let progress = &r.whole.progress;
    #[allow(clippy::cast_precision_loss)]
    out.push(format!(
        "Progress: {}  {} of {} closed",
        bar(progress.done as f64, progress.counted as f64),
        progress.done,
        progress.counted
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
    out.extend(r.squash.iter().cloned());
    out.push(plans_line(&s["plan_count"]));
    out.extend(plan_lines(&s["plans"]));
    let problems = problem_summary(&s["problems"]);
    if !problems.is_empty() {
        out.push("\nCheck:".into());
        out.push(format!("  {problems}: docket check"));
    }
    out.extend(next_lines(&r.next));
    out.push("\nTo get going, a session per role:".into());
    for (skill, what) in GET_GOING {
        out.push(format!("  {skill:<8}{what}"));
    }
    out.push("Claim: docket start ID. Search: docket search <words>.".into());
    out
}

/// `ready 212 > in progress 17 > done 141    blocked 3  parked 9`.
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

/// `Plans: 3 open, 1 audit due`, from the counts the summary route answers with.
fn plans_line(c: &Value) -> String {
    let n = |k: &str| c[k].as_u64().unwrap_or(0);
    format!("Plans: {} open, {} audit due", n("open"), n("audit_due"))
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

/// `95 release inversions, 5 cycles`: one count per kind of problem.
fn problem_summary(problems: &Value) -> String {
    check::counts(problems)
        .into_iter()
        .map(|(kind, n)| check::phrase(&kind, n))
        .collect::<Vec<_>>()
        .join(", ")
}

fn problem_line(p: &Value) -> String {
    let s = |k: &str| or_none(p[k].as_str()).to_string();
    let n = p["n"].as_i64().unwrap_or(0);
    match p["kind"].as_str().unwrap_or_default() {
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
        "abandoned" => format!(
            "{} depends on {}, dropped with no successor: docket dep rm {} {}, or wait on what replaced it",
            s("id"),
            s("on"),
            s("id"),
            s("on")
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
        ctx.emit(&Json::strs(&problems));
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
