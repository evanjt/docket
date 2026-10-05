//! What the published ref adds to the other verbs: the publication holding an item, a squash
//! offered when one is due, and the owner's ask to push a recorded publication. Each is read from
//! the checkout the verb runs in and stays out of the way where there is none.

use std::path::{Path, PathBuf};

use serde_json::Value;

use docket_core::api::{AskRequest, CloseRequest, Opened, Publications};
use docket_core::cut::Group;
use docket_core::fact::{Publish, publish};
use docket_core::publication::{
    LANDINGS_BEFORE_SQUASH, Publication, first_containing, push_title, squash_due,
};

use crate::cmd::audit::shas;
use crate::ctx::Ctx;
use crate::dispatch::git;
use crate::fail::Result;

/// A project that publishes, read from the checkout the verb runs in.
struct Published {
    repo: PathBuf,
    target: Publish,
    work_ref: String,
    /// Newest first.
    publications: Vec<Publication>,
}

fn rev(repo: &Path, what: &str) -> Option<String> {
    git(repo, &["rev-parse", "--verify", "--quiet", what]).ok()
}

fn contains(repo: &Path, commit: &str, snapshot: &str) -> bool {
    git(repo, &["merge-base", "--is-ancestor", commit, snapshot]).is_ok()
}

fn open(ctx: &mut Ctx) -> Option<Published> {
    let slug = ctx.project().ok()?;
    let facts = ctx.api.get("/facts", &[("project", slug.clone())]).ok()?;
    let skills = facts["skills"]
        .as_object()?
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect();
    let target = publish(&skills)?;
    let project = ctx.project_row(&slug).ok()?;
    let work_ref = project["integration_ref"]
        .as_str()
        .filter(|r| !r.is_empty())?
        .to_string();
    let cwd = std::env::current_dir().ok()?;
    let repo = PathBuf::from(git(&cwd, &["rev-parse", "--show-toplevel"]).ok()?);
    let listed = ctx.api.get("/publications", &[("project", slug)]).ok()?;
    let listed: Publications = serde_json::from_value(listed).ok()?;
    Some(Published {
        repo,
        target,
        work_ref,
        publications: listed.publications,
    })
}

/// `published: SHA  DATE`: the publication whose work snapshot is the first to contain the item's
/// close sha.
#[must_use]
pub fn show_line(ctx: &mut Ctx, resolution: &str) -> Option<String> {
    let p = open(ctx)?;
    let closes: Vec<String> = shas(resolution)
        .iter()
        .filter_map(|s| rev(&p.repo, &format!("{s}^{{commit}}")))
        .collect();
    let found = closes
        .iter()
        .filter_map(|c| first_containing(&p.publications, |w| contains(&p.repo, c, w)))
        .min_by_key(|f| {
            p.publications
                .iter()
                .rev()
                .position(|q| q.published == f.published)
        })?;
    Some(format!(
        "published: {}  {}",
        found.published, found.created_at
    ))
}

/// The line offering a squash when one is due: a plan closed that no publication covers, or the
/// work ref well past the last one.
#[must_use]
pub fn offer(ctx: &mut Ctx) -> Option<String> {
    let p = open(ctx)?;
    let tip = rev(&p.repo, &format!("{}^{{commit}}", p.work_ref))?;
    let from = match p.publications.first() {
        Some(last) => last.work.clone(),
        None => rev(&p.repo, &format!("{}^{{commit}}", p.target.remote))?,
    };
    let landings: usize = git(
        &p.repo,
        &[
            "rev-list",
            "--first-parent",
            "--count",
            &format!("{from}..{tip}"),
        ],
    )
    .ok()?
    .parse()
    .ok()?;
    let slug = ctx.project().ok()?;
    let filter = serde_json::json!({ "project": slug, "state": "done" }).to_string();
    let rows = ctx
        .api
        .get(
            "/items",
            &[("filter", filter), ("range", "[0,99999]".into())],
        )
        .ok()?;
    let covered: Vec<&String> = p.publications.iter().flat_map(|q| &q.plans).collect();
    let unpublished: Vec<String> = rows
        .as_array()?
        .iter()
        .filter(|r| r["item_type"] == "plan")
        .filter_map(|r| r["id"].as_str())
        .filter(|id| !covered.iter().any(|c| c.as_str() == *id))
        .map(str::to_string)
        .collect();
    let why = squash_due(&unpublished, landings, LANDINGS_BEFORE_SQUASH)?;
    Some(format!("Squash: {why}: docket squash"))
}

fn open_ask(ctx: &mut Ctx, title: &str) -> Result<Option<String>> {
    let rows = ctx.read("/todo", &[])?;
    Ok(rows
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["title"] == title)
        .and_then(|r| r["id"].as_str().map(str::to_string)))
}

/// Asks the owner to push the published ref, unless that ask is open already. It is filed in the
/// area of the first of `plans` that has one. The ask carries every push command, each
/// submodule's before the parent's.
pub fn ask_push(ctx: &mut Ctx, target: &Publish, groups: &[Group], pushes: &[String]) {
    if let Err(why) = file_push(ctx, target, groups, pushes) {
        eprintln!("the push ask was not filed: {why}");
    }
}

fn file_push(ctx: &mut Ctx, target: &Publish, groups: &[Group], pushes: &[String]) -> Result<()> {
    let plans: Vec<&String> = groups
        .iter()
        .flat_map(|g| g.finished.iter().chain(&g.partial))
        .collect();
    let title = push_title(&target.local, &target.remote);
    if open_ask(ctx, &title)?.is_some() {
        return Ok(());
    }
    let mut area = None;
    for plan in plans {
        let shown = ctx.read(&format!("/show/{plan}"), &[])?;
        if let Some(a) = shown["area"].as_str().filter(|a| !a.is_empty()) {
            area = Some(a.to_string());
            break;
        }
    }
    let command = pushes.join("\n");
    let mut req = docket_core::api::NewRequest {
        common: ctx.common(false)?,
        key: "T".into(),
        title,
        body: Some(command.clone()),
        release: Some("current".into()),
        area,
        ..Default::default()
    };
    // A project with no release open files it in the backlog.
    let opened: Opened = if let Ok(opened) = ctx.api.post("new", &req) {
        opened
    } else {
        req.release = None;
        ctx.api.post("new", &req)?
    };
    let req = AskRequest {
        common: ctx.common(false)?,
        id: opened.item.id,
        note: command,
        need: Some("act".into()),
    };
    let _: Value = ctx.api.post("ask", &req)?;
    Ok(())
}

/// Whether each submodule the published tip pins has its published ref on its remote ref. A
/// submodule with no published ref of its own is public as it stands.
fn submodules_pushed(p: &Published, tip: &str) -> bool {
    let Ok(tree) = git(&p.repo, &["ls-tree", "-r", tip]) else {
        return true;
    };
    let local = format!("refs/heads/{}", p.target.local);
    let remote = format!("refs/remotes/{}", p.target.remote);
    tree.lines()
        .filter(|l| l.starts_with("160000 "))
        .filter_map(|l| l.split_once('\t').map(|(_, path)| path))
        .all(|path| {
            let dir = p.repo.join(path);
            let Some(inner) = rev(&dir, &local) else {
                return true;
            };
            rev(&dir, &remote).is_some() && contains(&dir, &inner, &remote)
        })
}

/// Closes the push ask once the remote ref holds the last publication.
pub fn settle_push(ctx: &mut Ctx) {
    let Some(p) = open(ctx) else { return };
    let Some(last) = p.publications.first() else {
        return;
    };
    let remote = format!("refs/remotes/{}", p.target.remote);
    if rev(&p.repo, &remote).is_none() || !contains(&p.repo, &last.published, &remote) {
        return;
    }
    if !submodules_pushed(&p, &last.published) {
        return;
    }
    let title = push_title(&p.target.local, &p.target.remote);
    let Ok(Some(id)) = open_ask(ctx, &title) else {
        return;
    };
    let Ok(common) = ctx.common(false) else {
        return;
    };
    let req = CloseRequest {
        common,
        id,
        resolution: Some(format!("{} is on {}", last.published, p.target.remote)),
        ..Default::default()
    };
    let _: Result<Value> = ctx.api.post("close", &req);
}
