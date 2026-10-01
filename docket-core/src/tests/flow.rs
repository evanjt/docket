use super::*;

#[test]
fn test_tally_counts_tickets_in_total_and_every_item_by_key() {
    let (total, by_key) = tally([
        ("B", Kind::Work, "ready"),
        ("B", Kind::Work, "done"),
        ("PK", Kind::Package, "ready"),
        ("Q", Kind::Decision, "parked"),
    ]);
    assert_eq!(total["ready"], 1);
    assert_eq!(total["done"], 1);
    assert_eq!(total["parked"], 1);
    assert_eq!(total.values().sum::<u64>(), 3);
    assert_eq!(by_key["PK"]["ready"], 1);
    assert_eq!(by_key["B"]["ready"], 1);
}

#[test]
fn test_derived_parts_splits_basis_from_choice() {
    assert_eq!(
        derived_parts("Derived from Q7, no retries: keep one"),
        ("Q7, no retries".to_string(), "keep one".to_string())
    );
    assert_eq!(
        derived_parts("Derived from CID3"),
        ("CID3".to_string(), String::new())
    );
    assert_eq!(derived_parts("short"), (String::new(), String::new()));
}
