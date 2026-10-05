use super::*;

fn area(name: &str, position: i64) -> Area {
    Area {
        name: name.to_string(),
        position,
        ..Area::default()
    }
}

fn entry(id: &str, state: &str, kind: Kind, area: Option<&str>) -> Entry {
    Entry {
        id: id.to_string(),
        title: format!("title of {id}"),
        state: state.to_string(),
        kind,
        area: area.map(str::to_string),
        plan: None,
    }
}

#[test]
fn test_a_release_groups_its_closed_work_by_area_in_position_order() {
    let areas = [area("kites", 1), area("lanterns", 0)];
    let items = [
        entry("B1", "done", Kind::Work, Some("kites")),
        entry("B2", "done", Kind::Work, Some("lanterns")),
        entry("T3", "done", Kind::Work, Some("lanterns")),
        entry("B4", "done", Kind::Work, Some("lanterns")),
        entry("T5", "dropped", Kind::Work, Some("kites")),
        entry("Q6", "done", Kind::Decision, Some("kites")),
        entry("A7", "done", Kind::Audit, Some("kites")),
    ];
    let got = changelog(&items, &areas);
    let heads: Vec<String> = got.iter().map(Section::head).collect();
    assert_eq!(heads, ["lanterns: 2 fixed, 1 added", "kites: 1 fixed"]);
    let ids: Vec<&str> = got[0].lines.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, ["B2", "T3", "B4"]);
}

#[test]
fn test_closed_work_without_an_area_comes_last() {
    let areas = [area("kites", 0)];
    let items = [
        entry("T1", "done", Kind::Work, None),
        entry("T2", "done", Kind::Work, Some("kites")),
    ];
    let heads: Vec<String> = changelog(&items, &areas)
        .iter()
        .map(Section::head)
        .collect();
    assert_eq!(heads, ["kites: 1 added", "no area: 1 added"]);
}

#[test]
fn test_a_pipe_gets_plain_markdown_and_plans_head_their_tickets() {
    let areas = [area("kites", 0)];
    let mut a = entry("T1", "done", Kind::Work, Some("kites"));
    a.plan = Some("Sky plan".to_string());
    let b = entry("T2", "done", Kind::Work, Some("kites"));
    let got = changelog(&[a, b], &areas);
    assert_eq!(
        render(&got, true, false),
        "## kites: 2 added\n\n### Sky plan\n\n- title of T1 (T1)\n- title of T2 (T2)\n\n"
    );
    assert!(render(&got, false, true).contains("\x1b[1m## kites: 2 added"));
    assert!(!render(&got, false, false).contains('\x1b'));
}
