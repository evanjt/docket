//! `docket status`: the flow, the release, the loop, the machines, the claims, the packages, the
//! check and the queue, as text; and `docket check`.

use std::fmt::Write;

use serde_json::Value;

use docket_core::pace::{Pace, duration};

use crate::ctx::Ctx;
use crate::fail::{Fail, Result};
use crate::local;
use crate::py::{Py, cut, or_none};

const FLOW: [&str; 4] = ["ready", "building", "checking", "done"];
const ASIDE: [&str; 4] = ["blocked", "parked", "inbox", "later"];
const WIDTH: usize = 160;
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
    let s = ctx.read("/summary", &[])?;
    let now = now();
    let facts = Facts::of(&s["skills"]);
    let machines = machines(&facts, &s["busy"]);
    for line in head(&slug, &s, &facts, &machines, &total) {
        println!("{}", cut(&line, WIDTH - 1));
    }
    println!("{}", flow_line(&total));
    print_release(&s);
    println!(
        "\nLoop: mode {}, pool {}, last tick never, {} running{}",
        facts.mode,
        facts.pool,
        s["running"].as_i64().unwrap_or(0),
        facts
            .paused
            .as_ref()
            .map_or(String::new(), |p| format!("; paused itself: {p}"))
    );
    for line in stalled(&s["claims"], facts.stale, now) {
        println!("{line}");
    }
    for block in [
        machine_lines(&machines),
        job_lines(&s["jobs"], now),
        package_lines(&s["packages"]),
        vec![cut(
            "CHORES  none has run here yet: docket loop runs them",
            WIDTH - 1,
        )],
    ] {
        println!();
        print_block(&block);
    }
    let problems = problem_lines(&s["problems"]);
    if !problems.is_empty() {
        println!("\nCheck:");
        for p in problems {
            println!("  {p}");
        }
    }
    print_next(ctx)?;
    println!(
        "\nOwner: {} parked on you (docket todo). Claim: docket start ID. Search: docket search <words>.",
        count(&total, "parked")
    );
    Ok(0)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

fn print_block(lines: &[String]) {
    for l in lines.iter().take(SHOWN) {
        println!("{l}");
    }
    if lines.len() > SHOWN {
        println!("  ... {} more", lines.len() - SHOWN);
    }
}

/// The loop facts the status text reads, each its default when unset.
pub struct Facts {
    pub mode: String,
    pub pool: String,
    pub paused: Option<String>,
    pub stale: i64,
}

impl Facts {
    #[must_use]
    pub fn of(skills: &Value) -> Self {
        let get = |k: &str| {
            skills[k]
                .as_str()
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        Self {
            mode: get("mode").unwrap_or_else(|| "pause".into()),
            pool: get("pool").unwrap_or_else(|| "local=2".into()),
            paused: get("paused_by"),
            stale: get("stale_claim")
                .and_then(|v| v.parse().ok())
                .unwrap_or(120),
        }
    }

    /// `{host: slots}` in the order the pool names them; an unreadable pool holds none.
    #[must_use]
    pub fn slots(&self) -> Vec<(String, i64)> {
        self.pool
            .split_whitespace()
            .filter_map(|part| {
                let (host, n) = part.split_once('=')?;
                Some((host.to_string(), n.parse().ok()?))
            })
            .collect()
    }
}

/// One pool host: its slots, the jobs this host runs on it, and its cores, load and memory when read.
pub struct Machine {
    pub host: String,
    pub slots: i64,
    pub running: i64,
    pub stats: Option<(u64, f64, u64, u64)>,
}

fn machines(facts: &Facts, busy: &Value) -> Vec<Machine> {
    facts
        .slots()
        .into_iter()
        .map(|(host, slots)| Machine {
            running: busy[&host].as_i64().unwrap_or(0),
            stats: if host == "local" {
                local::stats()
            } else {
                None
            },
            host,
            slots,
        })
        .collect()
}

fn head(
    slug: &str,
    s: &Value,
    facts: &Facts,
    machines: &[Machine],
    total: &[(String, u64)],
) -> Vec<String> {
    let mut parts = vec![slug.to_string()];
    if let Some(name) = s["release"]["name"].as_str() {
        parts.push(format!("release {name}"));
    }
    parts.push("NO LOOP".into());
    let running: i64 = machines.iter().map(|m| m.running).sum();
    let slots: i64 = facts.slots().iter().map(|(_, n)| n).sum();
    parts.push(format!("jobs {running} of {slots} slots"));
    let closed = s["hour"]["closed"].as_u64().unwrap_or(0);
    if closed > 0 {
        #[allow(clippy::cast_precision_loss)]
        let ratio = s["hour"]["observed"].as_u64().unwrap_or(0) as f64 / closed as f64;
        parts.push(format!("{ratio:.2} observations per close"));
    }
    let pace = Pace {
        closed: s["pace"]["closed"].as_u64().unwrap_or(0),
        opened: 0,
        working: s["pace"]["working"].as_i64().unwrap_or(0),
    };
    if let Some(per_hour) = pace.per_hour() {
        parts.push(format!("closing {per_hour}/h"));
        let todo = count(total, "ready") + count(total, "building") + count(total, "checking");
        if todo > 0 {
            parts.push(format!(
                "clear in {}",
                duration(i64::try_from(todo * 3600 / per_hour).unwrap_or(0))
            ));
        }
    }
    let mut lines = vec![parts.join("   ")];
    lines.push(match &facts.paused {
        Some(p) => format!(
            "  The loop paused itself: {p}. Fix that, then press space to run it again."
        ),
        None => "  Nothing is being worked: no loop runs on this host. L starts one; space sets it to run. \
                 ? explains the screen."
            .into(),
    });
    lines
}

/// `ready 212 > building 18 > checking 6 > done 141    blocked 3  parked 9`.
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

fn print_release(s: &Value) {
    let r = &s["release"];
    if r.is_object() {
        let n = |k: &str| r[k].as_i64().unwrap_or(0);
        println!(
            "Release {}, cut {}: {} open, {} done since the cut, {} closed in the last day",
            r["name"].as_str().unwrap_or_default(),
            r["cut"].as_str().unwrap_or_default(),
            n("open"),
            n("since_cut"),
            n("last_day")
        );
    }
    for t in s["held_themes"].as_array().into_iter().flatten() {
        let name = t[0].as_str().unwrap_or_default();
        println!(
            "  {name}: {} open, held out of the release  (docket next --theme {name})",
            t[1].as_u64().unwrap_or(0)
        );
    }
}

/// The claims older than `stale_claim`, which no loop has looked at.
fn stalled(jobs: &Value, stale: i64, now: i64) -> Vec<String> {
    let rows: Vec<String> = jobs
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|j| {
            let since = j["since"].as_i64()?;
            (now - since >= stale * 60).then(|| {
                format!(
                    "  {:<7} claimed {} ago, past stale_claim {stale}m",
                    j["id"].as_str().unwrap_or_default(),
                    duration(now - since)
                )
            })
        })
        .collect();
    if rows.is_empty() {
        return rows;
    }
    let mut out = vec!["Stalled while the loop was off (the loop has never ticked):".to_string()];
    out.extend(rows);
    out
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

fn machine_lines(rows: &[Machine]) -> Vec<String> {
    if rows.is_empty() {
        return vec!["MACHINES  none in the pool".into()];
    }
    let mut lines = vec![format!("{:<18}{:<13}{:<13}slots", "MACHINES", "cpu", "ram")];
    for m in rows {
        #[allow(clippy::cast_precision_loss)]
        let seen = match m.stats {
            Some((cores, load, free, total)) if cores > 0 => format!(
                "{:<13}{:<13}",
                bar(load, cores as f64),
                bar(total.saturating_sub(free) as f64, total as f64)
            ),
            _ => format!("{:<26}", "not read yet"),
        };
        lines.push(format!(
            " {:<17}{seen}{}/{}",
            cut(&m.host, 16),
            m.running,
            m.slots
        ));
    }
    lines
}

fn job_lines(jobs: &Value, now: i64) -> Vec<String> {
    let rows = jobs.as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return vec!["JOBS  nothing claimed".into()];
    }
    let mut lines = vec![format!(
        "{:<7}{:<8}{:<12}{:<18}{:>6}  title",
        "JOBS", "role", "host", "model", "time"
    )];
    for j in rows {
        let s = |k: &str| j[k].as_str().filter(|v| !v.is_empty());
        let took = j["since"]
            .as_i64()
            .filter(|t| *t != 0)
            .map_or(String::new(), |t| duration(now - t));
        let flag = s("flag").map_or(String::new(), |f| format!("[{f}] "));
        let pkg = s("package").map_or(String::new(), |p| format!("{p} "));
        let line = format!(
            " {:<6}{:<8}{:<12}{:<18}{took:>6}  {flag}{pkg}{}",
            s("id").unwrap_or_default(),
            s("role").unwrap_or("build"),
            cut(s("host").unwrap_or_default(), 11),
            cut(s("model").unwrap_or("by hand"), 17),
            s("title").unwrap_or_default()
        );
        lines.push(cut(&line, WIDTH - 1));
    }
    lines
}

fn package_lines(packages: &Value) -> Vec<String> {
    let rows = packages.as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return vec!["PACKAGES  none started".into()];
    }
    let n = |p: &Value, k: &str| p[k].as_u64().unwrap_or(0);
    let live: Vec<&Value> = rows.iter().filter(|p| n(p, "live") > 0).collect();
    let mut shown = live.clone();
    shown.extend(rows.iter().filter(|p| n(p, "live") == 0).take(5));
    let mut head = format!(
        "PACKAGES  {} started, {} being worked",
        rows.len(),
        live.len()
    );
    if shown.len() > live.len() {
        let _ = write!(head, ", and the {} nearest done", shown.len() - live.len());
    }
    let mut lines = vec![head, format!("{:<20}done  live  title", "")];
    for p in shown {
        #[allow(clippy::cast_precision_loss)]
        let row = format!(
            " {:<7}{}  {:>3}/{:<3}{:>4}  {}",
            p["id"].as_str().unwrap_or_default(),
            bar(n(p, "done") as f64, n(p, "total") as f64),
            n(p, "done"),
            n(p, "total"),
            n(p, "live"),
            p["title"].as_str().unwrap_or_default()
        );
        lines.push(cut(&row, WIDTH - 1));
    }
    lines
}

fn print_next(ctx: &mut Ctx) -> Result<()> {
    let rows = ctx.read("/next", &[("n", Some("8".into()))])?;
    let rows = rows.as_array().cloned().unwrap_or_default();
    println!("\nNext {} for an agent:", rows.len());
    for (i, r) in rows.iter().enumerate() {
        let s = |k: &str| r[k].as_str().filter(|v| !v.is_empty());
        let cx = s("complexity").map_or(String::new(), |c| format!(" [{c}]"));
        let g = s("group").map_or(String::new(), |g| format!("  {{{g}}}"));
        println!(
            "  {:>2}. {:<6} {}{cx}{g}",
            i + 1,
            s("id").unwrap_or_default(),
            cut(s("title").unwrap_or_default(), 80)
        );
    }
    Ok(())
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
        "two_packages" => format!(
            "{} sits in two open packages, {}. Unlink it from one.",
            s("id"),
            s("packages")
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
