//! The write verbs: each one request to the server, its answer printed.

use std::path::{Path, PathBuf};

use serde_json::Value;

use docket_core::api::{
    AddRequest, AnswerRequest, Answered, AskRequest, Asked, Brief, CloseRequest, Closed, Common,
    DropRequest, Dropped, EditRequest, ItemView, LinkRequest, Linked, Moved, MovedMany, NewRequest,
    Opened, ParentRequest, Parented, PriorityRequest, Reindexed, ReleaseRequest, SetField,
    StartRequest, Started,
};
use docket_core::word::ItemType;

use crate::cmd::lists::shares_text;
use crate::ctx::{Ctx, id};
use crate::fail::{Fail, Result};
use crate::jsonout::cut;
use crate::local;
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
        println!("unblocked {}: {}", x.id, cut(&x.title, 70));
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

/// A job writes nothing outside its worktree, so it installs no instructions or skills.
///
/// # Errors
/// `DOCKET_JOB` names the job this runs in, and `what` is `install`.
pub fn refuse_install_in_job(what: &str) -> Result<()> {
    let job = std::env::var("DOCKET_JOB").ok();
    match crate::job::install_refusal(what, job.as_deref()) {
        Some(why) => Err(Fail::refused(why)),
        None => Ok(()),
    }
}

/// A job names the release and the area or plan of every item it files.
///
/// # Errors
/// `DOCKET_JOB` names the job this runs in, and no release, or neither an area nor a plan, is given.
fn refuse_unfiled(
    verb: &str,
    release: Option<&String>,
    area: Option<&String>,
    parent: Option<&String>,
) -> Result<()> {
    let job = std::env::var("DOCKET_JOB").ok();
    let name = String::as_str;
    match crate::job::unfiled(
        verb,
        job.as_deref(),
        release.map(name),
        area.map(name),
        parent.map(name),
    ) {
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
    pub repo: Option<&'a String>,
    pub area: Option<&'a String>,
    pub parent: Option<&'a String>,
}

/// # Errors
/// The server refuses.
pub fn new(ctx: &mut Ctx, n: &New) -> Result<i32> {
    refuse_unfiled("new", n.release, n.area, n.parent)?;
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
        repo: n.repo.cloned(),
        area: n.area.cloned(),
        parent: n.parent.cloned(),
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
                    "\nThen give it its members: docket parent B1 B2 {}",
                    out.item.id
                );
            }
            _ => {}
        }
    }
}

pub struct Add<'a> {
    pub title: &'a str,
    pub key: Option<&'a String>,
    pub body: Option<&'a String>,
    pub from: Option<&'a String>,
    pub release: Option<&'a String>,
    pub area: Option<&'a String>,
    pub parent: Option<&'a String>,
}

/// # Errors
/// The server refuses.
pub fn add(ctx: &mut Ctx, a: &Add) -> Result<i32> {
    let (key, body, from, release, area, parent) =
        (a.key, a.body, a.from, a.release, a.area, a.parent);
    let title = a.title;
    refuse_unfiled("add", release, area, parent)?;
    let common = ctx.common(false)?;
    let body = read_body(body)?;
    let req = AddRequest {
        common,
        title: title.to_string(),
        key: key.filter(|k| !k.is_empty()).cloned(),
        body: body.clone(),
        from: from.filter(|f| !f.is_empty()).cloned(),
        release: release.filter(|r| !r.is_empty()).cloned(),
        area: area.filter(|a| !a.is_empty()).cloned(),
        parent: parent.filter(|p| !p.is_empty()).cloned(),
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
            "\nThis is a decided question: turn its decision into items, link each with docket link B1 origin {i}, then docket close {i} \"opened B1, B2\"."
        ),
        "audit" => println!(
            "\nThis is a plan. With nothing under it yet, file its tickets, put each under it with docket parent \
             T1 {i}, and unclaim it; it comes back for its audit when they are all closed. With every ticket \
             closed, this is its audit: check the plan in its body against the working tree and against every \
             item under it (docket deps {i}), file each gap as a ticket in the plan's release with docket link \
             T1 origin {i} and no parent, and close it. One round: the gaps are worked as tickets."
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
    refuse_in_job("unclaim")?;
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
    let is_question = v["type"].as_str() == Some(ItemType::Question.as_str());
    if let Some(busy) = local::unfinished(&ctx.cwd).filter(|_| !c.force) {
        return Err(Fail::refused(format!(
            "This repository is mid-{}, so a sha read or given now may not be the one that lands. Finish or \
             abort {busy} first, then close {item}.",
            &busy[2..]
        )));
    }
    let mut resolution = c.resolution.filter(|r| !r.is_empty()).cloned();
    if resolution.is_none() && !is_question {
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

/// What `docket edit` was asked to change.
pub struct Edit<'a> {
    pub id: &'a str,
    pub set: &'a [String],
    pub release: Option<&'a String>,
    pub carry: bool,
    pub area: Option<&'a String>,
    pub append: Option<&'a String>,
    pub body: Option<&'a String>,
}

/// # Errors
/// The body cannot be read, or the server refuses.
pub fn edit(ctx: &mut Ctx, e: &Edit) -> Result<i32> {
    let common = ctx.common(false)?;
    let append = e.append.filter(|a| !a.is_empty()).cloned();
    let body = e.body.filter(|b| !b.is_empty());
    if e.set.is_empty()
        && append.is_none()
        && body.is_none()
        && e.release.is_none()
        && e.area.is_none()
    {
        return Err(Fail::refused(
            "edit needs --set field=value, --release NAME, --area NAME, --append \"text\" or --body FILE|-.",
        ));
    }
    let set = e
        .set
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
        id: e.id.to_string(),
        set,
        append,
        body: read_body(body)?,
        expect_updated_at: None,
        release: e.release.cloned(),
        carry: e.carry,
        area: e.area.cloned(),
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
/// The server refuses, or a plan is named with --none or missing without it.
pub fn parent(ctx: &mut Ctx, words: &[String], none: bool) -> Result<i32> {
    let (a, plan) = match (none, words.split_last()) {
        (true, _) => (words.to_vec(), None),
        (false, Some((plan, a))) if !a.is_empty() => (a.to_vec(), Some(plan.clone())),
        _ => {
            return Err(Fail::refused(
                "docket parent takes the items then the plan, as docket parent T1 T2 A3, or the items alone with --none.",
            ));
        }
    };
    let req = ParentRequest {
        common: ctx.common(false)?,
        a,
        plan,
    };
    let out: Parented = ctx.api.post("parent", &req)?;
    for a in &out.items {
        match &out.plan {
            Some(p) => println!("{a} under {p}"),
            None => println!("{a} under no plan"),
        }
    }
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
