//! `docket squash`: the published ref built from the work ref's landing snapshots. The work ref is
//! only read, nothing is pushed, and the published ref moves only after every message and every
//! added line has passed the private guard and the last tree equals the work tip's. Each submodule
//! the project names among its repos is published first, and the published trees pin its published
//! commits.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use docket_core::api::{PublicationRequest, Publications};
use docket_core::cut::{Group, Landed, Snapshot, cut};
use docket_core::fact::{Publish, publish};

use crate::cmd::audit::shas;
use crate::cmd::private::{Look, read};
use crate::ctx::Ctx;
use crate::dispatch::{git, index_git, scratch};
use crate::fail::{Fail, Result};

/// Where a squash starts: the commit its first group is parented on, and the work commit whose
/// landings are already published.
struct Start {
    parent: String,
    work: String,
}

/// A submodule the project names among its repos, and what publishing it takes.
struct Inner {
    /// Its path inside the checkout.
    path: String,
    dir: PathBuf,
    /// The published ref here as it stood before the squash, empty when it did not exist.
    was: String,
    /// The commit its next published commit is parented on.
    base: String,
    /// The published commit carrying each tree.
    by_tree: HashMap<String, String>,
    /// The pinned commits that need a published commit of their own, oldest first, each with the
    /// group whose message it takes.
    new: Vec<(String, usize)>,
    /// Each group's pinned commit and, unless it is already public, its tree.
    pins: Vec<Option<(String, Option<String>)>>,
}

/// What the work ref's snapshots since the start landed.
struct Landings {
    snapshots: Vec<Snapshot>,
    /// Each item landed, by snapshot, with its title.
    titles: BTreeMap<String, String>,
    open: BTreeMap<String, usize>,
}

/// # Errors
/// The project does not publish, the checkout or the server cannot be read, a message or an added
/// line fails the guard, the trees do not match, or git refuses.
pub fn squash(ctx: &mut Ctx, messages: Option<&str>, cap: Option<usize>) -> Result<i32> {
    let slug = ctx.project()?;
    let project = ctx.project_row(&slug)?;
    let facts = ctx.api.get("/facts", &[("project", slug.clone())])?;
    let skills: BTreeMap<String, String> = facts["skills"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect();
    let Some(target) = publish(&skills) else {
        return Err(Fail::refused(
            "this project does not squash: docket skills set publish \"published origin/main\"",
        ));
    };
    let Some(work_ref) = project["integration_ref"]
        .as_str()
        .filter(|r| !r.is_empty())
    else {
        return Err(Fail::refused("the project stores no integration_ref"));
    };
    let cwd = std::env::current_dir().map_err(|e| Fail::refused(e.to_string()))?;
    let repo = PathBuf::from(
        git(&cwd, &["rev-parse", "--show-toplevel"])
            .map_err(|_| Fail::refused("docket squash runs inside a git checkout"))?,
    );
    let tip = rev(&repo, &format!("{work_ref}^{{commit}}"))
        .ok_or_else(|| Fail::refused(format!("{work_ref} is no ref in this checkout")))?;
    let listed: Publications =
        serde_json::from_value(ctx.api.get("/publications", &[("project", slug.clone())])?)
            .map_err(|e| Fail::refused(format!("/publications: {e}")))?;
    let start = start_of(&repo, &target, &tip, listed.publications.first())?;
    let landings = landings(ctx, &slug, &repo, &start.work, &tip)?;
    let groups = cut(&landings.snapshots, &landings.open, cap);
    if groups.is_empty() {
        println!("nothing to publish: {work_ref} has no landing past the last publication");
        return Ok(0);
    }
    let Some(file) = messages else {
        print_proposals(&landings, &groups);
        return Ok(0);
    };
    let lines = message_lines(file, groups.len())?;
    let look = read(ctx)?;
    let repos: Vec<String> = project["repos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r.as_str().map(str::to_string))
        .collect();
    let checkout = skills.get("checkout").map_or(".", String::as_str);
    let mut inners = Vec::new();
    for path in submodule_paths(&repos, checkout) {
        inners.extend(inner(&repo, &target, &path, &groups)?);
    }
    guard(&look, &repo, &start, &groups, &lines)?;
    guard_inners(&look, &inners)?;
    let built = tree_of(&repo, &groups[groups.len() - 1].end)?;
    tips_match(&built, &tree_of(&repo, &tip)?)?;
    let local = format!("refs/heads/{}", target.local);
    for i in &mut inners {
        publish_inner(i, &local, &lines)?;
    }
    let mut parent = start.parent.clone();
    let mut made = Vec::new();
    for (n, (g, line)) in groups.iter().zip(&lines).enumerate() {
        let tree = relink(&repo, &g.end, &inners, n)?;
        parent = commit(&repo, &tree, &parent, line, &g.date)?;
        made.push(parent.clone());
    }
    let was = rev(&repo, &local).unwrap_or_default();
    git(&repo, &["update-ref", &local, &parent, &was]).map_err(Fail::refused)?;
    for (g, sha) in groups.iter().zip(&made) {
        let plans: Vec<String> = g.finished.iter().chain(&g.partial).cloned().collect();
        let req = PublicationRequest {
            common: ctx.common(false)?,
            published: sha.clone(),
            work: g.end.clone(),
            plans,
        };
        let _: Publications = ctx.api.post("publication", &req)?;
    }
    let pushes = print_pushes(&target, &inners);
    crate::cmd::published::ask_push(ctx, &target, &groups, &pushes);
    println!("published {} commits on {}", made.len(), target.local);
    println!("{}", pushes[pushes.len() - 1]);
    Ok(0)
}

/// Prints what each submodule published and the push commands, the parent's last, and returns
/// the commands.
fn print_pushes(target: &Publish, inners: &[Inner]) -> Vec<String> {
    let (remote, branch) = target
        .remote
        .split_once('/')
        .unwrap_or((&target.remote, ""));
    let mut pushes = Vec::new();
    for i in inners.iter().filter(|i| !i.new.is_empty()) {
        println!(
            "published {} commits on {} in {}",
            i.new.len(),
            target.local,
            i.path
        );
        let line = format!("git -C {} push {remote} {}:{branch}", i.path, target.local);
        println!("{line}");
        pushes.push(line);
    }
    pushes.push(format!("git push {remote} {}:{branch}", target.local));
    pushes
}

fn rev(repo: &Path, what: &str) -> Option<String> {
    git(repo, &["rev-parse", "--verify", "--quiet", what])
        .ok()
        .filter(|s| !s.is_empty())
}

fn tree_of(repo: &Path, commit: &str) -> Result<String> {
    git(repo, &["rev-parse", &format!("{commit}^{{tree}}")]).map_err(Fail::refused)
}

/// Refuses a published tree that is not the work tip's.
fn tips_match(built: &str, tip: &str) -> Result<()> {
    if built == tip {
        return Ok(());
    }
    Err(Fail::refused(format!(
        "the last published tree {built} is not the work tip's tree {tip}: the work ref moved or was rewritten, nothing written"
    )))
}

/// Where the next squash starts: on the published ref's tip when a publication names it, else on
/// the remote ref the first time.
fn start_of(
    repo: &Path,
    target: &Publish,
    tip: &str,
    last: Option<&docket_core::publication::Publication>,
) -> Result<Start> {
    let local = format!("refs/heads/{}", target.local);
    let start = if let Some(published) = rev(repo, &local) {
        match last {
            Some(l) if l.published == published => Start {
                parent: published,
                work: l.work.clone(),
            },
            _ => {
                return Err(Fail::refused(format!(
                    "{} is at {published}, which no recorded publication names: nothing written",
                    target.local
                )));
            }
        }
    } else {
        let remote = rev(repo, &format!("refs/remotes/{}", target.remote)).ok_or_else(|| {
            Fail::refused(format!(
                "neither {} nor {} exists here to build on: fetch the remote first",
                target.local, target.remote
            ))
        })?;
        let work = git(repo, &["merge-base", &remote, tip]).map_err(|_| {
            Fail::refused(format!(
                "{} shares no history with the work ref",
                target.remote
            ))
        })?;
        Start {
            parent: remote,
            work,
        }
    };
    if git(repo, &["merge-base", "--is-ancestor", &start.work, tip]).is_err() {
        return Err(Fail::refused(format!(
            "the work commit {} the last publication covers is no longer on the work ref: it was rewritten, nothing written",
            start.work
        )));
    }
    Ok(start)
}

/// The first-parent snapshots after `from`, each with the items whose close sha it brought in.
fn landings(ctx: &mut Ctx, slug: &str, repo: &Path, from: &str, tip: &str) -> Result<Landings> {
    let range = format!("{from}..{tip}");
    let listed = git(
        repo,
        &[
            "rev-list",
            "--first-parent",
            "--reverse",
            "--format=%H %cI",
            &range,
        ],
    )
    .map_err(Fail::refused)?;
    let mut snapshots: Vec<Snapshot> = Vec::new();
    for line in listed.lines().filter(|l| !l.starts_with("commit ")) {
        let (sha, date) = line.split_once(' ').unwrap_or((line, ""));
        snapshots.push(Snapshot {
            sha: sha.to_string(),
            date: date.to_string(),
            items: Vec::new(),
        });
    }
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut before = from.to_string();
    for (i, s) in snapshots.iter().enumerate() {
        let brought =
            git(repo, &["rev-list", &format!("{before}..{}", s.sha)]).map_err(Fail::refused)?;
        at.extend(brought.lines().map(|l| (l.to_string(), i)));
        before.clone_from(&s.sha);
    }
    let filter = serde_json::json!({ "project": slug }).to_string();
    let rows = ctx.api.get(
        "/items",
        &[("filter", filter), ("range", "[0,99999]".to_string())],
    )?;
    let rows: &[Value] = rows.as_array().map_or(&[], Vec::as_slice);
    let plan_of: HashMap<i64, &str> = rows
        .iter()
        .filter(|r| r["item_type"] == "plan")
        .filter_map(|r| Some((r["rid"].as_i64()?, r["id"].as_str()?)))
        .collect();
    let mut open: BTreeMap<String, usize> = BTreeMap::new();
    let mut titles = BTreeMap::new();
    for r in rows {
        let plan = r["parent_rid"]
            .as_i64()
            .and_then(|p| plan_of.get(&p))
            .map(|p| (*p).to_string());
        let (Some(id), Some(state)) = (r["id"].as_str(), r["state"].as_str()) else {
            continue;
        };
        if let Some(plan) = &plan {
            let n = open.entry(plan.clone()).or_default();
            if state != "done" && state != "dropped" {
                *n += 1;
            }
        }
        if state != "done" {
            continue;
        }
        let landed: BTreeSet<usize> = shas(r["resolution"].as_str().unwrap_or_default())
            .iter()
            .filter_map(|s| rev(repo, &format!("{s}^{{commit}}")))
            .filter_map(|full| at.get(&full).copied())
            .collect();
        if let Some(&i) = landed.iter().next() {
            snapshots[i].items.push(Landed {
                item: id.to_string(),
                plan,
            });
            titles.insert(
                id.to_string(),
                r["title"].as_str().unwrap_or_default().to_string(),
            );
        }
    }
    Ok(Landings {
        snapshots,
        titles,
        open,
    })
}

/// The snapshots each group spans: from the one after the previous group's end to its own.
fn spans<'a>(l: &'a Landings, groups: &[Group]) -> Vec<&'a [Snapshot]> {
    let mut from = 0;
    groups
        .iter()
        .map(|g| {
            let to = l.snapshots.iter().position(|s| s.sha == g.end).unwrap_or(0) + 1;
            let span = &l.snapshots[from..to];
            from = to;
            span
        })
        .collect()
}

fn print_proposals(l: &Landings, groups: &[Group]) {
    for (g, span) in groups.iter().zip(spans(l, groups)) {
        println!("{}  {}", g.end, g.date);
        if !g.finished.is_empty() {
            println!("  plans finished: {}", g.finished.join(", "));
        }
        if !g.partial.is_empty() {
            println!("  plans in part:  {}", g.partial.join(", "));
        }
        for item in span.iter().flat_map(|s| &s.items) {
            let title = l.titles.get(&item.item).map_or("", String::as_str);
            println!("    {}  {title}", item.item);
        }
    }
    println!(
        "{} commit{}: docket squash --messages FILE, one line each, oldest first",
        groups.len(),
        if groups.len() == 1 { "" } else { "s" }
    );
}

/// The non-blank lines of the messages file, one per group.
fn message_lines(file: &str, groups: usize) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(file).map_err(|e| Fail::refused(format!("{file}: {e}")))?;
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if lines.len() != groups {
        return Err(Fail::refused(format!(
            "{file} has {} message line{}, and the squash has {groups} commit{}: nothing written",
            lines.len(),
            if lines.len() == 1 { "" } else { "s" },
            if groups == 1 { "" } else { "s" }
        )));
    }
    Ok(lines)
}

/// Runs the private guard over every message and every diff the published commits would carry.
fn guard(
    look: &Look,
    repo: &Path,
    start: &Start,
    groups: &[Group],
    lines: &[String],
) -> Result<()> {
    let mut found: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        found.extend(look.message_hits(&format!("message {}", i + 1), line));
    }
    let mut before = start.parent.clone();
    for (i, g) in groups.iter().enumerate() {
        let diff = git(
            repo,
            &[
                "diff",
                "--no-color",
                "--no-ext-diff",
                "-U0",
                &before,
                &g.end,
            ],
        )
        .map_err(Fail::refused)?;
        found.extend(
            look.diff_hits(&diff)
                .into_iter()
                .map(|h| format!("commit {}: {h}", i + 1)),
        );
        before.clone_from(&g.end);
    }
    if found.is_empty() {
        return Ok(());
    }
    Err(Fail::refused(format!(
        "the private guard refuses the squash, nothing written:\n{}",
        found.join("\n")
    )))
}

/// The paths inside the checkout of the project's repos that sit under it. A repo is listed
/// relative to the root and the checkout is a path under the root, so a repo's path here is its
/// own less the checkout's. The checkout itself and a repo outside it are none.
fn submodule_paths(repos: &[String], checkout: &str) -> Vec<String> {
    let clean = |p: &str| {
        let p = p.trim_end_matches('/');
        p.strip_prefix("./").unwrap_or(p).to_string()
    };
    let checkout = clean(checkout);
    repos
        .iter()
        .filter(|r| !r.starts_with('~') && !Path::new(r.as_str()).is_absolute())
        .filter_map(|r| {
            let r = clean(r);
            let inside = if checkout.is_empty() || checkout == "." {
                Some(r.as_str())
            } else {
                r.strip_prefix(&format!("{checkout}/"))
            };
            inside
                .filter(|p| !p.is_empty() && *p != ".")
                .map(str::to_string)
        })
        .collect()
}

/// The commit a snapshot pins at `path`, `None` when it has no submodule there.
fn pin_at(repo: &Path, snapshot: &str, path: &str) -> Result<Option<String>> {
    let links = crate::job::gitlinks(repo, &["ls-tree", "-z", snapshot, "--", path], 2)
        .map_err(Fail::refused)?;
    Ok(links
        .into_iter()
        .find(|(p, _)| p == path)
        .map(|(_, sha)| sha))
}

/// What publishing the submodule at `path` takes, `None` when no group end pins one there. A
/// commit already on its remote ref is public as it stands; one whose tree a published commit
/// carries maps to that commit; every other one pinned at a group end gets a published commit of
/// its own. A pinned commit this repository lacks has no published counterpart and is refused.
fn inner(repo: &Path, target: &Publish, path: &str, groups: &[Group]) -> Result<Option<Inner>> {
    let mut at = Vec::new();
    for g in groups {
        at.push(pin_at(repo, &g.end, path)?);
    }
    if at.iter().all(Option::is_none) {
        return Ok(None);
    }
    let dir = repo.join(path);
    if !crate::job::is_repository(&dir) {
        return Err(Fail::refused(format!(
            "the submodule {path} is not checked out here: git submodule update --init, then squash again"
        )));
    }
    let local = format!("refs/heads/{}", target.local);
    let remote = rev(&dir, &format!("refs/remotes/{}", target.remote));
    let was = rev(&dir, &local);
    let Some(base) = was.clone().or_else(|| remote.clone()) else {
        return Err(Fail::refused(format!(
            "neither {} nor {} exists in the submodule {path} to build on: fetch its remote first",
            target.local, target.remote
        )));
    };
    let mut by_tree = HashMap::new();
    let mut walk = vec!["log", "--first-parent", "--format=%H %T", base.as_str()];
    let not_remote = remote.as_ref().map(|r| format!("^{r}"));
    walk.extend(not_remote.as_deref());
    for line in git(&dir, &walk).map_err(Fail::refused)?.lines() {
        if let Some((sha, tree)) = line.split_once(' ') {
            by_tree
                .entry(tree.to_string())
                .or_insert_with(|| sha.to_string());
        }
    }
    let mut new = Vec::new();
    let mut planned = BTreeSet::new();
    let mut pins = Vec::new();
    for (n, (g, pin)) in groups.iter().zip(at).enumerate() {
        let Some(pin) = pin else {
            pins.push(None);
            continue;
        };
        if git(&dir, &["merge-base", "--is-ancestor", &pin, &base]).is_ok() {
            pins.push(Some((pin, None)));
            continue;
        }
        let tree = tree_of(&dir, &pin).map_err(|_| {
            Fail::refused(format!(
                "{} pins the submodule {path} at {pin}, which has no published counterpart: {} holds no such commit, nothing written",
                g.end,
                dir.display()
            ))
        })?;
        if !by_tree.contains_key(&tree) && planned.insert(tree.clone()) {
            new.push((pin.clone(), n));
        }
        pins.push(Some((pin, Some(tree))));
    }
    Ok(Some(Inner {
        path: path.to_string(),
        dir,
        was: was.unwrap_or_default(),
        base,
        by_tree,
        new,
        pins,
    }))
}

/// Runs the private guard over every diff the submodules' published commits would carry.
fn guard_inners(look: &Look, inners: &[Inner]) -> Result<()> {
    let mut found: Vec<String> = Vec::new();
    for i in inners {
        let mut before = i.base.clone();
        for (n, (pin, _)) in i.new.iter().enumerate() {
            let diff = git(
                &i.dir,
                &["diff", "--no-color", "--no-ext-diff", "-U0", &before, pin],
            )
            .map_err(Fail::refused)?;
            found.extend(
                look.diff_hits(&diff)
                    .into_iter()
                    .map(|h| format!("{} commit {}: {h}", i.path, n + 1)),
            );
            before.clone_from(pin);
        }
    }
    if found.is_empty() {
        return Ok(());
    }
    Err(Fail::refused(format!(
        "the private guard refuses the squash, nothing written:\n{}",
        found.join("\n")
    )))
}

/// The submodule's published commits made, each on its pinned commit's tree, dated as that commit
/// and carrying the message of the group that first pins it, and its published ref moved to them.
fn publish_inner(i: &mut Inner, local: &str, lines: &[String]) -> Result<()> {
    let mut parent = i.base.clone();
    for (pin, n) in &i.new {
        let date = git(&i.dir, &["log", "-1", "--format=%cI", pin]).map_err(Fail::refused)?;
        let tree = tree_of(&i.dir, pin)?;
        parent = commit(&i.dir, &tree, &parent, &lines[*n], &date)?;
        i.by_tree.insert(tree, parent.clone());
    }
    if !i.new.is_empty() {
        git(&i.dir, &["update-ref", local, &parent, &i.was]).map_err(Fail::refused)?;
    }
    Ok(())
}

/// The tree group `n` publishes: its snapshot's, with each submodule's gitlink pointing at the
/// published commit of the commit it pins.
fn relink(repo: &Path, snapshot: &str, inners: &[Inner], n: usize) -> Result<String> {
    let links: Vec<(&str, &str)> = inners
        .iter()
        .filter_map(|i| match &i.pins[n] {
            Some((_, Some(tree))) => Some((i.path.as_str(), i.by_tree[tree].as_str())),
            _ => None,
        })
        .collect();
    if links.is_empty() {
        return tree_of(repo, snapshot);
    }
    let index = scratch("index");
    let at = Some(index.as_path());
    let made = (|| {
        index_git(repo, at, &["read-tree", snapshot])?;
        for (path, sha) in &links {
            let info = format!("160000,{sha},{path}");
            index_git(repo, at, &["update-index", "--cacheinfo", &info])?;
        }
        index_git(repo, at, &["write-tree"])
    })();
    let _ = std::fs::remove_file(&index);
    made.map_err(Fail::refused)
}

/// A commit on `tree` over `parent`, dated `date`, by the repository's configured user.
fn commit(repo: &Path, tree: &str, parent: &str, message: &str, date: &str) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["commit-tree", tree, "-p", parent, "-m", message])
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .output()
        .map_err(|e| Fail::refused(format!("git: {e}")))?;
    if !out.status.success() {
        return Err(Fail::refused(format!(
            "git commit-tree: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
#[path = "../tests/squash.rs"]
mod tests;
