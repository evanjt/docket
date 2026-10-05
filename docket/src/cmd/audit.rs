//! `docket audit` and `docket stale`: what the server holds, read against this machine's checkouts.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use docket_core::label::{self, Label};

use crate::cmd::show::progress_line;
use crate::ctx::{Ctx, id};
use crate::fail::{Fail, Result};
use crate::local::{self, expand};
use crate::py::{Py, cut, or_none};

const SECTIONS: [&str; 7] = [
    "ready", "building", "checking", "blocked", "parked", "done", "dropped",
];

/// The words that are not open.
const NOT_OPEN: [&str; 2] = ["done", "dropped"];

/// The rows under each of the words, in the order the sections are printed.
#[must_use]
pub fn sections_of(rows: &[Value]) -> Vec<(&'static str, Vec<&Value>)> {
    let mut sections: Vec<(&str, Vec<&Value>)> =
        SECTIONS.iter().map(|k| (*k, Vec::new())).collect();
    for r in rows {
        if let Some((_, list)) = sections.iter_mut().find(|(k, _)| r["word"] == *k) {
            list.push(r);
        }
    }
    sections
}

/// The rows under one word; none for a word the sections do not hold.
#[must_use]
pub fn section<'a, 'b>(sections: &'a [(&str, Vec<&'b Value>)], word: &str) -> &'a [&'b Value] {
    sections
        .iter()
        .find(|(k, _)| *k == word)
        .map_or(&[], |(_, v)| v.as_slice())
}

/// How many rows are open: every word but done and dropped.
#[must_use]
pub fn open_count(sections: &[(&str, Vec<&Value>)]) -> usize {
    sections
        .iter()
        .filter(|(k, _)| !NOT_OPEN.contains(k))
        .map(|(_, v)| v.len())
        .sum()
}

static SHA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b([0-9a-f]{7,40})\b").unwrap());

/// The shas a resolution names.
#[must_use]
pub fn shas(resolution: &str) -> Vec<String> {
    SHA.captures_iter(resolution)
        .map(|m| m[1].to_string())
        .collect()
}

/// Where a closing sha sits against the integration branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShaState {
    OnIntegration,
    /// On branches other than the integration branch only.
    Unmerged(Vec<String>),
    /// A commit no branch holds, a tag or a dangling object.
    NoBranch,
    /// No repository here has the commit, or the resolution names none.
    Gone,
}

impl ShaState {
    fn rank(&self) -> u8 {
        match self {
            Self::OnIntegration => 3,
            Self::Unmerged(_) => 2,
            Self::NoBranch => 1,
            Self::Gone => 0,
        }
    }
}

/// Where one sha sits in the first of the directories that has it. The integration ref is the
/// measure, and HEAD only when the project stores none. A commit whose patch is already on the
/// integration branch under another sha, as after a squash or cherry-pick, counts as on it.
#[must_use]
pub fn sha_state(dirs: &[PathBuf], sha: &str, integration: Option<&str>) -> ShaState {
    let target = integration.filter(|r| !r.is_empty()).unwrap_or("HEAD");
    let mut best = ShaState::Gone;
    for d in dirs {
        let commit = format!("{sha}^{{commit}}");
        let Some(full) = local::git(&["rev-parse", "--verify", "--quiet", &commit], d) else {
            continue;
        };
        if local::git(&["merge-base", "--is-ancestor", &full, target], d).is_some() {
            return ShaState::OnIntegration;
        }
        // A root commit has no parent, so it cannot be compared by patch.
        let parent = format!("{full}^");
        let picked = local::git(&["cherry", target, &full, &parent], d)
            .is_some_and(|out| out.starts_with('-'));
        if picked {
            return ShaState::OnIntegration;
        }
        let listed = local::git(
            &[
                "branch",
                "--all",
                "--contains",
                &full,
                "--format=%(refname:short)",
            ],
            d,
        )
        .unwrap_or_default();
        let branches: Vec<String> = listed
            .lines()
            .map(str::trim)
            .filter(|b| !b.is_empty() && !b.ends_with("/HEAD") && !b.starts_with('('))
            .map(str::to_string)
            .collect();
        let state = if branches.is_empty() {
            ShaState::NoBranch
        } else {
            ShaState::Unmerged(branches)
        };
        if state.rank() > best.rank() {
            best = state;
        }
    }
    best
}

/// Each done work item with the best state any of its resolution's shas has. One naming no sha is
/// gone.
pub fn closed_states(
    done: &[Value],
    mut state: impl FnMut(&str) -> ShaState,
) -> Vec<(String, ShaState)> {
    let mut out = Vec::new();
    for r in done {
        if r["state"] != "done" || r["kind"] != "work" {
            continue;
        }
        let best = shas(r["resolution"].as_str().unwrap_or_default())
            .iter()
            .map(|s| state(s))
            .max_by_key(ShaState::rank)
            .unwrap_or(ShaState::Gone);
        out.push((r["id"].as_str().unwrap_or_default().to_string(), best));
    }
    out
}

static CLOSE_SUBJECT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Close ([A-Za-z]+\d+)\b").unwrap());

/// The other item a commit subject says it closes, when it names one that is not `id`.
#[must_use]
pub fn closes_another<'a>(id: &str, subject: &'a str) -> Option<&'a str> {
    CLOSE_SUBJECT
        .captures(subject)
        .and_then(|m| m.get(1))
        .map(|m| m.as_str())
        .filter(|other| !other.eq_ignore_ascii_case(id))
}

/// `(item, other)` for each done item whose resolution names a commit whose subject closes `other`.
/// `subject_of` gives a sha's commit subject, none for a sha no repository here resolves.
#[must_use]
pub fn misattributed(
    done: &[Value],
    subject_of: &dyn Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for r in done {
        if r["state"] != "done" {
            continue;
        }
        let id = r["id"].as_str().unwrap_or_default();
        let other = shas(r["resolution"].as_str().unwrap_or_default())
            .iter()
            .find_map(|s| {
                subject_of(s).and_then(|subj| closes_another(id, &subj).map(str::to_string))
            });
        if let Some(other) = other {
            out.push((id.to_string(), other));
        }
    }
    out
}

/// The subject of a commit in any of the directories.
fn subject_in(dirs: &[PathBuf], sha: &str) -> Option<String> {
    dirs.iter()
        .find_map(|d| local::git(&["log", "-1", "--format=%s", sha], d))
        .filter(|s| !s.is_empty())
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
    pub area: Option<&'a String>,
}

/// # Errors
/// No id, group, theme or area is named, the project cannot be resolved, or the id is unknown.
pub fn audit(ctx: &mut Ctx, t: &Target) -> Result<i32> {
    let slug = ctx.project()?;
    let given = |v: Option<&String>| v.filter(|s| !s.is_empty()).cloned();
    let item = match given(t.id) {
        Some(x)
            if given(t.group).is_none() && given(t.theme).is_none() && given(t.area).is_none() =>
        {
            Some(id(&x)?)
        }
        _ => None,
    };
    let a = ctx.read(
        "/audit",
        &[
            ("group", given(t.group)),
            ("theme", given(t.theme)),
            ("area", given(t.area)),
            ("id", item),
        ],
    )?;
    let project = ctx.project_row(&slug)?;
    let rows = a["rows"].as_array().cloned().unwrap_or_default();
    let sections = sections_of(&rows);
    let done: Vec<Value> = section(&sections, "done")
        .iter()
        .map(|r| (*r).clone())
        .collect();
    let dirs = repo_dirs(ctx, &slug, &project);
    let integration = project["integration_ref"].as_str();
    let states = closed_states(&done, |s| sha_state(&dirs, s, integration));
    let ids = |keep: fn(&ShaState) -> bool| -> Vec<String> {
        states
            .iter()
            .filter(|(_, st)| keep(st))
            .map(|(i, _)| i.clone())
            .collect()
    };
    let off_head = ids(|st| matches!(st, ShaState::Unmerged(_)));
    let nowhere = ids(|st| matches!(st, ShaState::NoBranch | ShaState::Gone));
    let wrong_close = misattributed(&done, &|sha| subject_in(&dirs, sha));
    let breakdown = breakdown_lines(&a["breakdown"]);
    let report = Report {
        a: &a,
        rows: &rows,
        sections: &sections,
        off_head: &off_head,
        nowhere: &nowhere,
        wrong_close: &wrong_close,
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
    if let Some(name) = t.area.filter(|n| !n.is_empty()) {
        return format!("Area {}", a["area"]["name"].as_str().unwrap_or(name));
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

/// The area an audit is of or an item sits in: its description, then its priority when it has one.
#[must_use]
pub fn area_lines(area: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(about) = area["description"].as_str().filter(|d| !d.is_empty()) {
        out.push(about.to_string());
    }
    if let Some(p) = area["priority"].as_str() {
        out.push(format!("priority: {p}"));
    }
    out
}

/// The lines under the head: a package's progress and files.
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
    out
}

struct Report<'a> {
    a: &'a Value,
    rows: &'a [Value],
    sections: &'a [(&'a str, Vec<&'a Value>)],
    off_head: &'a [String],
    nowhere: &'a [String],
    wrong_close: &'a [(String, String)],
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
        let blocked = section(self.sections, "blocked");
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
            (
                "closed_on_another_items_close".into(),
                Py::List(
                    self.wrong_close
                        .iter()
                        .map(|(id, other)| Py::str(format!("{id} closes {other}")))
                        .collect(),
                ),
            ),
            ("principles".into(), Py::Dict(principles)),
            ("labels".into(), Py::strs(&self.labels())),
            ("bound".into(), Py::Dict(bound)),
            ("breakdown".into(), Py::strs(self.breakdown)),
            ("area".into(), Py::from_value(&self.a["area"]["name"])),
        ])
    }

    fn print(&self, head: &str) {
        let target = &self.a["target"];
        let open = open_count(self.sections);
        let shown = target["word"]
            .as_str()
            .map_or(String::new(), |w| format!(", {w}"));
        println!("{head}\n{} items, {open} open{shown}", self.rows.len());
        if !target.is_null()
            && let Some(name) = self.a["area"]["name"].as_str()
        {
            println!("  area: {name}");
        }
        for line in area_lines(&self.a["area"]) {
            println!("  {line}");
        }
        for line in self.breakdown {
            println!("  {line}");
        }
        let package = target["kind"] == "package";
        for (k, items) in self.sections {
            if items.is_empty() {
                continue;
            }
            println!("  {k:<9}{:>4}", items.len());
            if matches!(*k, "done" | "dropped") && !package {
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
            (self.nowhere, "closed naming no commit on any branch"),
            (
                self.off_head,
                "closed on a commit only an unmerged branch has",
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
        if !self.wrong_close.is_empty() {
            let list: Vec<String> = self
                .wrong_close
                .iter()
                .map(|(id, other)| format!("{id} (commit closes {other})"))
                .collect();
            println!(
                "  {} closed on a commit whose subject closes another item: {}",
                list.len(),
                list.join(" ")
            );
        }
        self.print_principles(target["id"].as_str().unwrap_or_default());
        self.print_labels();
        self.print_bound();
    }

    /// The labels the target carries, each with its description.
    fn labels(&self) -> Vec<String> {
        let found: Vec<Label> =
            serde_json::from_value(self.a["labels"].clone()).unwrap_or_default();
        found.iter().map(label::line).collect()
    }

    fn print_labels(&self) {
        let labels = self.labels();
        if labels.is_empty() {
            return;
        }
        println!("\nlabels");
        for l in labels {
            println!("  {l}");
        }
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
        for (kind, label) in [("idea", "central ideas"), ("story", "stories")] {
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

/// The files a repository tracks, or none when git cannot list them.
fn tracked(repo: &Path) -> Vec<String> {
    let listed = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-z"])
        .output();
    let Ok(listed) = listed else {
        return Vec::new();
    };
    String::from_utf8_lossy(&listed.stdout)
        .split('\0')
        .filter(|f| !f.is_empty())
        .map(|f| repo.join(f).to_string_lossy().into_owned())
        .collect()
}

/// Every file under a directory, not following links, skipping the pruned names. A repository
/// contributes only what it tracks, and a linked worktree is skipped, so untracked copies and
/// stale checkouts of a deleted file do not resolve a citation to it.
fn walk(top: &Path, prune: &dyn Fn(&str) -> bool, out: &mut Vec<String>) {
    let marker = top.join(".git");
    if marker.is_file() {
        return;
    }
    if marker.is_dir() {
        out.extend(tracked(top));
        return;
    }
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

    /// The files a citation names under the roots, as a path or by its ending, and whether a
    /// dependency tree holds it when none does.
    fn find(&mut self, roots: &[String], path: &str) -> (Vec<String>, bool) {
        let bare = path.trim_start_matches(['.', '/']);
        let suffix = format!("/{bare}");
        let mut found: Vec<String> = roots
            .iter()
            .map(|s| Path::new(s).join(path))
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        for s in roots {
            found.extend(self.own(s).iter().filter(|f| f.ends_with(&suffix)).cloned());
        }
        if !found.is_empty() {
            return (found, false);
        }
        let first = bare.split('/').next().unwrap_or_default();
        let held = CRATE.is_match(first)
            || roots
                .iter()
                .any(|s| self.vendored(s).iter().any(|f| f.ends_with(&suffix)));
        (found, held)
    }
}

/// How a citation stands against the files it resolves to.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Resolves,
    Missing,
    /// The cited line is past the end of the longest matching file, which has this many lines.
    PastEnd(usize),
    /// A cited test that resolves only under a `benches` directory, which `cargo test` does not run.
    Benches,
}

/// Judge one citation from its link kind, its line, the files it resolved to with their line
/// counts, and whether a dependency tree held it when no file did.
fn judge(kind: &str, line: Option<u64>, found: &[(String, usize)], held: bool) -> Verdict {
    if found.is_empty() {
        return if held {
            Verdict::Resolves
        } else {
            Verdict::Missing
        };
    }
    let in_benches = |p: &str| {
        Path::new(p)
            .components()
            .any(|c| c.as_os_str() == "benches")
    };
    let candidates: Vec<&(String, usize)> = if kind == "cites_test" {
        found.iter().filter(|(p, _)| !in_benches(p)).collect()
    } else {
        found.iter().collect()
    };
    if candidates.is_empty() {
        return Verdict::Benches;
    }
    let longest = candidates.iter().map(|(_, n)| *n).max().unwrap_or(0);
    match line {
        Some(n) if n > longest as u64 => Verdict::PastEnd(longest),
        _ => Verdict::Resolves,
    }
}

/// The number of lines in a file, 0 when it cannot be read as text.
fn line_count(path: &str) -> usize {
    std::fs::read_to_string(path).map_or(0, |t| t.lines().count())
}

static BRANCH_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"audit/[^\s`'",;)\]]+"#).unwrap());
static BRANCH_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^audit/[A-Za-z]+[0-9]+-[0-9]+$").unwrap());

/// The `audit/<key><number>-<n>` branches a body names, once each. A wildcard, a placeholder or a
/// script name is not a branch.
#[must_use]
pub fn cited_branches(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in BRANCH_TOKEN.find_iter(body) {
        let name = m.as_str().trim_end_matches('.');
        if BRANCH_NAME.is_match(name) && !out.iter().any(|o| o == name) {
            out.push(name.to_string());
        }
    }
    out
}

/// The `(item, branch)` pairs whose branch no directory has as a local branch.
#[must_use]
pub fn gone_branches(cited: &[(String, String)], dirs: &[PathBuf]) -> Vec<(String, String)> {
    cited
        .iter()
        .filter(|(_, b)| {
            let r = format!("refs/heads/{b}");
            !dirs
                .iter()
                .any(|d| local::git(&["rev-parse", "--verify", "-q", &r], d).is_some())
        })
        .cloned()
        .collect()
}

static JOB_BRANCH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:lead|audit)/([a-z]+\d+)-").unwrap());

/// The item a job branch is for: `lead/b57-78530` is for `B57`.
#[must_use]
pub fn job_branch_item(branch: &str) -> Option<String> {
    JOB_BRANCH
        .captures(branch)
        .and_then(|m| m.get(1))
        .map(|m| m.as_str().to_uppercase())
}

/// A job branch that no live claim or job needs.
#[derive(Debug, PartialEq, Eq)]
pub struct StaleBranch {
    pub name: String,
    /// Why it is stale: its item is closed or dropped, or nothing names the branch.
    pub why: String,
    /// Commits on it whose patch is not on the work ref.
    pub unique: usize,
}

/// The `lead/` and `audit/` branches in these directories whose item is closed or dropped, or that
/// no claim or job names, with how many commits each holds that the work ref lacks. A commit
/// whose patch is on the work ref under another sha is not counted. A branch whose item is open
/// and which a claim or job names is not stale. `state_of` gives an item's state word.
#[must_use]
pub fn stale_job_branches(
    dirs: &[PathBuf],
    work_ref: Option<&str>,
    state_of: impl Fn(&str) -> Option<String>,
    named: &HashSet<String>,
) -> Vec<StaleBranch> {
    let target = work_ref.filter(|r| !r.is_empty()).unwrap_or("HEAD");
    let mut out: Vec<StaleBranch> = Vec::new();
    for d in dirs {
        let listed = local::git(
            &[
                "for-each-ref",
                "--format=%(refname:short)",
                "refs/heads/lead",
                "refs/heads/audit",
            ],
            d,
        )
        .unwrap_or_default();
        for name in listed.lines().filter(|n| JOB_BRANCH.is_match(n)) {
            if out.iter().any(|b| b.name == name) {
                continue;
            }
            let closed = job_branch_item(name)
                .and_then(|i| state_of(&i))
                .filter(|s| NOT_OPEN.contains(&s.as_str()));
            let why = match closed {
                Some(s) => format!("its item is {s}"),
                None if !named.contains(name) => "no claim or job names it".to_string(),
                None => continue,
            };
            let unique = local::git(&["cherry", target, name], d)
                .map_or(0, |o| o.lines().filter(|l| l.starts_with('+')).count());
            out.push(StaleBranch {
                name: name.to_string(),
                why,
                unique,
            });
        }
    }
    out
}

/// Delete the stale branches that hold nothing the work ref lacks, and give the names of the rest.
/// A branch git will not delete, as one checked out in a worktree, is kept.
#[must_use]
pub fn prune_job_branches(dirs: &[PathBuf], stale: &[StaleBranch]) -> Vec<String> {
    let mut kept = Vec::new();
    for b in stale {
        let removed = b.unique == 0
            && dirs.iter().any(|d| {
                let r = format!("refs/heads/{}", b.name);
                local::git(&["rev-parse", "--verify", "-q", &r], d).is_some()
                    && local::git(&["branch", "-D", &b.name], d).is_some()
            });
        if !removed {
            kept.push(b.name.clone());
        }
    }
    kept
}

/// # Errors
/// The project has no root on this machine, or the server refuses.
pub fn stale(ctx: &mut Ctx, open_only: bool, prune: bool) -> Result<i32> {
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
    let mut cache: HashMap<String, (Vec<String>, bool)> = HashMap::new();
    let mut lines: HashMap<String, usize> = HashMap::new();
    let mut found_none: Vec<Finding> = Vec::new();
    for c in cited.as_array().into_iter().flatten() {
        let path = c["path"].as_str().unwrap_or_default().to_string();
        let kind = c["kind"].as_str().unwrap_or("cites_file");
        let line = c["line"].as_u64();
        let (files, held) = cache
            .entry(path.clone())
            .or_insert_with(|| index.find(&search, &path))
            .clone();
        let sized: Vec<(String, usize)> = files
            .into_iter()
            .map(|f| {
                let n = if line.is_some() {
                    *lines.entry(f.clone()).or_insert_with(|| line_count(&f))
                } else {
                    0
                };
                (f, n)
            })
            .collect();
        let reason = match judge(kind, line, &sized, held) {
            Verdict::Resolves => continue,
            Verdict::Missing => Reason::Missing,
            Verdict::PastEnd(n) => Reason::PastEnd(line.unwrap_or(0), n),
            Verdict::Benches => Reason::Benches,
        };
        found_none.push(Finding {
            id: c["id"].as_str().unwrap_or_default().to_string(),
            path,
            reason,
        });
    }
    let bodies = ctx.read("/open_bodies", &[])?;
    let mut branches: Vec<(String, String)> = Vec::new();
    for b in bodies.as_array().into_iter().flatten() {
        let id = b["id"].as_str().unwrap_or_default();
        for name in cited_branches(b["body"].as_str().unwrap_or_default()) {
            branches.push((id.to_string(), name));
        }
    }
    let gone = gone_branches(&branches, &repo_dirs(ctx, &slug, &project));
    let closed = closed_off_integration(ctx, &slug, &project)?;
    print_stale(ctx, &found_none, &gone, search.len());
    print_closed(ctx, &closed);
    job_branches(ctx, &slug, &project, prune)?;
    Ok(0)
}

/// Report the job branches nothing needs, and with `prune` delete those holding no unlanded work.
fn job_branches(ctx: &mut Ctx, slug: &str, project: &Value, prune: bool) -> Result<()> {
    let dirs = repo_dirs(ctx, slug, project);
    if dirs.is_empty() {
        return Ok(());
    }
    let filter = serde_json::json!({ "project": slug }).to_string();
    let items = ctx.api.get(
        "/items",
        &[("filter", filter), ("range", "[0,99999]".to_string())],
    )?;
    let states: HashMap<String, String> = items
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            Some((
                r["id"].as_str()?.to_string(),
                r["state"].as_str()?.to_string(),
            ))
        })
        .collect();
    let mut named: HashSet<String> = HashSet::new();
    let claims: Value = ctx.api.get("/wip", &[("project", slug.to_string())])?;
    for r in claims.as_array().into_iter().flatten() {
        if let Some(b) = r["claim_branch"].as_str() {
            named.insert(b.to_string());
        }
    }
    for (r, _) in crate::job::rows(&crate::job::state_root(), crate::job::now()) {
        if r.project == slug {
            named.insert(r.branch);
        }
    }
    let stale = stale_job_branches(
        &dirs,
        project["integration_ref"].as_str(),
        |i| states.get(i).cloned(),
        &named,
    );
    if stale.is_empty() {
        return Ok(());
    }
    let kept = if prune {
        prune_job_branches(&dirs, &stale)
    } else {
        Vec::new()
    };
    if ctx.json {
        return Ok(());
    }
    println!("\nJob branches nothing needs:");
    for b in &stale {
        let held = if b.unique == 0 {
            "no commits the work ref lacks".to_string()
        } else {
            format!("{} commits the work ref lacks", b.unique)
        };
        let done = if prune && !kept.contains(&b.name) {
            ", removed"
        } else {
            ""
        };
        println!("  {}  {}; {held}{done}", b.name, b.why);
    }
    if !prune {
        println!("docket stale --prune removes those with no commits the work ref lacks.");
    }
    Ok(())
}

/// The done work items whose closing sha is not on the integration branch, with where it is.
fn closed_off_integration(
    ctx: &Ctx,
    slug: &str,
    project: &Value,
) -> Result<Vec<(String, ShaState)>> {
    let dirs = repo_dirs(ctx, slug, project);
    if dirs.is_empty() {
        return Ok(Vec::new());
    }
    let filter = serde_json::json!({ "project": slug, "state": "done" }).to_string();
    let rows = ctx.api.get(
        "/items",
        &[("filter", filter), ("range", "[0,99999]".to_string())],
    )?;
    let other: Vec<String> = project["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|k| k["kind"] != "work")
        .filter_map(|k| k["key"].as_str().map(str::to_string))
        .collect();
    let done: Vec<Value> = rows
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            let mut r = r.clone();
            let key = r["key"].as_str().unwrap_or_default();
            r["kind"] = Value::from(if other.iter().any(|k| k == key) {
                "other"
            } else {
                "work"
            });
            r
        })
        .collect();
    let integration = project["integration_ref"].as_str();
    let mut cache: HashMap<String, ShaState> = HashMap::new();
    let mut states = closed_states(&done, |s| {
        cache
            .entry(s.to_string())
            .or_insert_with(|| sha_state(&dirs, s, integration))
            .clone()
    });
    states.retain(|(_, st)| *st != ShaState::OnIntegration);
    Ok(states)
}

fn print_closed(ctx: &Ctx, closed: &[(String, ShaState)]) {
    if ctx.json {
        return;
    }
    for (i, st) in closed {
        let what = match st {
            ShaState::OnIntegration => continue,
            ShaState::Unmerged(b) => format!("only on unmerged {}", b.join(", ")),
            ShaState::NoBranch => "on no branch".to_string(),
            ShaState::Gone => "gone, or names no commit".to_string(),
        };
        println!("{i} closed {what}");
    }
}

/// Why a citation is reported.
enum Reason {
    Missing,
    /// The cited line, and the length of the longest matching file.
    PastEnd(u64, usize),
    Benches,
}

struct Finding {
    id: String,
    path: String,
    reason: Reason,
}

/// Whether a finding belongs in a section of the report.
type Pick = fn(&Reason) -> bool;

fn print_stale(ctx: &Ctx, found: &[Finding], gone: &[(String, String)], roots: usize) {
    if ctx.json {
        let rows = found
            .iter()
            .map(|f| {
                let reason = match f.reason {
                    Reason::Missing => "missing",
                    Reason::PastEnd(..) => "past_end",
                    Reason::Benches => "benches",
                };
                Py::Dict(vec![
                    ("id".into(), Py::str(&f.id)),
                    ("path".into(), Py::str(&f.path)),
                    ("reason".into(), Py::str(reason)),
                ])
            })
            .chain(gone.iter().map(|(i, b)| {
                Py::Dict(vec![
                    ("id".into(), Py::str(i)),
                    ("branch".into(), Py::str(b)),
                ])
            }))
            .collect();
        ctx.emit(&Py::List(rows));
        return;
    }
    if found.is_empty() && gone.is_empty() {
        println!("Every citation resolves.");
        return;
    }
    if found.is_empty() {
        println!("Every cited path resolves.");
    } else {
        print_findings(found, roots);
    }
    if !gone.is_empty() {
        println!("\nBranches an open item names that no repository here has:");
        for (i, b) in gone {
            println!("  {i}  {b}");
        }
        println!("A gone branch is not proof of lost work: it may be merged and deleted.");
    }
}

fn print_findings(found: &[Finding], roots: usize) {
    let sections: [(&str, Pick); 3] = [
        ("Do not resolve:", |r| matches!(r, Reason::Missing)),
        ("Cite a line past the end of the file:", |r| {
            matches!(r, Reason::PastEnd(..))
        }),
        (
            "Cite a test that resolves only under benches/, which cargo test does not run:",
            |r| matches!(r, Reason::Benches),
        ),
    ];
    for (title, pick) in sections {
        let rows: Vec<&Finding> = found.iter().filter(|f| pick(&f.reason)).collect();
        if rows.is_empty() {
            continue;
        }
        println!("{title}");
        let mut last: Option<&str> = None;
        for f in rows {
            if last != Some(f.id.as_str()) {
                println!("{}", f.id);
                last = Some(&f.id);
            }
            match f.reason {
                Reason::PastEnd(at, n) => println!("  {}:{at} (the file has {n} lines)", f.path),
                _ => println!("  {}", f.path),
            }
        }
        println!();
    }
    let items: HashSet<&str> = found.iter().map(|f| f.id.as_str()).collect();
    println!(
        "{} citations in {} items do not resolve under {roots} roots.",
        found.len(),
        items.len()
    );
}

#[cfg(test)]
#[path = "../tests/audit.rs"]
mod tests;
