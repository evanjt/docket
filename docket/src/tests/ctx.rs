use super::*;

#[test]
fn test_id_reads_any_case_and_leading_zeros() {
    assert_eq!(id(" b07 ").unwrap(), "B7");
    assert_eq!(id("STY12").unwrap(), "STY12");
}

#[test]
fn test_id_refuses_what_is_not_one_in_pythons_words() {
    assert_eq!(
        id("zz").unwrap_err(),
        Fail::refused("'zz' is not an id: a key of one to three capitals and a number, like B14")
    );
}
