//! `docket audit` and `docket stale`: what the server holds, read against this machine's checkouts.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::cmd::show::progress_line;
use crate::ctx::{Ctx, id};
use crate::fail::{Fail, Result};
use crate::local::{self, expand};
use crate::py::{Py, cut, or_none};

const SECTIONS: [&str; 10] = [
    "inbox", "ready", "building", "checking", "blocked", "parked", "later", "standing", "done",
    "dropped",
];

static SHA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b([0-9a-f]{7,40})\b").unwrap());

/// The shas a resolution names.
#[must_use]
pub fn shas(resolution: &str) -> Vec<String> {
    SHA.captures_iter(resolution)
        .map(|m| m[1].to_string())
        .collect()
}

/// `(off HEAD, nowhere)`: done work whose sha is in some ref but not HEAD, and done work naming no
/// commit any of the project's repositories here has.
#[must_use]
pub fn unreachable(
    done: &[Value],
    head: &HashSet<String>,
    known: &HashSet<String>,
) -> (Vec<String>, Vec<String>) {
    let found = |shas: &[String], pool: &HashSet<String>| {
        shas.iter()
            .any(|s| pool.iter().any(|full| full.starts_with(s.as_str())))
    };
    let (mut off_head, mut nowhere) = (Vec::new(), Vec::new());
    for r in done {
        if r["state"] != "done" || r["kind"] != "work" {
            continue;
        }
        let shas = shas(r["resolution"].as_str().unwrap_or_default());
        if !shas.is_empty() && found(&shas, head) {
            continue;
        }
        let id = r["id"].as_str().unwrap_or_default().to_string();
        if !shas.is_empty() && found(&shas, known) {
            off_head.push(id);
        } else {
            nowhere.push(id);
        }
    }
    (off_head, nowhere)
}

/// Every commit on HEAD, and every commit in any ref, across the directories.
fn commits(dirs: &[PathBuf]) -> (HashSet<String>, HashSet<String>) {
    let (mut head, mut known) = (HashSet::new(), HashSet::new());
    for d in dirs {
        let list = |spec: &str| local::git(&["rev-list", spec], d).unwrap_or_default();
        head.extend(list("HEAD").split_whitespace().map(str::to_string));
        known.extend(list("--all").split_whitespace().map(str::to_string));
    }
    (head, known)
}

/// The project's repositories on this machine: its roots, shortest first, with its repos under each.
fn repo_dirs(ctx: &Ctx, slug: &str, project: &Value) -> Vec<PathBuf> {
    let repos: Vec<String> = project["repos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r.as_str().map(str::to_string))
        .collect();
    local::repo_dirs(&ctx.roots.of(slug), &repos)
}

pub struct Target<'a> {
    pub id: Option<&'a String>,
    pub group: Option<&'a String>,
    pub theme: Option<&'a String>,
    pub orphans: bool,
}

/// # Errors
/// No id, group, theme or orphans is named, the project cannot be resolved, or the id is unknown.
pub fn audit(ctx: &mut Ctx, t: &Target) -> Result<i32> {
    let slug = ctx.project()?;
    let given = |v: Option<&String>| v.filter(|s| !s.is_empty()).cloned();
    let item = match given(t.id) {
        Some(x) if !t.orphans && given(t.group).is_none() && given(t.theme).is_none() => {
            Some(id(&x)?)
        }
        _ => None,
    };
    let a = ctx.read(
        "/audit",
        &[
            ("orphans", t.orphans.then(|| "true".to_string())),
            ("group", given(t.group)),
            ("theme", given(t.theme)),
            ("id", item),
        ],
    )?;
    let project = ctx.project_row(&slug)?;
    let rows = a["rows"].as_array().cloned().unwrap_or_default();
    let mut sections: Vec<(&str, Vec<&Value>)> =
        SECTIONS.iter().map(|k| (*k, Vec::new())).collect();
    for r in &rows {
        if let Some((_, list)) = sections.iter_mut().find(|(k, _)| r["word"] == *k) {
            list.push(r);
        }
    }
    let done: Vec<Value> = sections[8].1.iter().map(|r| (*r).clone()).collect();
    let (head, known) = commits(&repo_dirs(ctx, &slug, &project));
    let (off_head, nowhere) = unreachable(&done, &head, &known);
    let breakdown = breakdown_lines(&a["breakdown"]);
    let report = Report {
        a: &a,
        rows: &rows,
        sections: &sections,
        off_head: &off_head,
        nowhere: &nowhere,
        breakdown: &breakdown,
    };
    if ctx.json {
        ctx.emit(&report.json());
        return Ok(0);
    }
    report.print(&heading(&a, t));
    Ok(0)
}

fn heading(a: &Value, t: &Target) -> String {
    if t.orphans {
        return "Open items that belong to no concept".into();
    }
    if let Some(g) = t.group.filter(|g| !g.is_empty()) {
        return format!("Group {g}");
    }
    if let Some(th) = t.theme.filter(|th| !th.is_empty()) {
        return format!("Theme {th}");
    }
    format!(
        "{}  {}",
        a["target"]["id"].as_str().unwrap_or_default(),
        a["target"]["title"].as_str().unwrap_or_default()
    )
}

/// The lines under the head: a package's progress and files, or where a concept's members sit.
#[must_use]
pub fn breakdown_lines(b: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if b["progress"].is_object() {
        out.push(progress_line(&b["progress"]));
        let touches: Vec<&str> = b["touches"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !touches.is_empty() {
            out.push(format!(
                "touches, {} files: {}",
                touches.len(),
                touches.join(", ")
            ));
        }
    }
    for label in ["by plan", "by package"] {
        let parts: Vec<String> = b[label]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| {
                format!(
                    "{} {} ({} open)",
                    p[0].as_str().unwrap_or_default(),
                    p[1].as_u64().unwrap_or(0),
                    p[2].as_u64().unwrap_or(0)
                )
            })
            .collect();
        if !parts.is_empty() {
            out.push(format!("{label}: {}", parts.join(", ")));
        }
    }
    if let Some(n) = b["shared"].as_u64() {
        let what = if b["of"] == "concept" {
            "concepts"
        } else {
            "central ideas"
        };
        out.push(format!("{n} of these belong to other {what} too"));
    }
    out
}

struct Report<'a> {
    a: &'a Value,
    rows: &'a [Value],
    sections: &'a [(&'a str, Vec<&'a Value>)],
    off_head: &'a [String],
    nowhere: &'a [String],
    breakdown: &'a [String],
}

fn ids_of(rows: &[&Value]) -> Py {
    Py::List(rows.iter().map(|r| Py::from_value(&r["id"])).collect())
}

impl Report<'_> {
    fn principles(&self) -> Vec<(u64, String)> {
        self.a["principles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| Some((p[0].as_u64()?, p[1].as_str()?.to_string())))
            .collect()
    }

    fn served(&self, n: u64) -> Vec<Value> {
        self.a["served"][n.to_string()]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    fn json(&self) -> Py {
        let target = &self.a["target"];
        let blocked = &self.sections[4].1;
        let principles = self
            .principles()
            .into_iter()
            .map(|(n, text)| {
                let by = self
                    .served(n)
                    .iter()
                    .map(|x| Py::from_value(&x["id"]))
                    .collect();
                (
                    n.to_string(),
                    Py::Dict(vec![
                        ("text".into(), Py::str(text)),
                        ("served_by".into(), Py::List(by)),
                    ]),
                )
            })
            .collect();
        let bound = self.a["bound"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|b| {
                let ids = b[1]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|x| Py::from_value(&x["id"]));
                (
                    b[0].as_str().unwrap_or_default().to_string(),
                    Py::List(ids.collect()),
                )
            })
            .collect();
        Py::Dict(vec![
            ("target".into(), Py::from_value(&target["id"])),
            (
                "sections".into(),
                Py::Dict(
                    self.sections
                        .iter()
                        .map(|(k, v)| ((*k).to_string(), ids_of(v)))
                        .collect(),
                ),
            ),
            (
                "blocked_on".into(),
                Py::Dict(
                    blocked
                        .iter()
                        .map(|r| {
                            (
                                r["id"].as_str().unwrap_or_default().to_string(),
                                Py::from_value(&r["wait_ref"]),
                            )
                        })
                        .collect(),
                ),
            ),
            ("closed_off_head".into(), Py::strs(self.off_head)),
            ("closed_on_no_commit".into(), Py::strs(self.nowhere)),
            ("principles".into(), Py::Dict(principles)),
            ("bound".into(), Py::Dict(bound)),
            ("breakdown".into(), Py::strs(self.breakdown)),
        ])
    }

    fn print(&self, head: &str) {
        let target = &self.a["target"];
        let open: usize = self.sections[..7].iter().map(|(_, v)| v.len()).sum();
        let shown = target["word"]
            .as_str()
            .map_or(String::new(), |w| format!(", {w}"));
        println!("{head}\n{} items, {open} open{shown}", self.rows.len());
        for line in self.breakdown {
            println!("  {line}");
        }
        let package = target["kind"] == "package";
        for (k, items) in self.sections {
            if items.is_empty() {
                continue;
            }
            println!("  {k:<9}{:>4}", items.len());
            if matches!(*k, "done" | "dropped" | "standing") && !(package && *k != "standing") {
                continue;
            }
            for r in items {
                let extra = match *k {
                    "building" | "checking" => format!("  {}", or_none(r["claim_branch"].as_str())),
                    "blocked" => format!("  on {}", cut(or_none(r["wait_ref"].as_str()), 50)),
                    _ => String::new(),
                };
                println!(
                    "    {:<7}{}{extra}",
                    r["id"].as_str().unwrap_or_default(),
                    cut(r["title"].as_str().unwrap_or_default(), 80)
                );
            }
        }
        for (ids, what) in [
            (self.nowhere, "closed naming no commit any repo has"),
            (
                self.off_head,
                "closed on a commit not on HEAD (unmerged, or rewritten by a squash)",
            ),
        ] {
            if !ids.is_empty() {
                let more = if ids.len() > 25 { " ..." } else { "" };
                println!(
                    "  {} {what}: {}{more}",
                    ids.len(),
                    ids[..ids.len().min(25)].join(" ")
                );
            }
        }
        self.print_principles(target["id"].as_str().unwrap_or_default());
        self.print_bound();
    }

    fn print_principles(&self, target: &str) {
        let principles = self.principles();
        if principles.is_empty() {
            return;
        }
        println!("\nprinciples");
        for (n, text) in principles {
            let by = self.served(n);
            let state = if by.is_empty() {
                format!("gap: nothing cites {target}#{n}")
            } else {
                let ids: Vec<&str> = by.iter().filter_map(|x| x["id"].as_str()).collect();
                let open = by.iter().filter(|x| x["state"] == "open").count();
                let tail = if open > 0 {
                    format!("  ({open} open)")
                } else {
                    "  (done)".to_string()
                };
                format!("{}{tail}", ids.join(" "))
            };
            println!("  {n:>2}  {:<60}  {state}", cut(&text, 60));
        }
    }

    fn print_bound(&self) {
        let bound: HashMap<&str, &Value> = self.a["bound"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|b| Some((b[0].as_str()?, &b[1])))
            .collect();
        for (kind, label) in [
            ("idea", "central ideas"),
            ("story", "stories"),
            ("concept", "concepts"),
        ] {
            let Some(list) = bound
                .get(kind)
                .and_then(|l| l.as_array())
                .filter(|l| !l.is_empty())
            else {
                continue;
            };
            let parts: Vec<String> = list
                .iter()
                .map(|x| {
                    format!(
                        "{} {}",
                        x["id"].as_str().unwrap_or_default(),
                        cut(x["title"].as_str().unwrap_or_default(), 50)
                    )
                })
                .collect();
            println!("\n{label}: {}", parts.join(", "));
        }
    }
}

// ---- stale --------------------------------------------------------------------

/// Build output and dependency trees, pruned by name.
const BUILD_DIRS: [&str; 13] = [
    ".git",
    "build",
    "dist",
    "target",
    ".svelte-kit",
    ".expo",
    ".gradle",
    ".cxx",
    "Pods",
    "DerivedData",
    ".next",
    "coverage",
    ".jest-cache",
];
/// Trees a citation may point into that no repository root holds: someone else's code.
const VENDOR_DIRS: [&str; 3] = ["node_modules", "vendor", ".cargo"];

static CRATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.-]+-\d+\.\d+\.\d+").unwrap());

/// Every file under a directory, not following links, skipping the pruned names.
fn walk(top: &Path, prune: &dyn Fn(&str) -> bool, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(top) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(kind) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        // A link to a directory is listed as one and never entered; any other link is a file.
        if kind.is_symlink() && e.path().is_dir() {
            continue;
        }
        if kind.is_dir() {
            if !prune(&name) {
                walk(&e.path(), prune, out);
            }
        } else {
            out.push(e.path().to_string_lossy().into_owned());
        }
    }
}

/// The files under each search root, and under its dependency trees once one is asked for.
#[derive(Default)]
struct Index {
    own: HashMap<String, Vec<String>>,
    vendored: HashMap<String, Vec<String>>,
}

impl Index {
    fn own(&mut self, root: &str) -> &[String] {
        self.own.entry(root.to_string()).or_insert_with(|| {
            let mut out = Vec::new();
            let prune = |n: &str| BUILD_DIRS.contains(&n) || VENDOR_DIRS.contains(&n);
            walk(Path::new(root), &prune, &mut out);
            out
        })
    }

    fn vendored(&mut self, root: &str) -> &[String] {
        self.vendored.entry(root.to_string()).or_insert_with(|| {
            let mut out = Vec::new();
            for name in VENDOR_DIRS {
                let top = Path::new(root).join(name);
                if top.is_dir() {
                    walk(&top, &|n: &str| BUILD_DIRS.contains(&n), &mut out);
                }
            }
            out
        })
    }

    /// Whether a citation names a file under a root: as a path, by its ending, or in a dependency.
    fn resolves(&mut self, roots: &[String], path: &str) -> bool {
        let bare = path.trim_start_matches(['.', '/']);
        let suffix = format!("/{bare}");
        if roots.iter().any(|s| Path::new(s).join(path).exists()) {
            return true;
        }
        if roots
            .iter()
            .any(|s| self.own(s).iter().any(|f| f.ends_with(&suffix)))
        {
            return true;
        }
        let first = bare.split('/').next().unwrap_or_default();
        CRATE.is_match(first)
            || roots
                .iter()
                .any(|s| self.vendored(s).iter().any(|f| f.ends_with(&suffix)))
    }
}

/// # Errors
/// The project has no root on this machine, or the server refuses.
pub fn stale(ctx: &mut Ctx, open_only: bool) -> Result<i32> {
    let slug = ctx.project()?;
    let project = ctx.project_row(&slug)?;
    let roots: Vec<String> = ctx
        .roots
        .roots
        .iter()
        .filter(|r| r.project == slug)
        .map(|r| r.path.clone())
        .collect();
    if roots.is_empty() {
        return Err(Fail::refused(format!(
            "no root is bound for {slug} on {}, so no citation can be resolved.",
            ctx.host()?
        )));
    }
    let mut search = Vec::new();
    for root in &roots {
        search.push(root.clone());
        for rel in project["cite_roots"].as_array().into_iter().flatten() {
            let rel = rel.as_str().unwrap_or_default();
            search.push(expand(root, rel).to_string_lossy().into_owned());
        }
    }
    let cited = ctx.read(
        "/citations",
        &[("open_only", open_only.then(|| "true".into()))],
    )?;
    let mut index = Index::default();
    let mut cache: HashMap<String, bool> = HashMap::new();
    let mut missing: Vec<(String, String)> = Vec::new();
    for c in cited.as_array().into_iter().flatten() {
        let path = c["path"].as_str().unwrap_or_default().to_string();
        let ok = *cache
            .entry(path.clone())
            .or_insert_with(|| index.resolves(&search, &path));
        if !ok {
            missing.push((c["id"].as_str().unwrap_or_default().to_string(), path));
        }
    }
    print_stale(ctx, &missing, search.len());
    Ok(0)
}

fn print_stale(ctx: &Ctx, missing: &[(String, String)], roots: usize) {
    if ctx.json {
        let rows = missing
            .iter()
            .map(|(i, p)| Py::Dict(vec![("id".into(), Py::str(i)), ("path".into(), Py::str(p))]))
            .collect();
        ctx.emit(&Py::List(rows));
        return;
    }
    if missing.is_empty() {
        println!("Every citation resolves.");
        return;
    }
    let mut last: Option<&str> = None;
    for (i, p) in missing {
        if last != Some(i.as_str()) {
            println!("{i}");
            last = Some(i);
        }
        println!("  {p}");
    }
    let items: HashSet<&str> = missing.iter().map(|(i, _)| i.as_str()).collect();
    println!(
        "\n{} citations in {} items do not resolve under {roots} roots.",
        missing.len(),
        items.len()
    );
}

#[cfg(test)]
#[path = "../tests/audit.rs"]
mod tests;
