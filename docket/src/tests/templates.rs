use std::fs;
use std::path::Path;

use super::*;

/// What a template must never carry: a project's own path or an unfilled placeholder. Real project
/// and owner names are kept out of every tracked file by `docket private check`.
const NAMED: [&str; 3] = ["{{", "~/projects/", "/home/"];

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
    assert_eq!(
        names,
        [
            "plan",
            "work",
            "audit",
            "lead",
            "ask-questions",
            "owner-queue",
            "squash"
        ]
    );
    for s in &SKILLS {
        for text in [s.claude, s.codex] {
            assert!(
                text.starts_with(&format!("---\nname: {}\n", s.name)),
                "{}",
                s.name
            );
            let role = match s.name {
                "lead" => "docket lead take".to_string(),
                "ask-questions" | "owner-queue" | "squash" => "docket skills".to_string(),
                _ => format!("--role {}", s.name),
            };
            assert!(text.contains(&role), "{}", s.name);
        }
    }
}

#[test]
fn test_owner_skills_order_by_release_and_set_no_theme() {
    for name in ["ask-questions", "owner-queue"] {
        let s = SKILLS.iter().find(|s| s.name == name).unwrap();
        for text in [s.claude, s.codex] {
            assert!(!text.contains("--set theme="), "{name} sets a theme");
            assert!(!text.contains("batch size"), "{name} cites a batch size");
            assert!(!text.contains("background sync"), "{name} cites a sync");
            assert!(text.contains("release"), "{name} does not order by release");
        }
    }
}

#[test]
fn test_every_template_keeps_the_private_docket_out_of_the_repository() {
    for (name, text) in texts() {
        assert!(text.contains("docket private check"), "{name}");
    }
}

#[test]
fn test_no_template_carries_an_address() {
    let address = regex::Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap();
    for (name, text) in texts() {
        assert!(!address.is_match(text), "{name} carries an address");
    }
}

#[test]
fn test_the_marker_names_the_tool_and_an_older_marker_is_still_replaced() {
    assert!(!BEGIN.contains("evanjt"), "{BEGIN}");
    let older = "# Rules\n\n<!-- docket:begin, from owner/tool skills/docket-block.md; docket instructions install rewrites it -->\nold\n<!-- docket:end -->\n";
    let (a, text) = rewritten(older);
    assert_eq!(a, Action::Overwrite);
    assert_eq!(text, format!("# Rules\n\n{}", block()));
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
            "new .claude/skills/lead/SKILL.md",
            "new .agents/skills/lead/SKILL.md",
            "overwrite .claude/skills/ask-questions/SKILL.md",
            "new .agents/skills/ask-questions/SKILL.md",
            "new .claude/skills/owner-queue/SKILL.md",
            "new .agents/skills/owner-queue/SKILL.md",
            "new .claude/skills/squash/SKILL.md",
            "new .agents/skills/squash/SKILL.md",
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
    assert_eq!(steps.len(), 7);
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

#[test]
fn test_a_derived_answer_names_its_carrier_and_a_routine_choice_is_a_note() {
    for (name, text) in texts() {
        if text.contains("--derived") {
            assert!(text.contains("--carried-by"), "{name}");
            assert!(text.contains("outine choice"), "{name}");
        }
    }
}

#[test]
fn test_the_release_triage_has_one_owner_and_no_copy() {
    assert!(BLOCK.contains("names its release"));
    assert!(BLOCK.contains("data loss"));
    assert!(BLOCK.contains("A ticket under a plan takes its release"));
    for (name, text) in texts() {
        if name == "docket-block.md" {
            continue;
        }
        assert!(!text.contains("names its release"), "{name}");
        assert!(!text.contains("a feature or polish"), "{name}");
    }
}

#[test]
fn test_no_template_files_every_side_finding() {
    for (name, text) in texts() {
        assert!(!text.contains("docket add"), "{name}");
    }
}

#[test]
fn test_the_audit_skill_files_only_critical_and_high_gaps() {
    let audit = SKILLS.iter().find(|s| s.name == "audit").unwrap();
    for text in [audit.claude, audit.codex] {
        assert!(text.contains("critical or high gap"));
        assert!(text.contains("normal or low gap is an observation"));
    }
}

#[test]
fn test_both_lead_variants_land_in_a_batch_and_resolve_conflicts_themselves() {
    let lead = SKILLS.iter().find(|s| s.name == "lead").unwrap();
    for text in [lead.claude, lead.codex] {
        assert!(text.contains("lead/batch"));
        assert!(text.contains("A conflict is yours to resolve"));
        assert!(!text.contains("never resolved by hand"));
    }
}

fn work_texts() -> Vec<(String, &'static str)> {
    let work = SKILLS.iter().find(|s| s.name == "work").unwrap();
    vec![
        ("work/SKILL.md".to_string(), work.claude),
        ("work/SKILL.codex.md".to_string(), work.codex),
        ("job/build.md".to_string(), crate::job::BRIEFS[0].1),
    ]
}

#[test]
fn test_work_and_build_name_the_one_owner_of_a_fact_before_fixing_it() {
    for (name, text) in work_texts() {
        assert!(text.contains("one owner"), "{name}");
        assert!(text.contains("inventory"), "{name}");
    }
}

#[test]
fn test_a_side_finding_command_carries_a_complexity() {
    for (name, text) in work_texts() {
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let at = flat.find("docket new B").unwrap();
        let command = flat[at..].split('`').next().unwrap();
        assert!(command.contains("--complexity"), "{name}");
    }
}

#[test]
fn test_no_installed_text_names_a_retired_skill() {
    let mut all = texts();
    all.extend(crate::job::BRIEFS.iter().map(|(n, t)| (n.to_string(), *t)));
    for (name, text) in all {
        for old in RETIRED {
            assert!(!text.contains(old), "{name} names {old}");
        }
    }
}

#[test]
fn test_no_text_orders_the_queue_by_priority_alone() {
    for (name, text) in texts() {
        let flat = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        for phrase in [
            "most urgent first",
            "most urgent then",
            "only order the queue reads",
        ] {
            assert!(!flat.contains(phrase), "{name} says \"{phrase}\"");
        }
    }
    let plan = SKILLS.iter().find(|s| s.name == "plan").unwrap();
    for text in [plan.claude, plan.codex] {
        let flat = text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        assert!(
            flat.contains("release, then priority, then age"),
            "plan does not state the queue order"
        );
        assert!(
            flat.contains(
                "an item with no release is in the backlog, which ranks after every release"
            ),
            "plan does not say where an item with no release ranks"
        );
    }
}

/// Where the word names a release of the project (the theme an item ships in), not the act of
/// giving a claim back or freeing a waiter.
const RELEASE_NOUN: [&str; 33] = [
    "--release",
    "skills releases",
    "the releases",
    "releases fact",
    "releases in order",
    "current release",
    "current-release",
    "later release",
    "next release",
    "a release",
    "the release",
    "each release",
    "its release",
    "one release",
    "this release",
    "by release",
    "in release",
    "then release",
    "release, then",
    "release first",
    "release work",
    "release never",
    "that release",
    "'s release",
    "<release>",
    "release of",
    "release and",
    "release order",
    "**release.**",
    "releases, the current",
    "no release",
    "every release",
    "release is in",
];

fn release_verbs(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in text.lines() {
        let mut lower = line.to_lowercase();
        for noun in RELEASE_NOUN {
            lower = lower.replace(noun, "");
        }
        if lower.contains("releas") {
            found.push(line.trim().to_string());
        }
    }
    found
}

#[test]
fn test_no_skill_or_brief_says_release_for_giving_a_claim_back_or_freeing_a_waiter() {
    let mut all = texts();
    for (name, text) in crate::job::BRIEFS {
        all.push((format!("job/{name}.md"), text));
    }
    let mut found = Vec::new();
    for (name, text) in all {
        for line in release_verbs(text) {
            found.push(format!("{name}: {line}"));
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn test_no_lead_variant_merges_with_a_message_naming_the_branch() {
    let lead = SKILLS.iter().find(|s| s.name == "lead").unwrap();
    for text in [lead.claude, lead.codex] {
        assert!(!text.contains("--no-edit"));
        assert!(text.contains("merge --no-ff -m \"Merge <the job's commit subject>\""));
    }
}

#[test]
fn test_every_unclaim_in_a_skill_names_its_outcome() {
    for (name, text) in texts() {
        for (n, line) in text.lines().enumerate() {
            if line.contains("docket unclaim") || line.contains("> unclaim ") {
                assert!(line.contains("--outcome"), "{name}:{}: {line}", n + 1);
            }
        }
    }
}

#[test]
fn filing_skills_say_how_to_name_repositories() {
    for name in ["plan", "work", "lead", "audit"] {
        let s = SKILLS.iter().find(|s| s.name == name).unwrap();
        for (copy, text) in [("claude", s.claude), ("codex", s.codex)] {
            for word in ["--repo", "@OWNER/NAME"] {
                assert!(text.contains(word), "{name} {copy} skill lacks {word}");
            }
            if name == "lead" {
                assert!(
                    text.contains("OBSERVE [REPO]"),
                    "lead {copy} skill lacks OBSERVE [REPO]"
                );
            }
        }
    }
}
