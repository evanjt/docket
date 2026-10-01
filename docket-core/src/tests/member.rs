use super::*;

fn tie(rid: i64, opened: bool, to: i64) -> Tie {
    Tie { rid, opened, to }
}

fn set(rids: &[i64]) -> HashSet<i64> {
    rids.iter().copied().collect()
}

#[test]
fn test_opened_under_reaches_any_depth_and_skips_cycles() {
    let ties = [
        tie(2, true, 1),
        tie(3, true, 2),
        tie(1, true, 3),
        tie(4, false, 1),
    ];
    assert_eq!(opened_under(&ties, 1), set(&[2, 3]));
    assert_eq!(opened_under(&ties, 4), set(&[]));
}

#[test]
fn test_members_of_follows_both_directions_and_drops_standing() {
    // 10 is a concept; 1 relates to it, 2 was opened under 1, 3 opened the concept, 11 is another concept.
    let ties = [
        tie(1, false, 10),
        tie(2, true, 1),
        tie(3, true, 10),
        tie(11, false, 10),
        tie(5, true, 3),
    ];
    let standing = set(&[10, 11]);
    assert_eq!(members_of(&ties, &standing, 10), set(&[1, 2, 3, 5]));
    assert_eq!(members_of(&ties, &standing, 11), set(&[]));
}

#[test]
fn test_members_of_survives_an_opened_cycle_and_an_empty_graph() {
    let ties = [tie(1, true, 2), tie(2, true, 1), tie(1, false, 9)];
    let standing = set(&[9]);
    assert_eq!(members_of(&ties, &standing, 9), set(&[1, 2]));
    assert_eq!(members_of(&[], &standing, 9), set(&[]));
}

#[test]
fn test_concepts_of_reads_ties_either_way_and_up_through_openers() {
    // 10 and 11 are concepts. 1 relates to 10; 2 was opened by 1; 3 opened 2 and is tied from 11;
    // 4 stands alone.
    let ties = [
        tie(1, false, 10),
        tie(2, true, 1),
        tie(2, true, 3),
        tie(11, false, 3),
    ];
    let index = Neighbours::new(&ties);
    let concepts = set(&[10, 11]);
    assert_eq!(index.concepts_of(&concepts, 1), set(&[10]));
    assert_eq!(index.concepts_of(&concepts, 2), set(&[10, 11]));
    assert_eq!(index.concepts_of(&concepts, 4), set(&[]));
}

#[test]
fn test_concepts_of_a_concept_leaves_itself_out_and_stops_at_a_cycle() {
    let ties = [tie(10, false, 11), tie(1, true, 2), tie(2, true, 1)];
    let index = Neighbours::new(&ties);
    let concepts = set(&[10, 11]);
    assert_eq!(index.concepts_of(&concepts, 10), set(&[11]));
    assert_eq!(index.concepts_of(&concepts, 1), set(&[]));
}
