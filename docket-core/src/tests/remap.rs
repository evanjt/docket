use super::*;

const A: &str = "aaaaaaaaa1111111111111111111111111111111";
const B: &str = "bbbbbbbbb2222222222222222222222222222222";
const A2: &str = "aaaaaaaaa9999999999999999999999999999999";
const X: &str = "1234567890abcdef1234567890abcdef12345678";
const Y: &str = "fedcba0987654321fedcba0987654321fedcba09";

fn map() -> Map {
    Map::parse(&format!("old new\n{A} {X}\n\n{B} {Y}\n")).unwrap()
}

#[test]
fn a_prefix_is_replaced_at_its_own_length_and_the_text_stays() {
    let r = map().remap("aaaaaaaaa11 and the rest").unwrap();
    assert_eq!(r.old, "aaaaaaaaa11");
    assert_eq!(r.new, "1234567890a");
    assert_eq!(r.resolution, "1234567890a and the rest");
}

#[test]
fn a_full_sha_and_a_trailing_bracket_read_as_the_sha() {
    let r = map().remap(&format!("{B}) merged")).unwrap();
    assert_eq!(r.resolution, format!("{Y}) merged"));
}

#[test]
fn an_ambiguous_prefix_a_short_sha_and_an_unmapped_one_are_left() {
    let two = Map::parse(&format!("{A} {X}\n{A2} {Y}\n")).unwrap();
    assert_eq!(two.remap("aaaaaaaaa rest"), None);
    assert!(two.remap(&format!("{A2} rest")).is_some());
    assert_eq!(map().remap("aaaaaaaa rest"), None);
    assert_eq!(map().remap("ccccccccc rest"), None);
    assert_eq!(map().remap("superseded by B1"), None);
    assert_eq!(map().remap("aaaaaaaaaxyz"), None);
}

#[test]
fn a_map_line_that_is_no_pair_of_shas_is_refused() {
    assert!(Map::parse("aaaaaaaaa").is_err());
    assert!(Map::parse("aaaaaaaaa zzzzzzzzz").is_err());
    assert!(Map::parse("a b c").is_err());
}
