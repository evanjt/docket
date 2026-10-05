use super::*;
use clap::CommandFactory;

#[test]
fn test_unclaim_and_its_hidden_alias_parse_alike() {
    let new = Cli::try_parse_from(["docket", "unclaim", "T1", "--bounce"]).unwrap();
    let old = Cli::try_parse_from(["docket", "release", "T1", "--bounce"]).unwrap();
    assert_eq!(format!("{:?}", new.cmd), format!("{:?}", old.cmd));
}

#[test]
fn test_only_unclaim_shows_in_the_help() {
    let help = command().render_help().to_string();
    assert!(help.contains("unclaim"));
    assert!(!help.contains("  release"));
}

#[test]
fn test_the_unread_bare_command_flags_are_gone() {
    for flag in ["--every", "--window", "--recent"] {
        assert!(
            Cli::try_parse_from(["docket", flag, "3"]).is_err(),
            "{flag}"
        );
    }
}

#[test]
fn test_help_hides_plumbing_and_groups_the_rest_by_role() {
    let help = command().render_help().to_string();
    for verb in ["dispatch", "collect", "reindex", "instructions", "private"] {
        assert!(!help.contains(&format!("\n  {verb} ")), "{verb} shown");
    }
    for heading in ["Owner:", "Agent:", "Lead:"] {
        assert!(help.contains(heading), "{heading}");
    }
}

#[test]
fn test_help_all_lists_the_plumbing() {
    let help = full_help();
    for verb in ["dispatch", "collect", "reindex", "instructions", "private"] {
        assert!(help.contains(&format!("\n  {verb} ")), "{verb} missing");
    }
}

#[test]
fn test_edit_set_help_lists_the_fields() {
    let mut cmd = Cli::command();
    let edit = cmd.find_subcommand_mut("edit").unwrap();
    let help = edit.render_long_help().to_string();
    assert!(help.contains("title"));
    assert!(help.contains("theme"));
}

#[test]
fn test_every_verb_not_marked_hidden_is_listed_under_a_heading() {
    let help = command().render_help().to_string();
    let missing: Vec<String> = Cli::command()
        .get_subcommands()
        .filter(|c| !c.is_hide_set())
        .map(|c| c.get_name().to_string())
        .filter(|n| !help.contains(&format!("\n  {n} ")))
        .collect();
    assert!(missing.is_empty(), "not in the help: {missing:?}");
}
