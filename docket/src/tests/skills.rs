use super::*;

fn args(
    what: Option<&str>,
    key: Option<&str>,
    value: Option<&str>,
) -> (String, Option<String>, Option<String>) {
    read_args(what, key, value)
}

fn owned(
    what: &str,
    key: Option<&str>,
    value: Option<&str>,
) -> (String, Option<String>, Option<String>) {
    (what.into(), key.map(Into::into), value.map(Into::into))
}

#[test]
fn test_read_args_bare_is_show() {
    assert_eq!(args(None, None, None), owned("show", None, None));
}

#[test]
fn test_read_args_a_fact_alone_gets_it_and_with_a_value_sets_it() {
    assert_eq!(
        args(Some("owner"), None, None),
        owned("get", Some("owner"), None)
    );
    assert_eq!(
        args(Some("owner"), Some("Ana"), None),
        owned("set", Some("owner"), Some("Ana"))
    );
}

#[test]
fn test_read_args_a_computed_name_gets_it() {
    assert_eq!(
        args(Some("root"), None, None),
        owned("get", Some("root"), None)
    );
}

#[test]
fn test_read_args_set_and_get_pass_through() {
    assert_eq!(
        args(Some("set"), Some("mode"), Some("run")),
        owned("set", Some("mode"), Some("run"))
    );
    assert_eq!(
        args(Some("get"), Some("land"), None),
        owned("get", Some("land"), None)
    );
}

#[test]
fn test_values_layer_set_over_default_and_mark_the_rest() {
    let set: BTreeMap<String, String> = [
        ("poll".to_string(), "30".to_string()),
        ("land".into(), String::new()),
    ]
    .into_iter()
    .collect();
    let v = values(&set, Some("2026-10-01T00:00:00Z"));
    assert_eq!(v["poll"], "30");
    assert_eq!(v["owner"], "the owner");
    assert_eq!(v["land"], "(not set: docket skills set land \"...\")");
    assert_eq!(v["last_tick"], "2026-10-01T00:00:00Z");
    assert_eq!(v.len(), FACTS.len());
}

#[test]
fn test_facts_text_hangs_a_value_s_lines_under_it() {
    let mut v = values(&BTreeMap::new(), None);
    v.insert("traps".into(), "one\ntwo".into());
    let text = facts_text(&v);
    assert!(
        text.starts_with("  owner     the owner\n            the owner's name"),
        "{text}"
    );
    assert!(
        text.contains("  traps     one\n            two\n"),
        "{text}"
    );
    assert!(text.contains("  model_build (not set"), "{text}");
}

#[test]
fn test_home_relative_shortens_only_paths_under_home() {
    assert_eq!(home_relative("/home/a/src/x", "/home/a"), "~/src/x");
    assert_eq!(home_relative("/home/a", "/home/a"), "~");
    assert_eq!(home_relative("/home/ab/x", "/home/a"), "/home/ab/x");
}

#[test]
fn test_installed_lists_directories_holding_a_skill_sorted() {
    let dir = std::env::temp_dir().join(format!("docket-skills-{}", std::process::id()));
    for (name, skill) in [("zeta", true), ("alpha", true), ("empty", false)] {
        std::fs::create_dir_all(dir.join(name)).unwrap();
        if skill {
            std::fs::write(dir.join(name).join("SKILL.md"), "x").unwrap();
        }
    }
    assert_eq!(installed(&dir), ["alpha", "zeta"]);
    assert!(installed(&dir.join("missing")).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}
