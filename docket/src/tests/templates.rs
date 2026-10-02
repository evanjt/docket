use std::fs;
use std::path::Path;

use super::*;

/// Project and owner names a template must never carry: every fact comes from `docket skills`.
const NAMED: [&str; 9] = [
    "sample",
    "sample-audit",
    "evanjt",
    "sample-costs",
    "sample-snow",
    "sample-lake",
    "Ada",
    "{{",
    "~/projects/",
];

fn texts() -> Vec<(String, &'static str)> {
    let mut out = vec![("docket-block.md".to_string(), BLOCK)];
    for s in &SKILLS {
        out.push((format!("{}/SKILL.md", s.name), s.claude));
        out.push((format!("{}/SKILL.codex.md", s.name), s.codex));
    }
    out
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn listed(steps: &[Step], home: &Path) -> Vec<String> {
    steps
        .iter()
        .map(|s| {
            let rel = s.path.strip_prefix(home).unwrap().display();
            format!("{} {rel}", s.action.word())
        })
        .collect()
}

#[test]
fn test_templates_name_no_project() {
    for (name, text) in texts() {
        for word in NAMED {
            assert!(!text.contains(word), "{name} names {word}");
        }
        assert!(!text.contains('\u{2014}'), "{name} carries an em dash");
    }
}

#[test]
fn test_each_skill_names_itself_and_its_role() {
    let names: Vec<&str> = SKILLS.iter().map(|s| s.name).collect();
    assert_eq!(names, ["plan", "work", "audit"]);
    for s in &SKILLS {
        for text in [s.claude, s.codex] {
            assert!(
                text.starts_with(&format!("---\nname: {}\n", s.name)),
                "{}",
                s.name
            );
            assert!(text.contains(&format!("--role {}", s.name)), "{}", s.name);
        }
    }
}

#[test]
fn test_the_block_is_under_one_and_a_half_kilobytes() {
    let b = block();
    assert!(b.len() < 1536, "{} bytes", b.len());
    assert!(b.starts_with(BEGIN) && b.ends_with(&format!("{END}\n")));
    assert!(b.contains("docket wait ID --on"));
    assert!(b.contains("/work"));
}

#[test]
fn test_install_on_a_scratch_home_lists_and_writes_exactly_the_skills() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path();
    write(&h.join(".claude/skills/start-queue/SKILL.md"), "old loop");
    write(&h.join(".agents/skills/codex-agents/SKILL.md"), "old fleet");
    write(
        &h.join(".claude/skills/ask-questions/SKILL.md"),
        "the owner's own",
    );
    write(
        &h.join(".claude/skills/work/SKILL.md"),
        "an older work skill",
    );
    write(&h.join(".agents/skills/audit/SKILL.md"), SKILLS[2].codex);
    let steps = skill_steps(h, &["claude", "codex"]);
    assert_eq!(
        listed(&steps, h),
        [
            "remove .claude/skills/start-queue",
            "remove .agents/skills/codex-agents",
            "new .claude/skills/plan/SKILL.md",
            "new .agents/skills/plan/SKILL.md",
            "overwrite .claude/skills/work/SKILL.md",
            "new .agents/skills/work/SKILL.md",
            "new .claude/skills/audit/SKILL.md",
            "current .agents/skills/audit/SKILL.md",
        ]
    );
    apply(&steps).unwrap();
    assert!(!h.join(".claude/skills/start-queue").exists());
    assert!(!h.join(".agents/skills/codex-agents").exists());
    assert!(h.join(".claude/skills/ask-questions/SKILL.md").exists());
    assert_eq!(
        fs::read_to_string(h.join(".claude/skills/work/SKILL.md")).unwrap(),
        SKILLS[1].claude
    );
    assert_eq!(
        fs::read_to_string(h.join(".agents/skills/plan/SKILL.md")).unwrap(),
        SKILLS[0].codex
    );
    let again = skill_steps(h, &["claude", "codex"]);
    assert!(
        again.iter().all(|s| s.action == Action::Current),
        "{again:?}"
    );
}

#[test]
fn test_install_for_one_tool_leaves_the_other_alone() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path();
    write(&h.join(".agents/skills/start-queue/SKILL.md"), "old loop");
    let steps = skill_steps(h, &["claude"]);
    assert!(steps.iter().all(|s| s.path.starts_with(h.join(".claude"))));
    assert_eq!(steps.len(), 3);
}

#[test]
fn test_rewritten_appends_refreshes_adopts_or_keeps_the_block() {
    let b = block();
    let (a, text) = rewritten("# Rules\n\nOwn rules.\n");
    assert_eq!(a, Action::Overwrite);
    assert_eq!(text, format!("# Rules\n\nOwn rules.\n\n{b}"));
    let (a, again) = rewritten(&text);
    assert_eq!((a, again.as_str()), (Action::Current, text.as_str()));
    let stale =
        "# Rules\n\n<!-- docket:begin, old -->\nold block\n<!-- docket:end -->\n\n## Tail\n";
    let (a, text) = rewritten(stale);
    assert_eq!(a, Action::Overwrite);
    assert_eq!(text, format!("# Rules\n\n{b}\n## Tail\n"));
    let pasted = "# Rules\n\n## The docket\n\nold words\n\n## Tail\n";
    let (a, text) = rewritten(pasted);
    assert_eq!(a, Action::Overwrite);
    assert_eq!(text, format!("# Rules\n\n{b}\n## Tail\n"));
}

#[test]
fn test_instructions_write_agents_md_and_retire_a_claude_md_copy() {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    write(&r.join("CLAUDE.md"), &format!("# Local\n\n{}", block()));
    let steps = instruction_steps(r);
    assert_eq!(listed(&steps, r), ["new AGENTS.md", "overwrite CLAUDE.md"]);
    apply(&steps).unwrap();
    assert_eq!(fs::read_to_string(r.join("AGENTS.md")).unwrap(), block());
    assert_eq!(
        fs::read_to_string(r.join("CLAUDE.md")).unwrap(),
        "# Local\n"
    );
    assert!(
        instruction_steps(r)
            .iter()
            .all(|s| s.action == Action::Current)
    );
}

#[cfg(unix)]
#[test]
fn test_instructions_never_write_through_a_symlink() {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    write(&r.join("elsewhere.md"), "# Elsewhere\n");
    std::os::unix::fs::symlink(r.join("elsewhere.md"), r.join("AGENTS.md")).unwrap();
    let steps = instruction_steps(r);
    assert_eq!(listed(&steps, r), ["skip AGENTS.md"]);
    apply(&steps).unwrap();
    assert_eq!(
        fs::read_to_string(r.join("elsewhere.md")).unwrap(),
        "# Elsewhere\n"
    );
}
