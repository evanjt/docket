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
    let (_, body) = post("release T3 --bounce", &[]);
    assert_eq!(body["bounce"], true);
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
        parse("search bun cache", "o/p", &[]),
        Ok(Command::Search("bun cache".into()))
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
    assert!(split("   ").unwrap().is_empty());
}
