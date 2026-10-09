use super::*;

fn sel(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|s| (*s).to_string()).collect()
}

fn post(line: &str, selection: &[&str]) -> (String, Value) {
    match parse(line, "o/p", &sel(selection)) {
        Ok(Command::Post(verb, body)) => (verb, body),
        other => panic!("{line}: {other:?}"),
    }
}

#[test]
fn test_parse_runs_an_id_verb_on_the_selected_item() {
    let (verb, body) = post("close abc1234 --gates 'passed: all'", &["T3"]);
    assert_eq!(verb, "close");
    assert_eq!(
        body,
        json!({ "project": "o/p", "force": false, "id": "T3", "resolution": "abc1234", "gates": "passed: all" })
    );
}

#[test]
fn test_parse_a_named_id_wins_over_the_selection() {
    let (_, body) = post("answer Q2 use the second store", &["T3"]);
    assert_eq!(body["id"], "Q2");
    assert_eq!(body["decision"], "use the second store");
}

#[test]
fn test_parse_a_many_id_verb_takes_the_marked_rows() {
    let (verb, body) = post("priority high", &["T3", "B1"]);
    assert_eq!(verb, "priority");
    assert_eq!(body["ids"], json!(["T3", "B1"]));
    assert_eq!(body["tier"], "high");
    let (_, body) = post("priority T7 T8 low", &["T3"]);
    assert_eq!(body["ids"], json!(["T7", "T8"]));
    assert_eq!(body["tier"], "low");
}

#[test]
fn test_parse_link_puts_the_selection_where_the_ids_go() {
    let (_, body) = post("link related CON1 --remove", &["T3", "B1"]);
    assert_eq!(body["a"], json!(["T3", "B1"]));
    assert_eq!(
        (&body["kind"], &body["b"], &body["remove"]),
        (&json!("related"), &json!("CON1"), &json!(true))
    );
}

#[test]
fn test_parse_aliases_flags_and_lists() {
    assert_eq!(post("park T3 need a device", &[]).0, "ask");
    assert_eq!(post("built T3 abc1234", &[]).0, "close");
    let (_, body) = post(
        "edit --set theme=crusts --set group=g --append=noted --force",
        &["T3"],
    );
    assert_eq!(
        body["set"],
        json!([{ "field": "theme", "value": "crusts" }, { "field": "group", "value": "g" }])
    );
    assert_eq!(body["append"], "noted");
    assert_eq!(body["force"], true);
    let (_, body) = post("release T3 --bounce --outcome ended", &[]);
    assert_eq!(body["bounce"], true);
    assert_eq!(body["outcome"], "ended");
}

#[test]
fn test_parse_skills_set_posts_a_fact() {
    let (verb, body) = post("skills set gates make test", &[]);
    assert_eq!(verb, "fact");
    assert_eq!(
        (&body["key"], &body["value"]),
        (&json!("gates"), &json!("make test"))
    );
    let (_, body) = post("skills owner", &[]);
    assert!(body.get("value").is_none(), "no value unsets: {body}");
    assert_eq!(parse("skills", "o/p", &[]), Ok(Command::Settings));
}

#[test]
fn test_parse_reads_open_their_page() {
    assert_eq!(
        parse("show", "o/p", &sel(&["T3"])),
        Ok(Command::Open(Target::Item("T3".into())))
    );
    assert_eq!(
        parse("q", "o/p", &[]),
        Ok(Command::Open(Target::List(Listing::Route(
            Route::Questions
        ))))
    );
    assert_eq!(
        parse("search loaf count", "o/p", &[]),
        Ok(Command::Search("loaf count".into()))
    );
    assert_eq!(
        parse("status", "o/p", &[]),
        Ok(Command::Open(Target::Project("o/p".into())))
    );
}

#[test]
fn test_parse_refusals_say_what_is_wrong() {
    assert_eq!(
        parse("close", "o/p", &[]).unwrap_err(),
        "close needs an id: select a row or name one"
    );
    assert_eq!(
        parse("rate T3", "o/p", &[]).unwrap_err(),
        "rate needs level"
    );
    assert_eq!(
        parse("rate T3 high x", "o/p", &[]).unwrap_err(),
        "rate takes no x"
    );
    assert_eq!(
        parse("drop T3 --colour red", "o/p", &[]).unwrap_err(),
        "drop takes no --colour"
    );
    assert_eq!(
        parse("graph", "o/p", &[]).unwrap_err(),
        "docket graph is not a verb the screen runs: run it in a terminal"
    );
    assert_eq!(
        parse("answer T3 'open", "o/p", &[]).unwrap_err(),
        "a quote is left open"
    );
}

#[test]
fn test_split_keeps_quoted_runs_whole() {
    assert_eq!(split(r#"a "b c" 'd' e"#).unwrap(), ["a", "b c", "d", "e"]);
    assert_eq!(split(r#"x "" y"#).unwrap(), ["x", "", "y"]);
    assert_eq!(split("   ").unwrap(), [] as [std::string::String; 0]);
}

#[test]
fn test_parse_setup_verbs_post_their_requests() {
    let (verb, body) = post(
        "releases add 9.9.9 --target 2030-01-01 --note 'a line'",
        &[],
    );
    assert_eq!(verb, "releases");
    assert_eq!(
        (
            &body["action"],
            &body["name"],
            &body["target_date"],
            &body["note"]
        ),
        (
            &json!("add"),
            &json!("9.9.9"),
            &json!("2030-01-01"),
            &json!("a line")
        )
    );
    let (_, body) = post("releases ship 9.9.9 --move-open-to 9.9.10", &[]);
    assert_eq!(body["to"], "9.9.10");
    let (_, body) = post("releases move 9.9.9 --to 9.9.10 --carry", &[]);
    assert_eq!(body["carry"], true);

    let (verb, body) = post("areas move billing --to 2", &[]);
    assert_eq!(verb, "areas");
    assert_eq!((&body["action"], &body["to"]), (&json!("move"), &json!(2)));
    let (_, body) = post("areas edit billing --name invoicing --priority high", &[]);
    assert_eq!(
        (&body["rename"], &body["priority"]),
        (&json!("invoicing"), &json!("high"))
    );

    let (verb, body) = post("label add T3 crusty --about 'a loaf label'", &[]);
    assert_eq!(verb, "label");
    assert_eq!(
        (&body["action"], &body["id"], &body["name"], &body["about"]),
        (
            &json!("add"),
            &json!("T3"),
            &json!("crusty"),
            &json!("a loaf label")
        )
    );
    let (_, body) = post("label rm crusty", &["T4"]);
    assert_eq!(
        (&body["id"], &body["name"]),
        (&json!("T4"), &json!("crusty"))
    );

    let (verb, body) = post("dep add T3 T4 T5", &[]);
    assert_eq!(verb, "dep");
    assert_eq!(body["id"], "T3");
    assert_eq!(body["on"], json!(["T4", "T5"]));
    assert_eq!(body["remove"], false);
    assert_eq!(post("dep rm T3 T4", &[]).1["remove"], true);

    let (verb, body) = post("parent T3 T4 PL1", &[]);
    assert_eq!(verb, "parent");
    assert_eq!(
        (&body["a"], &body["plan"]),
        (&json!(["T3", "T4"]), &json!("PL1"))
    );
    let (_, body) = post("parent PL1", &["T3", "T4"]);
    assert_eq!(
        (&body["a"], &body["plan"]),
        (&json!(["T3", "T4"]), &json!("PL1"))
    );
    let (_, body) = post("parent T3 --none", &[]);
    assert_eq!(body["a"], json!(["T3"]));
    assert!(body.get("plan").is_none(), "{body}");

    let (verb, body) = post(
        "machine set oven --ssh me@oven --slots 3 --runners claude,codex",
        &[],
    );
    assert_eq!(verb, "machine");
    assert_eq!(body["name"], "oven");
    assert_eq!(body["slots"], json!(3));
    assert_eq!(body["runners"], json!(["claude", "codex"]));
    assert_eq!(body["remove"], false);
    assert_eq!(post("machine remove oven", &[]).1["remove"], true);

    let (verb, body) = post("limit oven claude 2030-01-01T00:00:00Z", &[]);
    assert_eq!(verb, "limit");
    assert_eq!(
        (&body["machine"], &body["runner"], &body["until"]),
        (
            &json!("oven"),
            &json!("claude"),
            &json!("2030-01-01T00:00:00Z")
        )
    );

    let (verb, body) = post("lead give --session s1", &[]);
    assert_eq!(verb, "lead");
    assert_eq!(
        (&body["act"], &body["session"]),
        (&json!("give"), &json!("s1"))
    );
}

#[test]
fn test_parse_setup_verbs_refuse_what_they_cannot_read() {
    assert!(parse("machine wipe oven", "o/p", &[]).is_err());
    assert!(parse("areas move billing --to two", "o/p", &[]).is_err());
    assert!(parse("lead give", "o/p", &[]).is_err());
}

/// The `/do/{verb}` routes the server registers, read from its source.
fn server_routes() -> Vec<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let verbs = path.ends_with("verbs/mod.rs");
            for line in text.lines().map(str::trim) {
                let Some(rest) = line.strip_prefix(".route(\"/") else {
                    continue;
                };
                let Some((name, tail)) = rest.split_once('"') else {
                    continue;
                };
                if !tail.contains("post(") {
                    continue;
                }
                if verbs {
                    out.push(name.to_string());
                } else if let Some(name) = name.strip_prefix("do/") {
                    out.push(name.to_string());
                }
            }
        }
    }
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docket-server/src");
    let mut out = Vec::new();
    walk(&src, &mut out);
    out.sort();
    out.dedup();
    out
}

/// Routes that are not the owner's to run from the screen, and why.
const NOT_FOR_THE_SCREEN: [&str; 5] = [
    // an agent's job report, posted by `docket job`
    "job-report",
    // a checkout resolving its project, posted by every client command
    "project",
    // what a squash wrote, posted by `docket publish`
    "publication",
    // a history rewrite's sha map, posted by `docket admin remap`
    "remap",
    // a project's move to a new slug, posted by `docket projects rename`; the screen is opened on a slug
    "projects-rename",
];

#[test]
fn test_every_server_write_route_has_a_palette_verb() {
    let routes = server_routes();
    assert!(routes.len() > 20, "the scan found {routes:?}");
    let missing: Vec<&String> = routes
        .iter()
        .filter(|r| !NOT_FOR_THE_SCREEN.contains(&r.as_str()))
        .filter(|r| {
            !SPECS
                .iter()
                .any(|s| s.route == r.as_str() || s.names.contains(&r.as_str()))
        })
        .collect();
    assert!(missing.is_empty(), "no palette verb posts to {missing:?}");
}

#[test]
fn test_every_palette_verb_posts_to_a_server_route() {
    let routes = server_routes();
    let stray: Vec<&str> = SPECS
        .iter()
        .map(|s| s.route)
        .filter(|r| !routes.iter().any(|x| x == r))
        .collect();
    assert!(stray.is_empty(), "the server has no route {stray:?}");
}
