use ratatui::style::Style;

use super::*;

fn known(id: &str) -> Option<Style> {
    ["T1", "PK1", "Q1"].contains(&id).then(Style::default)
}

#[test]
fn test_ids_in_finds_whole_ids_only() {
    let text = "See T1, PK1 and B14x; not SHA256x or abc12 or T, but CON1.";
    let found: Vec<&str> = ids_in(text).into_iter().map(|(a, b)| &text[a..b]).collect();
    assert_eq!(found, ["T1", "PK1", "CON1"]);
    assert!(ids_in("").is_empty());
}

#[test]
fn test_linked_makes_spots_of_known_ids_only() {
    let segs = linked("T1 waits on X9 and Q1.", Style::default(), &known);
    let spots: Vec<&str> = segs
        .iter()
        .filter(|s| s.target.is_some())
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(spots, ["T1", "Q1"]);
    let all: String = segs.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(all, "T1 waits on X9 and Q1.");
}

#[test]
fn test_wrap_keeps_indent_and_hangs_a_list_item() {
    let lines = wrap("- one two three four five six\n\n  deep words here", 14);
    assert_eq!(
        lines,
        [
            "- one two",
            "  three four",
            "  five six",
            "",
            "  deep words",
            "  here"
        ]
    );
}

#[test]
fn test_wrap_leaves_a_long_word_whole() {
    assert_eq!(wrap("abcdefghijklmnop q", 8), ["abcdefghijklmnop", "q"]);
}

fn spots() -> Vec<(usize, Target)> {
    let mut d = Doc::default();
    d.prose("T1 then PK1\n\n\n\nQ1", 40, &known);
    d.spots()
}

#[test]
fn test_cursor_steps_round_and_follows() {
    let s = spots();
    let mut c = Cursor::default();
    c.step(&s, false);
    assert_eq!(c.selected(), Some(&Target::Item("Q1".into())));
    c.step(&s, true);
    assert_eq!(c.selected(), Some(&Target::Item("T1".into())));
    c.step(&s, false);
    c.follow(&s, 2);
    // Q1 sits on line 4, so a window of two starts at line 3.
    assert_eq!(c.top, 3);
}

#[test]
fn test_cursor_settles_on_the_same_target_after_a_refetch() {
    let s = spots();
    let mut c = Cursor::default();
    c.step(&s, true);
    c.step(&s, true);
    assert_eq!(c.selected(), Some(&Target::Item("PK1".into())));
    let fewer: Vec<(usize, Target)> = s
        .into_iter()
        .filter(|(_, t)| *t != Target::Item("T1".into()))
        .collect();
    c.settle(&fewer);
    assert_eq!(
        (c.index, c.selected()),
        (Some(0), Some(&Target::Item("PK1".into())))
    );
    c.settle(&[]);
    assert_eq!(c.selected(), None);
}

#[test]
fn test_render_reverses_the_selected_spot_only() {
    let mut d = Doc::default();
    d.prose("T1 and Q1", 40, &known);
    let lines = d.render(Some(1));
    let spans = &lines[0].spans;
    assert!(!spans[0].style.add_modifier.contains(Modifier::REVERSED));
    assert!(spans[0].style.add_modifier.contains(Modifier::UNDERLINED));
    assert!(spans[2].style.add_modifier.contains(Modifier::REVERSED));
}

#[test]
fn test_places_give_each_spot_its_line_column_and_width() {
    let mut d = Doc::default();
    d.prose("T1 then PK1\n\nsee Q1", 40, &known);
    assert_eq!(d.places(), [(0, 0, 2), (0, 8, 3), (2, 4, 2)]);
}

#[test]
fn test_cursor_release_drops_a_spot_scrolled_out_of_view() {
    let s = spots();
    let mut c = Cursor::default();
    c.step(&s, true);
    c.scroll(1, 5, 2);
    c.release(&s, 2);
    assert_eq!((c.top, c.selected()), (1, None));
    c.step(&s, false);
    c.release(&s, 4);
    assert_eq!(c.selected(), Some(&Target::Item("Q1".into())));
}
