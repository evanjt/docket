use super::*;

#[test]
fn test_left_to_do_names_the_dump_and_the_other_machines_and_the_job_directory_only_when_present() {
    let dir = std::env::temp_dir().join(format!("docket-rename-{}", std::process::id()));
    let lines = left_to_do("garden/shed", "garden/barn", &dir);
    assert_eq!(lines.len(), 3);
    assert!(
        lines[1].contains("git mv garden/shed garden/barn"),
        "{}",
        lines[1]
    );
    assert!(
        lines[2].contains("docket projects rename garden/shed garden/barn"),
        "{}",
        lines[2]
    );
    std::fs::create_dir_all(dir.join("garden-shed")).unwrap();
    let lines = left_to_do("garden/shed", "garden/barn", &dir);
    assert_eq!(lines.len(), 4);
    assert!(
        lines[1].contains("garden-shed") && lines[1].contains("garden-barn"),
        "{}",
        lines[1]
    );
    std::fs::remove_dir_all(dir).unwrap();
}
