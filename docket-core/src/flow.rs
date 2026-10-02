use std::collections::BTreeMap;

use crate::word::Kind;

/// `{word: count}` over the tickets and `{key: {word: count}}` over every item. A package moves as
/// its tickets do, so the flow counts the tickets alone.
#[must_use]
pub fn tally<'a>(
    rows: impl IntoIterator<Item = (&'a str, Kind, &'a str)>,
) -> (
    BTreeMap<String, u64>,
    BTreeMap<String, BTreeMap<String, u64>>,
) {
    let (mut total, mut by_key) = (
        BTreeMap::new(),
        BTreeMap::<String, BTreeMap<String, u64>>::new(),
    );
    for (key, kind, word) in rows {
        if kind != Kind::Package {
            *total.entry(word.to_string()).or_default() += 1;
        }
        *by_key
            .entry(key.to_string())
            .or_default()
            .entry(word.to_string())
            .or_default() += 1;
    }
    (total, by_key)
}

/// `(basis, choice)` from a derived decision, the choice empty when no `: ` separates them.
#[must_use]
pub fn derived_parts(decision: &str) -> (String, String) {
    let rest = decision.get(DERIVED.len()..).unwrap_or_default();
    match rest.split_once(": ") {
        Some((basis, choice)) => (basis.to_string(), choice.to_string()),
        None => (rest.to_string(), String::new()),
    }
}

pub const DERIVED: &str = "Derived from ";

/// How work starts: a session per role, each with its skill, as many at once as wanted.
pub const GET_GOING: [(&str, &str); 3] = [
    ("/plan", "turn a goal into tickets"),
    ("/work", "take the next ticket to its merge"),
    ("/audit", "check a plan whose tickets closed"),
];

#[cfg(test)]
#[path = "tests/flow.rs"]
mod tests;
