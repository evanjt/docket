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
    let help = Cli::command().render_help().to_string();
    assert!(help.contains("unclaim"));
    assert!(!help.contains("  release"));
}
