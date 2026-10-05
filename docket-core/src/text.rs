//! What a body and a resolution say: citations, principles, the ids a close opened.

use std::fmt::Write;
use std::sync::LazyLock;

use regex::Regex;

use crate::item::Refused;

pub const GATES: [&str; 3] = ["passed", "failed", "skipped"];

static GATES_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(passed|failed|skipped)\b").unwrap());
static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([A-Z]{1,3})(\d+)$").unwrap());
// Longest first, since the alternation is first-match.
static CITE: LazyLock<Regex> = LazyLock::new(|| {
    let mut exts: Vec<&str> =
        "rs ts tsx svelte sql R r json yaml yml toml js jsx md py kt java swift dart sh mjs"
            .split(' ')
            .collect();
    exts.sort_by_key(|e| std::cmp::Reverse(e.len()));
    // The extension has to end where it ends: what follows it is a line number, a non-word
    // character, or the closing backtick.
    Regex::new(&format!(
        "`([A-Za-z0-9_./()-]+\\.(?:{}))(?::([0-9]+)[^`]*|[^A-Za-z0-9`][^`]*)?`",
        exts.join("|")
    ))
    .unwrap()
});
static TEST_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|/)(tests?|__tests__|spec)(/|$)|\.test\.|\.spec\.|_test\.").unwrap()
});
static BULLET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*-\s+\*\*([A-Za-z][^.*]*)\.?\*\*").unwrap());
static ORIGINAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#{1,3}\s+Original\b").unwrap());
static PRINCIPLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(\d+)\.\s+(.*)$").unwrap());

/// A gates run's result, checked: it opens with passed, failed or skipped.
///
/// # Errors
/// Refused when it opens with anything else.
pub fn gates_result(text: Option<&str>) -> Result<String, Refused> {
    let text = text.unwrap_or("").trim();
    if !GATES_RE.is_match(text) {
        return Err(Refused(format!(
            "a gates result opens with passed, failed or skipped, as \"skipped: lock held\". Got \"{text}\"."
        )));
    }
    Ok(text.to_string())
}

/// (key, number) of an id written in any case, with the spaces around it.
///
/// # Errors
/// Refused when the text is not a key of one to three capitals and a number.
pub fn split_id(text: &str) -> Result<(String, i64), Refused> {
    let upper = text.trim().to_uppercase();
    let not_an_id = || {
        Refused(format!(
            "{} is not an id: a key of one to three capitals and a number, like B14",
            py_repr(text)
        ))
    };
    let m = ID.captures(&upper).ok_or_else(not_an_id)?;
    let num = m[2].parse().map_err(|_| not_an_id())?;
    Ok((m[1].to_string(), num))
}

/// A string as Python's repr prints it, for refusals that quote one.
#[must_use]
pub fn py_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// One backticked path in a body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Citation {
    pub path: String,
    pub line: Option<i64>,
    pub kind: &'static str,
}

/// Every backticked path in the body, skipping Fix bullets and anything under an Original heading.
#[must_use]
pub fn citations(body: &str) -> Vec<Citation> {
    let mut out: Vec<Citation> = Vec::new();
    let mut label: Option<String> = None;
    for line in body.split('\n') {
        if ORIGINAL.is_match(line) {
            break;
        }
        if let Some(m) = BULLET.captures(line) {
            label = Some(m[1].trim().to_lowercase());
        } else if line.trim().is_empty() || line.starts_with('#') || line.starts_with("**") {
            label = None;
        }
        if label.as_deref() == Some("fix") {
            continue;
        }
        for m in CITE.captures_iter(line) {
            let path = m[1].to_string();
            let line_no = m.get(2).and_then(|n| n.as_str().parse().ok());
            let kind = if TEST_PATH.is_match(&path) {
                "cites_test"
            } else {
                "cites_file"
            };
            let cite = Citation {
                path,
                line: line_no,
                kind,
            };
            if !out.contains(&cite) {
                out.push(cite);
            }
        }
    }
    out
}

/// (number, text) for the numbered list under a Principles bullet, in the order written.
#[must_use]
pub fn principles(body: &str) -> Vec<(u64, String)> {
    let mut out: Vec<(u64, String)> = Vec::new();
    let mut inside = false;
    let mut last: Option<usize> = None;
    for line in body.split('\n') {
        if let Some(m) = BULLET.captures(line) {
            inside = m[1].trim().eq_ignore_ascii_case("principles");
            last = None;
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(p) = PRINCIPLE.captures(line) {
            let Ok(n) = p[1].parse::<u64>() else {
                continue;
            };
            let text = p[2].trim().to_string();
            let i = if let Some(i) = out.iter().position(|(k, _)| *k == n) {
                out[i].1 = text;
                i
            } else {
                out.push((n, text));
                out.len() - 1
            };
            last = Some(i);
        } else if let Some(i) = last
            && line.starts_with("   ")
            && !line.trim().is_empty()
        {
            out[i].1.push(' ');
            out[i].1.push_str(line.trim());
        }
    }
    out
}

fn key_alternation(keys: &[&str]) -> String {
    let mut keys: Vec<&str> = keys.to_vec();
    keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
    keys.iter()
        .map(|k| regex::escape(k))
        .collect::<Vec<_>>()
        .join("|")
}

/// Every backticked id of one of the project's keys, sorted.
#[must_use]
pub fn mentioned_ids(text: &str, keys: &[&str]) -> Vec<String> {
    let Ok(pat) = Regex::new(&format!("`({})(\\d+)`", key_alternation(keys))) else {
        return Vec::new();
    };
    let mut out: Vec<String> = pat
        .captures_iter(text)
        .map(|m| format!("{}{}", &m[1], &m[2]))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The principle numbers of `item_id` that the text cites as `A1#3`, each once, in order.
#[must_use]
pub fn principle_refs(text: &str, item_id: &str) -> Vec<u64> {
    let mark = format!("{item_id}#");
    let mut out: Vec<u64> = Vec::new();
    for (at, _) in text.match_indices(&mark) {
        let before = text[..at].chars().next_back();
        if before.is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
            continue;
        }
        let digits: String = text[at + mark.len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(n) = digits.parse::<u64>() {
            out.push(n);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
#[path = "tests/text.rs"]
mod tests;
