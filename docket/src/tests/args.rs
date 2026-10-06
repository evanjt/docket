use super::*;
use clap::CommandFactory;

#[test]
fn test_unclaim_and_its_hidden_alias_parse_alike() {
    let new =
        Cli::try_parse_from(["docket", "unclaim", "T1", "--bounce", "--outcome", "ended"]).unwrap();
    let old =
        Cli::try_parse_from(["docket", "release", "T1", "--bounce", "--outcome", "ended"]).unwrap();
    assert_eq!(format!("{:?}", new.cmd), format!("{:?}", old.cmd));
}

#[test]
fn test_an_unclaim_without_an_outcome_does_not_parse() {
    assert!(Cli::try_parse_from(["docket", "unclaim", "T1", "note"]).is_err());
}

#[test]
fn test_only_unclaim_shows_in_the_help() {
    let help = command().render_help().to_string();
    assert!(help.contains("unclaim"));
    assert!(!help.contains("\n  release "));
}

#[test]
fn test_releases_ship_takes_where_the_open_items_go() {
    let cli = Cli::try_parse_from([
        "docket",
        "releases",
        "ship",
        "1.0.0",
        "--move-open-to",
        "1.1.0",
    ])
    .unwrap();
    let shown = format!("{:?}", cli.cmd);
    assert!(shown.contains("\"ship\""), "{shown}");
    assert!(shown.contains("move_open_to: Some(\"1.1.0\")"), "{shown}");
    assert!(Cli::try_parse_from(["docket", "releases", "rename", "1.0.0"]).is_err());
    assert!(
        command()
            .render_help()
            .to_string()
            .contains("\n  releases ")
    );
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
