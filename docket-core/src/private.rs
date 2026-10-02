//! What a public repository must never carry: the terms that name the owner's private work, as the
//! server knows them, and where a text carries one. The terms are read from the server at check time
//! and never written down, so no list of them can leak.

use serde::{Deserialize, Serialize};

/// The private data the terms come from: `GET /private`, on the owner's key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Private {
    /// Every project's slug.
    pub projects: Vec<String>,
    /// Every name a project's `owner` fact gives.
    pub owners: Vec<String>,
    /// Every machine's name and ssh address.
    pub machines: Vec<(String, String)>,
    /// Every host a key, an event or a claim names.
    pub hosts: Vec<String>,
    /// Every key in any project's matrix, the letters item ids start with.
    pub keys: Vec<String>,
}

/// Shorter than this, a term is too common a word to tell anything apart.
const SHORTEST: usize = 3;

/// The terms to look for: each slug and its parts, each owner and their names, each machine and the
/// user and host in its address, each host and its first label, and what `local` adds (this machine's
/// user name and home). A term `allowed` names, in any case, is left out. Longest first.
#[must_use]
pub fn terms(private: &Private, local: &[String], allowed: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for slug in &private.projects {
        out.push(slug.clone());
        out.extend(slug.split('/').map(str::to_string));
    }
    for owner in &private.owners {
        out.push(owner.clone());
        out.extend(owner.split_whitespace().map(str::to_string));
    }
    for (name, ssh) in &private.machines {
        out.extend(host_terms(name));
        let addr = ssh.strip_prefix("ssh://").unwrap_or(ssh);
        let (user, host) = addr.split_once('@').map_or(("", addr), |(u, h)| (u, h));
        let host = host
            .rsplit_once(':')
            .filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
            .map_or(host, |(h, _)| h);
        out.push(user.to_string());
        out.extend(host_terms(host));
    }
    for host in &private.hosts {
        out.extend(host_terms(host));
    }
    out.extend(local.iter().cloned());
    let allowed: Vec<String> = allowed.iter().map(|a| a.to_lowercase()).collect();
    out.retain(|t| t.chars().count() >= SHORTEST && !allowed.contains(&t.to_lowercase()));
    out.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    out.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    out
}

/// A host and, unless it is an address, its first label.
fn host_terms(host: &str) -> Vec<String> {
    let mut out = vec![host.to_string()];
    let numeric = host.chars().all(|c| c.is_ascii_digit() || c == '.');
    if !numeric && let Some((first, _)) = host.split_once('.') {
        out.push(first.to_string());
    }
    out
}

/// The terms a line carries, each once, matched in any case where neither neighbour is a letter or a
/// digit, so `alpha` is found in `alpha-1` and `alpha.local` but not in `alphabet`.
#[must_use]
pub fn hits<'a>(terms: &'a [String], line: &str) -> Vec<&'a str> {
    let lower = line.to_lowercase();
    let mut out = Vec::new();
    for term in terms {
        let t = term.to_lowercase();
        let found = lower.match_indices(&t).any(|(at, _)| {
            let before = lower[..at].chars().next_back();
            let after = lower[at + t.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        });
        if found {
            out.push(term.as_str());
        }
    }
    out
}

/// Whether a line of a file is a comment, where a citation of a docket item would sit: a line
/// starting with a comment marker, or one with a `//` comment after code.
#[must_use]
pub fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    ["//", "#", "*", "/*", "<!--", "--"]
        .iter()
        .any(|m| t.starts_with(m))
        || line.contains(" // ")
}

/// The item ids a line cites: a key from any project's matrix followed directly by a number, standing
/// alone, as `A7` or `PK12`.
#[must_use]
pub fn ids(keys: &[String], line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let starts = i == 0 || !chars[i - 1].is_alphanumeric();
        if starts && chars[i].is_ascii_uppercase() {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_uppercase() {
                j += 1;
            }
            let mut k = j;
            while k < chars.len() && chars[k].is_ascii_digit() {
                k += 1;
            }
            let ends = k == chars.len() || !chars[k].is_alphanumeric();
            let key: String = chars[i..j].iter().collect();
            if k > j && ends && keys.contains(&key) {
                out.push(chars[i..k].iter().collect());
            }
            i = k.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
#[path = "tests/private.rs"]
mod tests;
