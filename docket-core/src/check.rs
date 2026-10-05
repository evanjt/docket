//! The problems `check` finds, as the server lists them: objects with a `kind`, counted and named
//! the same way wherever they are shown.

use serde_json::Value;

/// Problems per kind, in the order each kind first appears.
#[must_use]
pub fn counts(problems: &Value) -> Vec<(String, u64)> {
    let mut counts: Vec<(String, u64)> = Vec::new();
    for p in problems.as_array().into_iter().flatten() {
        let kind = p["kind"].as_str().unwrap_or("other");
        match counts.iter_mut().find(|(k, _)| k == kind) {
            Some((_, n)) => *n += 1,
            None => counts.push((kind.to_string(), 1)),
        }
    }
    counts
}

/// `5 cycles`: a count of one kind of problem, in the singular for one.
#[must_use]
pub fn phrase(kind: &str, n: u64) -> String {
    let (one, many) = match kind {
        "conflict" => ("sync conflict", "sync conflicts"),
        "undefined_key" => ("undefined key", "undefined keys"),
        "integrity" => ("integrity failure", "integrity failures"),
        "foreign_keys" => ("foreign key violation", "foreign key violations"),
        "cycle" => ("cycle", "cycles"),
        "held_later" => ("release inversion", "release inversions"),
        "held_gate" => ("hold on closed work", "holds on closed work"),
        "stale_wait" => ("stale wait", "stale waits"),
        "no_body" => ("item with no body", "items with no body"),
        _ => ("other problem", "other problems"),
    };
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The ids of the items the problems of one kind name, in order, each once.
#[must_use]
pub fn ids(problems: &[Value], kind: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in problems
        .iter()
        .filter(|p| p["kind"].as_str().unwrap_or("other") == kind)
    {
        if let Some(id) = p["id"].as_str().filter(|id| !out.iter().any(|o| o == id)) {
            out.push(id.to_string());
        }
    }
    out
}

#[cfg(test)]
#[path = "tests/check.rs"]
mod tests;
