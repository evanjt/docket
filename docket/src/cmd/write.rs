//! The write verbs: each one request to the server, its answer printed as the Python prints it.

use std::path::{Path, PathBuf};

use serde_json::Value;

use docket_core::api::{
    AddRequest, AnswerRequest, Answered, AskRequest, Asked, Brief, CloseRequest, Closed, Common,
    DropRequest, Dropped, EditRequest, ItemView, KeyRequest, KeySet, LinkRequest, Linked, Moved,
    MovedMany, NewRequest, Opened, PriorityRequest, Reindexed, ReleaseRequest, SetField,
    StartRequest, Started,
};

use crate::cmd::lists::shares_text;
use crate::ctx::{Ctx, id};
use crate::fail::{Fail, Result};
use crate::local;
use crate::py::cut;
use crate::row::{fmt_row, item_json, row_of};

const TEMPLATE: &str = "- **Evidence.** file:line citations, read from the working tree.
- **Failing case.** Concrete inputs that produce the wrong output.
- **Fix.** What to change, stated as a change to the code.";

const PACKAGE_TEMPLATE: &str = "- **Fact.** The one fact this package gives one owner, and where that owner lives.
- **Principles.**
  1. Each claim the tree must satisfy when it is done, checkable by reading the code or running a test.
- **Touches.** Write each path from the repository root, separated by commas.";

/// One written row: `--json`, or its line and tail.
pub fn print_item(ctx: &Ctx, item: &ItemView) {
    let v = serde_json::to_value(item).unwrap_or_default();
    if ctx.json {
        ctx.emit(&item_json(&v, &[], &[]));
    } else {
        println!("{}", fmt_row(&row_of(&v), None));
    }
}

fn print_released(ctx: &Ctx, released: &[Brief]) {
    if ctx.json {
        return;
    }
    for x in released {
        println!("released {}: {}", x.id, cut(&x.title, 70));
    }
}

/// A body from a file, or from stdin for `-`.
///
/// # Errors
/// The file cannot be read.
pub fn read_body(spec: Option<&String>) -> Result<Option<String>> {
    let Some(spec) = spec else { return Ok(None) };
    if spec == "-" {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
            .map_err(|e| Fail::refused(format!("docket: stdin: {e}")))?;
        return Ok(Some(text));
    }
    std::fs::read_to_string(spec)
        .map(Some)
        .map_err(|e| Fail::refused(format!("{spec}: {e}")))
}

/// A job under a lead files what it finds and builds its ticket, and leaves the lead's verbs (claim,
/// release, close, drop, reopen) to the lead.
///
/// # Errors
/// `DOCKET_JOB` names the job this runs in, and the verb is the lead's.
pub fn refuse_in_job(verb: &str) -> Result<()> {
    let job = std::env::var("DOCKET_JOB").ok();
    match crate::job::refusal(verb, job.as_deref()) {
        Some(why) => Err(Fail::refused(why)),
        None => Ok(()),
    }
}

/// A job names the release of every item it files.
///
/// # Errors
/// `DOCKET_JOB` names the job this runs in, and no release is given.
fn refuse_unreleased(verb: &str, release: Option<&String>) -> Result<()> {
    let job = std::env::var("DOCKET_JOB").ok();
    match crate::job::unreleased(verb, job.as_deref(), release.map(String::as_str)) {
        Some(why) => Err(Fail::refused(why)),
        None => Ok(()),
    }
}

pub struct New<'a> {
    pub key: &'a str,
    pub title: &'a str,
    pub body: Option<&'a String>,
    pub turn: Option<&'a String>,
    pub complexity: Option<&'a String>,
    pub priority: Option<&'a String>,
    pub theme: Option<&'a String>,
    pub release: Option<&'a String>,
    pub group: Option<&'a String>,
}

/// # Errors
/// The server refuses.
pub fn new(ctx: &mut Ctx, n: &New) -> Result<i32> {
    refuse_unreleased("new", n.release)?;
    let common = ctx.common(false)?;
    let body = read_body(n.body)?;
    let req = NewRequest {
        common,
        key: n.key.to_string(),
        title: n.title.to_string(),
        body: body.clone(),
        turn: n.turn.cloned(),
        complexity: n.complexity.cloned(),
        priority: n.priority.cloned(),
        theme: n.theme.cloned(),
        release: n.release.filter(|r| !r.is_empty()).cloned(),
        group: n.group.cloned(),
    };
    let out: Opened = ctx.api.post("new", &req)?;
    opened(ctx, &out, body.as_deref());
    Ok(0)
}

fn opened(ctx: &Ctx, out: &Opened, body: Option<&str>) {
    print_item(ctx, &out.item);
    if ctx.json {
        return;
    }
    if !out.decided_like.is_empty() {
        println!(
            "\nDecided questions close to this one. When one settles it, answer it:\n  docket answer {} \"the choice\" --derived \"Q<n>, what it decided\"",
            out.item.id
        );
        for t in &out.decided_like {
            println!(
                "  {:<6} {}\n         decided: {}",
                t.id,
                cut(&t.title, 70),
                cut(&t.decision, 110)
            );
        }
    }
    if body.is_none_or(str::is_empty) {
        println!(
            "\nNo body. Write it with: docket edit {} --body -",
            out.item.id
        );
        match out.kind.as_str() {
            "work" => println!("{TEMPLATE}"),
            "package" => {
                println!("{PACKAGE_TEMPLATE}");
                println!(
                    "\nThen give it its members: docket link B1 B2 opened {}",
                    out.item.id
                );
            }
            _ => {}
        }
    }
}

/// # Errors
/// The server refuses.
pub fn add(
    ctx: &mut Ctx,
    title: &str,
    key: Option<&String>,
    body: Option<&String>,
    from: Option<&String>,
    release: Option<&String>,
) -> Result<i32> {
    refuse_unreleased("add", release)?;
    let common = ctx.common(false)?;
    let body = read_body(body)?;
    let req = AddRequest {
        common,
        title: title.to_string(),
        key: key.filter(|k| !k.is_empty()).cloned(),
        body: body.clone(),
        from: from.filter(|f| !f.is_empty()).cloned(),
        release: release.filter(|r| !r.is_empty()).cloned(),
    };
    let out: Opened = ctx.api.post("add", &req)?;
    opened(ctx, &out, body.as_deref());
    Ok(0)
}

/// # Errors
/// Inside a job, or the server refuses the claim.
pub fn start(ctx: &mut Ctx, req: &StartRequest) -> Result<i32> {
    refuse_in_job("start")?;
    let out: Started = ctx.api.post("start", req)?;
    print_item(ctx, &out.item);
    if ctx.json {
        return Ok(0);
    }
    for s in &out.shares {
        println!(
            "{}",
            shares_text(&serde_json::to_value(s).unwrap_or_default())
        );
    }
    let i = &out.item.id;
    match out.kind.as_str() {
        "decision" => println!(
            "\nThis is a decided question: turn its decision into items, then docket close {i} \"opened B1, B2\"."
        ),
        "audit" => println!(
            "\nThis is a plan. With nothing opened under it yet, file its tickets, link each with docket link T1 \
             opened {i}, and release it; it comes back for its audit when they are all closed. With every ticket \
             closed, this is its audit: check the plan in its body against the working tree and against every \
             item it opened (docket deps {i}), file each gap as a ticket linked the same way, and close it. One \
             round: the gaps are worked as tickets."
        ),
        _ => {}
    }
    if let Some(hint) = out.worktree_hint.as_deref().filter(|h| !h.is_empty()) {
        println!("\n{hint}");
    }
    if let Some(g) = out.item.group.as_deref().filter(|g| !g.is_empty())
        && !out.group_others.is_empty()
    {
        println!(
            "\nGroup {g}: take {} with it, claiming each as you reach it.",
            out.group_others.join(", ")
        );
    }
    Ok(0)
}

/// # Errors
/// Inside a job, or the server refuses.
pub fn release(ctx: &mut Ctx, req: &ReleaseRequest) -> Result<i32> {
    refuse_in_job("release")?;
    let out: Moved = ctx.api.post("release", req)?;
    print_item(ctx, &out.item);
    Ok(0)
}

/// The tip of the claimed branch in one of the project's repositories here, if any.
fn claim_sha(ctx: &Ctx, slug: &str, project: &Value, branch: Option<&str>) -> Option<String> {
    let branch = branch.filter(|b| !b.is_empty() && !matches!(*b, "main" | "master" | "HEAD"))?;
    let repos: Vec<String> = project["repos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r.as_str().map(str::to_string))
        .collect();
    let rels = if repos.is_empty() {
        vec![".".to_string()]
    } else {
        repos
    };
    let spec = format!("refs/heads/{branch}");
    for root in ctx.roots.of(slug) {
        for rel in &rels {
            let d: PathBuf = local::expand(&root, rel);
            let sha = local::git(&["rev-parse", "--short", "--verify", "--quiet", &spec], &d);
            if let Some(sha) = sha.filter(|s| !s.is_empty()) {
                return Some(sha);
            }
        }
    }
    None
}

pub struct Close<'a> {
    pub id: &'a str,
    pub resolution: Option<&'a String>,
    pub gates: Option<&'a String>,
    pub runner: Option<&'a String>,
    pub model: Option<&'a String>,
    pub force: bool,
}

/// The checkout's own checks first: no close mid-merge, and a sha read from the claimed branch when
/// none is given. Then the close.
///
/// # Errors
/// Inside a job, the repository is mid-merge or mid-rebase, no resolution is found, or the server
/// refuses.
pub fn close(ctx: &mut Ctx, c: &Close) -> Result<i32> {
    refuse_in_job("close")?;
    let slug = ctx.project()?;
    let item = id(c.id)?;
    let v = ctx.read(&format!("/show/{item}"), &[])?;
    let project = ctx.project_row(&slug)?;
    let key = v["key"].as_str().unwrap_or_default();
    let kind = project["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|k| k["key"] == key)
        .and_then(|k| k["kind"].as_str())
        .unwrap_or("work");
    if let Some(busy) = local::unfinished(&ctx.cwd).filter(|_| !c.force) {
        return Err(Fail::refused(format!(
            "This repository is mid-{}, so a sha read or given now may not be the one that lands. Finish or \
             abort {busy} first, then close {item}.",
            &busy[2..]
        )));
    }
    let mut resolution = c.resolution.filter(|r| !r.is_empty()).cloned();
    if resolution.is_none() && kind != "decision" {
        let branch = v["claim_branch"].as_str();
        resolution = claim_sha(ctx, &slug, &project, branch);
        if resolution.is_none() {
            return Err(Fail::refused(format!(
                "close needs a resolution: the sha the work landed as, or what closed it. No branch of {} was \
                 found to read one from.",
                branch.filter(|b| !b.is_empty()).unwrap_or("this item")
            )));
        }
    }
    let req = CloseRequest {
        common: ctx.common(c.force)?,
        id: c.id.to_string(),
        resolution,
        gates: c.gates.filter(|g| !g.is_empty()).cloned(),
        runner: c.runner.filter(|r| !r.is_empty()).cloned(),
        model: c.model.filter(|m| !m.is_empty()).cloned(),
    };
    let out: Closed = ctx.api.post("close", &req)?;
    print_item(ctx, &out.item);
    if !ctx.json {
        print_released(ctx, &out.released);
        if !out.opened.is_empty() {
            println!("opened by {}: {}", out.item.id, out.opened.join(", "));
        }
    }
    Ok(0)
}

/// # Errors
/// Inside a job, or the server refuses.
pub fn drop(ctx: &mut Ctx, req: &DropRequest) -> Result<i32> {
    refuse_in_job("drop")?;
    let out: Dropped = ctx.api.post("drop", req)?;
    print_item(ctx, &out.item);
    print_released(ctx, &out.released);
    Ok(0)
}

/// `reopen`, `wait`, `resume`, `reply`, `decide`, `rate` and `edit`: one row moved.
///
/// # Errors
/// Inside a job for `reopen`, or the server refuses.
pub fn moved(ctx: &mut Ctx, verb: &str, req: &impl serde::Serialize) -> Result<i32> {
    refuse_in_job(verb)?;
    let out: Moved = ctx.api.post(verb, req)?;
    print_item(ctx, &out.item);
    Ok(0)
}

/// # Errors
/// The server refuses.
pub fn ask(ctx: &mut Ctx, req: &AskRequest) -> Result<i32> {
    let out: Asked = ctx.api.post("ask", req)?;
    print_item(ctx, &out.item);
    if !out.twins.is_empty() && !ctx.json {
        println!("\nAlready decided, and close to this one. Read before raising it:");
        for t in &out.twins {
            println!(
                "  {:<6} {}  ({})",
                t.id,
                cut(&t.title, 70),
                cut(&t.resolution, 50)
            );
        }
    }
    Ok(0)
}

/// # Errors
/// The server refuses.
pub fn answer(ctx: &mut Ctx, req: &AnswerRequest) -> Result<i32> {
    let out: Answered = ctx.api.post("answer", req)?;
    print_item(ctx, &out.item);
    if !ctx.json {
        print_released(ctx, &out.released);
        let i = &out.item.id;
        if req.carried_by.is_empty() {
            println!(
                "\n{i} now owes work items: docket start {i}, then docket close {i} \"opened ...\"."
            );
        }
    }
    Ok(0)
}

/// # Errors
/// The server refuses.
pub fn priority(ctx: &mut Ctx, req: &PriorityRequest) -> Result<i32> {
    let out: MovedMany = ctx.api.post("priority", req)?;
    for item in &out.items {
        print_item(ctx, item);
    }
    Ok(0)
}

/// # Errors
/// The body cannot be read, or the server refuses.
pub fn edit(
    ctx: &mut Ctx,
    item: &str,
    set: &[String],
    append: Option<&String>,
    body: Option<&String>,
) -> Result<i32> {
    let common = ctx.common(false)?;
    let append = append.filter(|a| !a.is_empty()).cloned();
    let body = body.filter(|b| !b.is_empty());
    if set.is_empty() && append.is_none() && body.is_none() {
        return Err(Fail::refused(
            "edit needs --set field=value, --append \"text\" or --body FILE|-.",
        ));
    }
    let set = set
        .iter()
        .map(|kv| {
            let (field, value) = kv.split_once('=').unwrap_or((kv, ""));
            SetField {
                field: field.to_string(),
                value: value.to_string(),
            }
        })
        .collect();
    let req = EditRequest {
        common,
        id: item.to_string(),
        set,
        append,
        body: read_body(body)?,
    };
    moved(ctx, "edit", &req)
}

/// # Errors
/// A word is not related or opened, or the server refuses.
pub fn link(ctx: &mut Ctx, words: &[String], remove: bool) -> Result<i32> {
    let (b, rest) = words.split_last().unwrap_or((&words[0], &[]));
    let (kind, a) = rest.split_last().unwrap_or((b, &[]));
    let req = LinkRequest {
        common: ctx.common(false)?,
        a: a.to_vec(),
        kind: kind.clone(),
        b: b.clone(),
        remove,
    };
    let out: Linked = ctx.api.post("link", &req)?;
    for a in &out.items {
        println!(
            "{a} {} {}{}",
            out.kind,
            out.to,
            if out.removed { " removed" } else { "" }
        );
    }
    Ok(0)
}

/// # Errors
/// The server refuses.
pub fn key(ctx: &mut Ctx, req: &KeyRequest) -> Result<i32> {
    let out: KeySet = ctx.api.post("key", req)?;
    println!("{}  {}  {}", out.key, out.kind, out.meaning);
    Ok(0)
}

/// # Errors
/// The server refuses.
pub fn reindex(ctx: &mut Ctx) -> Result<i32> {
    let common: Common = ctx.common(false)?;
    let out: Reindexed = ctx.api.post("reindex", &common)?;
    println!("reindexed {} items in {}", out.items, out.project);
    Ok(0)
}

/// With no slug, the project this directory resolves to; with one, the directory bound to it here.
///
/// # Errors
/// The project is unknown, or the roots file cannot be written.
pub fn bind(ctx: &mut Ctx, slug: Option<&String>, root: Option<&String>) -> Result<i32> {
    let Some(slug) = slug.filter(|s| !s.is_empty()) else {
        let resolved = ctx.project()?;
        println!("{} resolves to {resolved}", ctx.cwd.display());
        return Ok(0);
    };
    ctx.project_row(slug)?;
    let dir = root.map_or_else(|| ctx.cwd.clone(), PathBuf::from);
    let real = local::realpath(Path::new(&dir));
    ctx.bind_root(&real, slug, "bind")?;
    println!("bound {real} to {slug} on {}", ctx.host()?);
    Ok(0)
}
