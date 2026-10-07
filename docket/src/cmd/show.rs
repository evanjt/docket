//! One item in full, its events and its ties; every project; the graph of a project.

use std::fmt::Write;

use serde_json::Value;

use docket_core::area::Placement;
use docket_core::rows::Row;
use docket_core::touch::declared_files;

use crate::cmd::lists::{print_decided_like, question_keys};
use crate::cmd::published;
use crate::ctx::{Ctx, id};
use crate::fail::Result;
use crate::jsonout::{Json, cut, dumps_line, float_repr, or_none};
use crate::row::{fmt_row, item_json, row_of};

/// `members: 9 of 13 done, 2 live`.
#[must_use]
pub fn progress_line(p: &Value) -> String {
    let live = p["live"].as_u64().unwrap_or(0);
    let mut out = format!(
        "members: {} of {} done",
        p["done"].as_u64().unwrap_or(0),
        p["total"].as_u64().unwrap_or(0)
    );
    if live > 0 {
        let _ = write!(out, ", {live} live");
    }
    out
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect()
}

/// # Errors
/// The project cannot be resolved, or the item is unknown.
pub fn show(ctx: &mut Ctx, item: &str) -> Result<i32> {
    let item = id(item)?;
    let v = ctx.read(&format!("/show/{item}"), &[])?;
    if ctx.json {
        let mut tail = vec!["related", "parent", "origin", "children", "cites"];
        if v.get("progress").is_some() {
            tail.push("progress");
        }
        let mut out = item_json(&v, &[], &[]);
        if let Json::Dict(fields) = &mut out {
            for k in ["related", "parent", "origin", "children"] {
                fields.push((k.into(), Json::from_value(&v[k])));
            }
            let cites = v["cites"].as_array().into_iter().flatten();
            let cites = cites
                .map(|c| Json::pick(c, &["path", "line", "kind"]))
                .collect();
            fields.push(("cites".into(), Json::List(cites)));
            if tail.contains(&"progress") {
                let p = Json::pick(&v["progress"], &["done", "total", "live"]);
                fields.push(("progress".into(), p));
            }
        }
        ctx.emit(&out);
        return Ok(0);
    }
    let about = ctx.read(&format!("/context/{item}"), &[])?;
    let r = row_of(&v);
    println!("### {}. {}\n", r.id, r.title);
    println!("{}", fmt_row(&r, None));
    println!("       {}", facts(&r, &about).join("  "));
    for line in ties(&r, &v, &about) {
        println!("       {line}");
    }
    let events = ctx.read(&format!("/log/{item}"), &[])?;
    if let Some(line) = placed_line(events.as_array().map_or(&[], Vec::as_slice)) {
        println!("       {line}");
    }
    if let Some(line) = published::show_line(ctx, r.resolution.as_deref().unwrap_or_default()) {
        println!("       {line}");
    }
    println!();
    println!(
        "{}",
        if r.body.is_empty() {
            "(no body)"
        } else {
            &r.body
        }
    );
    print_cites(&v);
    if r.state == "open" {
        print_decided_like(ctx, &r, &question_keys())?;
    }
    Ok(0)
}

/// `placed in NAME (derived)`: the newest placement an agent recorded with `decide --area`.
fn placed_line(events: &[Value]) -> Option<String> {
    events
        .iter()
        .rev()
        .filter(|e| e["kind"] == "decided")
        .find_map(|e| Placement::of(&e["data"].to_string()))
        .map(|p| format!("placed in {} (derived)", p.area))
}

fn facts(r: &Row, about: &Value) -> Vec<String> {
    let mut facts = Vec::new();
    if let Some(release) = r.release.as_deref() {
        facts.push(format!("release: {release}"));
    }
    if let Some(area) = r.area.as_deref() {
        facts.push(format!("area: {area}"));
    }
    if let Some(repo) = about["repo"].as_str() {
        facts.push(format!("repo: {repo}"));
    }
    let tier = about["priority"].as_str().unwrap_or("normal");
    if tier != "normal" {
        facts.push(format!("priority: {tier}"));
    }
    if let Some(asked) = r.asked_at.as_deref().filter(|a| !a.is_empty()) {
        facts.push(format!("asked: {asked}"));
    }
    facts.push(format!(
        "opened: {}  updated: {}",
        r.opened_at, r.updated_at
    ));
    facts
}

/// The lines under the facts: members or related, plan, origin, package, members, touches, holds,
/// concepts.
fn ties(r: &Row, v: &Value, about: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let related = strs(&v["related"]);
    if !related.is_empty() {
        out.push(format!("related: {}", related.join(", ")));
    }
    if let Some(parent) = v["parent"].as_str() {
        out.push(format!("plan: {parent}"));
    }
    let origin = strs(&v["origin"]);
    if !origin.is_empty() {
        out.push(format!("origin: {}", origin.join(", ")));
    }
    let pkg = &about["package"];
    if pkg.is_object() {
        out.push(format!(
            "package: {} {}, {}",
            pkg["id"].as_str().unwrap_or_default(),
            cut(pkg["title"].as_str().unwrap_or_default(), 60),
            progress_line(&pkg["progress"])
        ));
    }
    if v.get("progress").is_some() {
        out.push(progress_line(&v["progress"]));
        let members: Vec<String> = about["members"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| {
                format!(
                    "{} {}",
                    m["id"].as_str().unwrap_or_default(),
                    m["word"].as_str().unwrap_or_default()
                )
            })
            .collect();
        if !members.is_empty() {
            out.push(format!("members: {}", members.join(", ")));
        }
    }
    let touches = declared_files(&r.body);
    if !touches.is_empty() {
        let all: Vec<&str> = touches.iter().map(String::as_str).collect();
        out.push(format!("touches, {} files: {}", all.len(), all.join(", ")));
    }
    let holds = strs(&about["holds"]);
    if !holds.is_empty() {
        out.push(format!("holds: {}", holds.join(", ")));
    }
    let labels = strs(&v["labels"]);
    if !labels.is_empty() {
        out.push(format!("labels: {}", labels.join(", ")));
    }
    out
}

fn print_cites(v: &Value) {
    let cites: Vec<String> = v["cites"]
        .as_array()
        .into_iter()
        .flatten()
        .take(20)
        .map(|c| {
            let path = or_none(c["path"].as_str());
            match c["line"].as_i64() {
                Some(l) if l != 0 => format!("{path}:{l}"),
                _ => path.to_string(),
            }
        })
        .collect();
    if !cites.is_empty() {
        println!("\nCites: {}", cites.join(", "));
    }
}

/// A value as its text reads inside a formatted line.
#[must_use]
pub fn shown(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => n
            .as_i64()
            .map_or_else(|| float_repr(n.as_f64().unwrap_or(0.0)), |i| i.to_string()),
        Value::String(s) => s.clone(),
        other => dumps_line(&Json::from_value(other)),
    }
}

/// # Errors
/// The project cannot be resolved, or the item is unknown.
pub fn log(ctx: &mut Ctx, item: &str) -> Result<i32> {
    let item = id(item)?;
    let events = ctx.read(&format!("/log/{item}"), &[])?;
    let list = events.as_array().cloned().unwrap_or_default();
    if ctx.json {
        let keys = [
            "seq", "uid", "project", "rid", "at", "host", "branch", "kind", "note", "data",
        ];
        ctx.emit(&Json::List(
            list.iter().map(|e| Json::pick(e, &keys)).collect(),
        ));
        return Ok(0);
    }
    if list.is_empty() {
        println!("No events.");
        return Ok(0);
    }
    for e in list {
        println!("{}", log_line(&e));
    }
    Ok(0)
}

/// `2026-01-01T00:00:00Z  host       branch  claimed   note  [role=review]`.
#[must_use]
pub fn log_line(event: &Value) -> String {
    let field = |k: &str| event[k].as_str().filter(|x| !x.is_empty());
    let branch = field("branch").map_or(String::new(), |b| format!(" {b}"));
    let note = field("note").map_or(String::new(), |n| format!("  {n}"));
    let data: Vec<String> = event["data"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| match v {
            Value::Array(items) => format!(
                "{k}={}",
                items.iter().map(shown).collect::<Vec<_>>().join(",")
            ),
            other => format!("{k}={}", shown(other)),
        })
        .collect();
    let data = if data.is_empty() {
        String::new()
    } else {
        format!("  [{}]", data.join(" "))
    };
    format!(
        "{}  {:<10}{branch:<24} {:<10}{note}{data}",
        field("at").unwrap_or_default(),
        field("host").unwrap_or_default(),
        field("kind").unwrap_or_default()
    )
}

/// What `docket deps` prints, by tie, in its order.
const TIES: [(&str, &str); 12] = [
    ("waits_on", "Waits on"),
    ("holds", "Holds"),
    ("group", "Group"),
    ("parent", "Plan"),
    ("children", "Children"),
    ("related", "Related"),
    ("origin", "Origin"),
    ("spawned", "Spawned"),
    ("superseded_by", "Superseded by"),
    ("supersedes", "Supersedes"),
    ("same_files", "Cites the same files"),
    ("mentions", "Mentioned in"),
];

/// # Errors
/// The project cannot be resolved, or the item is unknown.
pub fn deps(ctx: &mut Ctx, item: &str) -> Result<i32> {
    let item = id(item)?;
    let v = ctx.read(&format!("/show/{item}"), &[])?;
    let out = ctx.read(&format!("/deps/{item}"), &[])?;
    if ctx.json {
        let fields = TIES
            .iter()
            .filter(|(k, _)| out.get(*k).is_some())
            .map(|(k, _)| {
                let extra: &[&str] = match *k {
                    "same_files" => &["shared"],
                    "mentions" => &["score", "snip"],
                    _ => &[],
                };
                (
                    (*k).to_string(),
                    crate::row::items_json(&out[*k], extra, &[]),
                )
            })
            .collect();
        ctx.emit(&Json::Dict(fields));
        return Ok(0);
    }
    let mut any = false;
    for (k, label) in TIES {
        let rows = out[k].as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            continue;
        }
        any = true;
        println!("\n{label}:");
        for x in rows {
            let extra = if k == "same_files" {
                format!("  ({} shared)", shown(&x["shared"]))
            } else {
                String::new()
            };
            let r = row_of(&x);
            println!("  {:<6} {:<8} {}{extra}", r.id, r.word, cut(&r.title, 70));
        }
    }
    let touches = declared_files(v["body"].as_str().unwrap_or_default());
    if !touches.is_empty() {
        any = true;
        println!("\nTouches, {} files:", touches.len());
        for path in touches {
            println!("  {path}");
        }
    }
    if !any {
        println!(
            "{} stands alone: no waits, links, group or shared files.",
            v["id"].as_str().unwrap_or_default()
        );
    }
    Ok(0)
}

/// # Errors
/// The server refuses.
pub fn projects(ctx: &mut Ctx) -> Result<i32> {
    let counts = ctx.api.get("/counts", &[])?;
    let list = counts.as_array().cloned().unwrap_or_default();
    let with_roots: Vec<(Value, Vec<String>)> = list
        .into_iter()
        .map(|p| {
            let slug = p["slug"].as_str().unwrap_or_default().to_string();
            let roots: Vec<String> = ctx
                .roots
                .roots
                .iter()
                .filter(|r| r.project == slug)
                .map(|r| r.path.clone())
                .collect();
            (p, roots)
        })
        .collect();
    if ctx.json {
        let out = with_roots
            .iter()
            .map(|(p, roots)| {
                Json::Dict(vec![
                    ("slug".into(), Json::from_value(&p["slug"])),
                    ("open".into(), Json::from_value(&p["open"])),
                    ("done".into(), Json::from_value(&p["done"])),
                    ("dropped".into(), Json::from_value(&p["dropped"])),
                    ("roots".into(), Json::strs(roots)),
                    ("last_event".into(), Json::from_value(&p["last_event"])),
                ])
            })
            .collect();
        ctx.emit(&Json::List(out));
        return Ok(0);
    }
    if with_roots.is_empty() {
        println!("No projects yet. Run any docket command inside a git checkout to create one.");
        return Ok(0);
    }
    for (p, roots) in with_roots {
        let n = |k: &str| p[k].as_i64().unwrap_or(0);
        println!(
            "{:<28} {:>5} open {:>6} done {:>4} dropped   last {}",
            p["slug"].as_str().unwrap_or_default(),
            n("open"),
            n("done"),
            n("dropped"),
            p["last_event"].as_str().unwrap_or("-")
        );
        for r in roots {
            println!("    {r}");
        }
    }
    Ok(0)
}

/// # Errors
/// The project cannot be resolved.
pub fn graph(ctx: &mut Ctx, dot: bool, no_files: bool) -> Result<i32> {
    let g = ctx.read("/graph", &[])?;
    let edges: Vec<&Value> = g["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| !(no_files && e["kind"] == "cites"))
        .collect();
    let nodes = g["nodes"].as_array().cloned().unwrap_or_default();
    if dot {
        println!("digraph docket {{");
        for n in &nodes {
            let id = n["id"].as_str().unwrap_or_default();
            println!(
                "  \"{id}\" [label=\"{id}\", group=\"{}\"];",
                n["kind"].as_str().unwrap_or_default()
            );
        }
        for e in &edges {
            let style = match e["kind"].as_str() {
                Some("parent") => "solid",
                Some("related") => "dashed",
                _ => "dotted",
            };
            println!(
                "  \"{}\" -> \"{}\" [style={style}];",
                shown(&e["from"]),
                shown(&e["to"])
            );
        }
        println!("}}");
        return Ok(0);
    }
    let node_keys = [
        "id", "key", "kind", "state", "word", "release", "area", "title",
    ];
    let out = Json::Dict(vec![
        ("project".into(), Json::from_value(&g["project"])),
        (
            "nodes".into(),
            Json::List(nodes.iter().map(|n| Json::pick(n, &node_keys)).collect()),
        ),
        (
            "edges".into(),
            Json::List(
                edges
                    .iter()
                    .map(|e| Json::pick(e, &["from", "to", "kind"]))
                    .collect(),
            ),
        ),
    ]);
    println!("{}", dumps_line(&out));
    Ok(0)
}

#[cfg(test)]
#[path = "../tests/show.rs"]
mod tests;
