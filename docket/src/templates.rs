//! The role skills and the AGENTS.md block, carried in this binary, and what writing them changes:
//! each file added, overwritten, removed or already current.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One role's skill: its name, and its text for Claude Code and for Codex.
pub struct Skill {
    pub name: &'static str,
    pub claude: &'static str,
    pub codex: &'static str,
}

pub const SKILLS: [Skill; 6] = [
    Skill {
        name: "plan",
        claude: include_str!("../../skills/plan/SKILL.md"),
        codex: include_str!("../../skills/plan/SKILL.codex.md"),
    },
    Skill {
        name: "work",
        claude: include_str!("../../skills/work/SKILL.md"),
        codex: include_str!("../../skills/work/SKILL.codex.md"),
    },
    Skill {
        name: "audit",
        claude: include_str!("../../skills/audit/SKILL.md"),
        codex: include_str!("../../skills/audit/SKILL.codex.md"),
    },
    Skill {
        name: "lead",
        claude: include_str!("../../skills/lead/SKILL.md"),
        codex: include_str!("../../skills/lead/SKILL.codex.md"),
    },
    Skill {
        name: "ask-questions",
        claude: include_str!("../../skills/ask-questions/SKILL.md"),
        codex: include_str!("../../skills/ask-questions/SKILL.codex.md"),
    },
    Skill {
        name: "owner-queue",
        claude: include_str!("../../skills/owner-queue/SKILL.md"),
        codex: include_str!("../../skills/owner-queue/SKILL.codex.md"),
    },
];

/// Skills an earlier docket installed that the three roles replace.
pub const RETIRED: [&str; 8] = [
    "start-queue",
    "start-research",
    "start-audit",
    "ingest-plans",
    "package-triage",
    "consolidate-concepts",
    "elicit-stories",
    "codex-agents",
];

/// Each tool and the directory under the home it reads skills from.
pub const TOOLS: [(&str, &str); 2] = [("claude", ".claude/skills"), ("codex", ".agents/skills")];

pub const BLOCK: &str = include_str!("../../skills/docket-block.md");
pub const BEGIN: &str = "<!-- docket:begin, from evanjt/docket skills/docket-block.md; docket instructions install rewrites it -->";
pub const END: &str = "<!-- docket:end -->";
/// Headings that open a block pasted by hand, the current one and an earlier template's.
const PASTED: [&str; 2] = ["## The docket", "## The consolidation audit"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    New,
    Overwrite,
    Current,
    Remove,
    /// Never written: a symlink, which a write would follow into the file it points to.
    Skip,
}

impl Action {
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Action::New => "new",
            Action::Overwrite => "overwrite",
            Action::Current => "current",
            Action::Remove => "remove",
            Action::Skip => "skip",
        }
    }
}

/// One file or directory and what writing the templates does to it, with the text it ends holding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub action: Action,
    pub path: PathBuf,
    pub text: Option<String>,
}

fn step(action: Action, path: PathBuf, text: Option<String>) -> Step {
    Step { action, path, text }
}

/// What installing the skills for the tools does under `home`: a retired skill's directory removed,
/// then each role's `SKILL.md` added, overwritten or already current.
#[must_use]
pub fn skill_steps(home: &Path, tools: &[&str]) -> Vec<Step> {
    let dirs: Vec<(&str, PathBuf)> = TOOLS
        .iter()
        .filter(|(t, _)| tools.contains(t))
        .map(|(t, d)| (*t, home.join(d)))
        .collect();
    let mut out = Vec::new();
    for name in RETIRED {
        for (_, dir) in &dirs {
            let path = dir.join(name);
            if path.is_dir() && !path.is_symlink() {
                out.push(step(Action::Remove, path, None));
            }
        }
    }
    for s in &SKILLS {
        for (tool, dir) in &dirs {
            let text = if *tool == "codex" { s.codex } else { s.claude };
            let path = dir.join(s.name).join("SKILL.md");
            let action = match fs::read_to_string(&path) {
                Ok(have) if have == text => Action::Current,
                Ok(_) => Action::Overwrite,
                Err(_) => Action::New,
            };
            out.push(step(action, path, Some(text.to_string())));
        }
    }
    out
}

/// The block as it sits in an AGENTS.md, between its markers.
#[must_use]
pub fn block() -> String {
    format!("{BEGIN}\n{}\n{END}\n", BLOCK.trim_end())
}

/// `(start, end)` byte offsets of the marked block, the end past its newline.
fn marked(text: &str) -> Option<(usize, usize)> {
    let mut start = None;
    let mut pos = 0;
    for line in text.split_inclusive('\n') {
        if start.is_none() && line.starts_with("<!-- docket:begin") {
            start = Some(pos);
        } else if let Some(s) = start
            && line.starts_with("<!-- docket:end")
        {
            return Some((s, pos + line.len()));
        }
        pos += line.len();
    }
    None
}

/// `(start, end)` of a block pasted by hand: its heading to the next `## ` heading or the end.
fn pasted(text: &str) -> Option<(usize, usize)> {
    let mut start = None;
    let mut pos = 0;
    for line in text.split_inclusive('\n') {
        if start.is_none() && PASTED.contains(&line.trim_end()) {
            start = Some(pos);
        } else if let Some(s) = start
            && line.starts_with("## ")
        {
            return Some((s, pos));
        }
        pos += line.len();
    }
    start.map(|s| (s, text.len()))
}

/// An instruction file with the current block in it: refreshed between its markers, put in place of a
/// pasted one, or appended; current when it already holds it.
#[must_use]
pub fn rewritten(text: &str) -> (Action, String) {
    let b = block();
    if let Some((s, e)) = marked(text) {
        if text[s..e] == b {
            return (Action::Current, text.to_string());
        }
        return (
            Action::Overwrite,
            format!("{}{b}{}", &text[..s], &text[e..]),
        );
    }
    if let Some((s, e)) = pasted(text) {
        let tail = &text[e..];
        let gap = if tail.is_empty() { "" } else { "\n" };
        return (Action::Overwrite, format!("{}{b}{gap}{tail}", &text[..s]));
    }
    (
        Action::Overwrite,
        format!("{}\n\n{b}", text.trim_end_matches('\n')),
    )
}

/// The text with the marked block and the blank lines after it taken out.
fn without_block(text: &str) -> String {
    let Some((s, e)) = marked(text) else {
        return text.to_string();
    };
    let head = text[..s].trim_end_matches('\n');
    let tail = text[e..].trim_start_matches('\n');
    match (head.is_empty(), tail.is_empty()) {
        (true, _) => tail.to_string(),
        (false, true) => format!("{head}\n"),
        (false, false) => format!("{head}\n\n{tail}"),
    }
}

/// What installing the block does at a project root: its AGENTS.md written, and a copy of the block
/// in a CLAUDE.md beside it taken out, so no agent reads it twice.
#[must_use]
pub fn instruction_steps(root: &Path) -> Vec<Step> {
    let agents = root.join("AGENTS.md");
    if agents.is_symlink() {
        return vec![step(Action::Skip, agents, None)];
    }
    let mut out = vec![match fs::read_to_string(&agents) {
        Ok(text) => {
            let (action, text) = rewritten(&text);
            step(action, agents, Some(text))
        }
        Err(_) => step(Action::New, agents, Some(block())),
    }];
    let claude = root.join("CLAUDE.md");
    if !claude.is_symlink()
        && let Ok(text) = fs::read_to_string(&claude)
        && marked(&text).is_some()
    {
        out.push(step(Action::Overwrite, claude, Some(without_block(&text))));
    }
    out
}

/// The steps written: each file replaced by a rename, never rewritten in place, and each retired
/// directory removed. Returns the paths changed.
///
/// # Errors
/// A directory cannot be made, a file written or renamed, or a directory removed.
pub fn apply(steps: &[Step]) -> io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for s in steps {
        match (s.action, &s.text) {
            (Action::Remove, _) => fs::remove_dir_all(&s.path)?,
            (Action::New | Action::Overwrite, Some(text)) => {
                if let Some(dir) = s.path.parent() {
                    fs::create_dir_all(dir)?;
                }
                let tmp = s
                    .path
                    .with_extension(format!("docket-{}", std::process::id()));
                fs::write(&tmp, text)?;
                fs::rename(&tmp, &s.path)?;
            }
            _ => continue,
        }
        written.push(s.path.clone());
    }
    Ok(written)
}

#[cfg(test)]
#[path = "tests/templates.rs"]
mod tests;
