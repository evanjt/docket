use super::*;

#[test]
fn test_ready_counts_the_migrations_applied() {
    assert_eq!(ready(&[]), "database ready, no migrations applied");
    assert_eq!(
        ready(&["m1".to_string()]),
        "database ready, 1 migration applied (m1)"
    );
    assert_eq!(
        ready(&["m1".to_string(), "m2".to_string()]),
        "database ready, 2 migrations applied (m1, m2)"
    );
}

#[tokio::test]
async fn test_stop_signal_resolves_on_sigterm() {
    let stop = stop_signal().unwrap();
    let sent = std::process::Command::new("kill")
        .args(["-TERM", &std::process::id().to_string()])
        .status()
        .unwrap();
    assert!(sent.success());
    let waited = tokio::time::timeout(std::time::Duration::from_secs(2), stop).await;
    assert!(waited.is_ok(), "SIGTERM did not stop the server");
}

#[test]
fn test_the_server_takes_no_subcommand() {
    assert!(Args::try_parse_from(["docket-server"]).is_ok());
    for args in [
        ["docket-server", "import", "--from", "copy.db"].as_slice(),
        ["docket-server", "simplify", "--write"].as_slice(),
    ] {
        assert!(Args::try_parse_from(args).is_err(), "{args:?} parsed");
    }
}
