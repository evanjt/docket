//! `docket skills`: the facts a project's skills read, one of them, or one set.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use docket_core::fact::{self, COMPUTED, FACTS};

use crate::ctx::Ctx;
use crate::fail::{Fail, Result};
use crate::templates::{self, Action, Step};

/// The two user-level skill directories, by tool.
const TOOLS: [(&str, &str); 2] = [
    ("claude", "~/.claude/skills"),
    ("codex", "~/.agents/skills"),
];

/// The first word `docket skills` takes: a sub-verb, a fact or a computed name.
#[must_use]
pub fn words() -> Vec<&'static str> {
    let mut out = vec!["show", "get", "set", "install", "diff"];
    out.extend(FACTS.iter().map(|(k, _)| *k));
    out.extend(COMPUTED.iter().map(|(k, _)| *k));
    out
}

/// What the three positionals ask for, `docket skills owner "Ana"` read as `set owner "Ana"` and
/// `docket skills owner` or `docket skills root` as `get`.
#[must_use]
pub fn read_args(
    what: Option<&str>,
    key: Option<&str>,
    value: Option<&str>,
) -> (String, Option<String>, Option<String>) {
    let what = what.unwrap_or("show");
    let own = |s: Option<&str>| s.map(str::to_string);
    if fact::meaning(what).is_some() {
        return match key {
            Some(v) => ("set".into(), Some(what.into()), Some(v.into())),
            None => ("get".into(), Some(what.into()), None),
        };
    }
    if COMPUTED.iter().any(|(k, _)| *k == what) {
        return ("get".into(), Some(what.into()), own(value));
    }
    (what.into(), own(key), own(value))
}

/// Which tools an install writes for: the one named, or both.
#[derive(Clone, Copy, Debug, Default)]
pub struct Install {
    pub claude: bool,
    pub codex: bool,
    pub yes: bool,
}

impl Install {
    fn tools(self) -> Vec<&'static str> {
        match (self.claude, self.codex) {
            (true, false) => vec!["claude"],
            (false, true) => vec!["codex"],
            _ => vec!["claude", "codex"],
        }
    }
}

/// # Errors
/// The project cannot be resolved, a fact is unset or refused, or the server refuses.
pub fn skills(
    ctx: &mut Ctx,
    what: Option<&String>,
    key: Option<&String>,
    value: Option<&String>,
    install: Install,
    all_projects: bool,
) -> Result<i32> {
    let (what, key, value) = read_args(
        what.map(String::as_str),
        key.map(String::as_str),
        value.map(String::as_str),
    );
    match what.as_str() {
        "install" | "diff" => {
            crate::cmd::write::refuse_install_in_job(&what)?;
            let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
            let steps = templates::skill_steps(&home, &install.tools());
            if what == "diff" {
                list(&steps, &home.display().to_string());
                return Ok(i32::from(changes(&steps)));
            }
            write_steps(&steps, &home.display().to_string(), install.yes)
        }
        "get" => get(ctx, key.as_deref()),
        "set" => set(
            ctx,
            key.as_deref(),
            value.as_deref().unwrap_or_default(),
            all_projects,
        ),
        _ => show(ctx),
    }
}

fn all_facts() -> String {
    FACTS.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")
}

/// The project's shortest root on this machine.
pub fn root_of(ctx: &Ctx, slug: &str) -> Option<String> {
    ctx.roots.of(slug).into_iter().next()
}

/// One value a script reads; an unset one is a refusal naming what to set.
fn get(ctx: &mut Ctx, key: Option<&str>) -> Result<i32> {
    let slug = ctx.project()?;
    let Some(key) = key else {
        return Err(Fail::refused(format!(
            "docket skills get KEY; KEY one of {}",
            all_facts()
        )));
    };
    let computed = match key {
        "root" => Some(root_of(ctx, &slug)),
        "slug" => Some(Some(slug.clone())),
        "src" => Some(own_dir()),
        _ => None,
    };
    if let Some(found) = computed {
        let Some(found) = found else {
            return Err(Fail::refused(format!(
                "{slug} has no root on {}: run docket from its checkout once, or docket bind",
                ctx.host()?
            )));
        };
        println!("{found}");
        return Ok(0);
    }
    if fact::RETIRED.contains(&key) {
        return Err(Fail::refused(format!(
            "{key} was a fact of the old loop and is no longer read"
        )));
    }
    let Some(meaning) = fact::meaning(key) else {
        return Err(Fail::refused(format!(
            "{key} is not a skill fact. One of: {}",
            all_facts()
        )));
    };
    let facts = ctx.api.facts(&slug)?;
    let value = fact::layered(&facts.skills, &facts.owner, key).map(|(v, _)| v);
    let Some(value) = value else {
        return Err(Fail::refused(format!(
            "{slug} has no {key} set, {meaning}: docket skills set {key} \"...\""
        )));
    };
    println!("{value}");
    Ok(0)
}

/// The directory holding the running `docket`.
fn own_dir() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.display().to_string())
}

/// One fact set, or unset by an empty value, after the checks the server makes.
fn set(ctx: &mut Ctx, key: Option<&str>, value: &str, all_projects: bool) -> Result<i32> {
    let slug = ctx.project()?;
    let Some(key) = key.filter(|k| !k.is_empty()) else {
        return Err(Fail::refused(format!(
            "docket skills set KEY \"value\"; KEY one of {}",
            all_facts()
        )));
    };
    if all_projects {
        fact::check_owner(key, value)?;
        let out = ctx.api.set_fact(&slug, key, value, true)?;
        let now = out.owner.get(key).map_or("(unset)", String::as_str);
        println!("every project {key}: {now}");
        println!("  recorded for the owner on the server; a project that sets {key} keeps its own");
        return Ok(0);
    }
    fact::check(key, value)?;
    let out = ctx.api.set_fact(&slug, key, value, false)?;
    let now = out.skills.get(key).map_or("(unset)", String::as_str);
    println!("{slug} {key}: {now}");
    println!("  recorded on the project and pushed; every machine reads it with docket skills");
    Ok(0)
}

/// Every fact's value: the project's, the owner-level and default ones with the level each agent setting
/// came from, and a visible gap for the rest. A retired fact still stored is left out.
#[must_use]
pub fn values(
    project: &BTreeMap<String, String>,
    owner: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let project = fact::known(project);
    let mut out = BTreeMap::new();
    for (k, _) in FACTS {
        let text = match fact::layered(&project, owner, k) {
            Some((v, layer)) if fact::AGENT_SETTINGS.contains(&k) => {
                format!("{v}  ({})", layer.name())
            }
            Some((v, _)) => v,
            None => format!("(not set: docket skills set {k} \"...\")"),
        };
        out.insert(k.to_string(), text);
    }
    out
}

/// The facts as `docket skills` prints them, each under its meaning.
#[must_use]
pub fn facts_text(values: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let hang = format!("\n{}", " ".repeat(12));
    for (k, what) in FACTS {
        let value = values.get(k).map_or("", String::as_str);
        out.push_str(&format!("  {k:<9} {value}").replace('\n', &hang));
        out.push('\n');
        let _ = writeln!(out, "  {:<9} {what}", "");
    }
    out
}

fn show(ctx: &mut Ctx) -> Result<i32> {
    let slug = ctx.project()?;
    let facts = ctx.api.facts(&slug)?;
    let home = std::env::var("HOME").unwrap_or_default();
    let root = root_of(ctx, &slug).map_or_else(
        || "(no root bound on this host)".to_string(),
        |r| home_relative(&r, &home),
    );
    let values = values(&facts.skills, &facts.owner);
    println!(
        "{slug} on {}, root {root}\n\nThe facts the skills read with docket skills:\n",
        ctx.host()?
    );
    print!("{}", facts_text(&values));
    println!("\nInstalled, one generic copy per tool, in no repository:\n");
    for (tool, dir) in TOOLS {
        let have = installed(&expand(dir, &home));
        let have = if have.is_empty() {
            "nothing installed".to_string()
        } else {
            have.join(", ")
        };
        println!("  {tool:<7} {dir:<18} {have}");
    }
    println!(
        "\n  docket skills set KEY \"value\"    docket skills set --all-projects KEY \"value\"    \
         docket skills install    docket skills diff"
    );
    Ok(0)
}

fn changes(steps: &[Step]) -> bool {
    steps.iter().any(|s| s.action != Action::Current)
}

/// Each step that changes something, as `new  ~/.claude/skills/work/SKILL.md`; the rest counted.
pub fn list(steps: &[Step], home: &str) {
    let current = steps.iter().filter(|s| s.action == Action::Current).count();
    for s in steps.iter().filter(|s| s.action != Action::Current) {
        let path = home_relative(&s.path.display().to_string(), home);
        let why = if s.action == Action::Skip {
            "  (a symlink, never written through)"
        } else {
            ""
        };
        println!("  {:<10} {path}{why}", s.action.word());
    }
    if current > 0 {
        println!("  {current} current, left as they are");
    }
}

/// The steps listed, then written once the person says so, or at once with `yes`.
///
/// # Errors
/// Nobody can be asked and `yes` is not given, or a file cannot be written.
pub fn write_steps(steps: &[Step], home: &str, yes: bool) -> Result<i32> {
    if !changes(steps) {
        println!("Everything is current.");
        return Ok(0);
    }
    list(steps, home);
    if !yes && !confirmed()? {
        println!("Nothing written.");
        return Ok(1);
    }
    let written = templates::apply(steps).map_err(|e| Fail::refused(e.to_string()))?;
    println!("Written: {} paths.", written.len());
    Ok(0)
}

/// `y` typed at the prompt; a run with no terminal is refused rather than guessed.
fn confirmed() -> Result<bool> {
    use std::io::{BufRead, IsTerminal, Write as _};
    if !std::io::stdin().is_terminal() {
        return Err(Fail::refused(
            "nothing written: there is no terminal to ask on. Read the list, then pass --yes",
        ));
    }
    print!("Write these? [y/N] ");
    std::io::stdout().flush().ok();
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer).ok();
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

/// A path under the home directory written from `~`.
#[must_use]
pub fn home_relative(path: &str, home: &str) -> String {
    if home.is_empty() {
        return path.to_string();
    }
    match path.strip_prefix(home) {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_string(),
    }
}

fn expand(dir: &str, home: &str) -> PathBuf {
    match dir.strip_prefix("~/") {
        Some(rest) => Path::new(home).join(rest),
        None => PathBuf::from(dir),
    }
}

/// The skills a directory holds: each subdirectory with a `SKILL.md`, by name.
#[must_use]
pub fn installed(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().join("SKILL.md").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

#[cfg(test)]
#[path = "../tests/skills.rs"]
mod tests;
