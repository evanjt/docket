const STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "of", "to", "in", "on", "for", "with", "is", "are", "was",
    "were", "be", "by", "at", "as", "it", "its", "this", "that", "from", "not", "no", "but", "if",
    "so", "than", "then", "when", "which", "who", "what", "into", "over", "under", "one", "two",
];

fn quoted(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// Every word of the text as an FTS5 phrase, all required; `None` when there are no words.
#[must_use]
pub fn fts_query(text: &str) -> Option<String> {
    let words: Vec<String> = text.split_whitespace().map(quoted).collect();
    if words.is_empty() {
        return None;
    }
    Some(words.join(" "))
}

/// An FTS5 query matching any of the terms.
#[must_use]
pub fn any_of(terms: &[String]) -> String {
    terms
        .iter()
        .map(|t| quoted(t))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// An id: one to three capitals and a number.
#[must_use]
pub fn is_id(text: &str) -> bool {
    let letters = text.bytes().take_while(u8::is_ascii_uppercase).count();
    (1..=3).contains(&letters)
        && text.len() > letters
        && text.bytes().skip(letters).all(|b| b.is_ascii_digit())
}

/// Words of at least four characters in a title, stop words aside.
fn title_words(title: &str) -> Vec<String> {
    let chars: Vec<char> = title.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len()
            && (chars[j].is_ascii_alphanumeric() || chars[j] == '_' || chars[j] == '\'')
        {
            j += 1;
        }
        if j - i >= 4 {
            let word: String = chars[i..j].iter().collect();
            if !STOP.contains(&word.to_lowercase().as_str()) {
                out.push(word);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Backticked symbols in a body, each cut to its last path segment; ids are skipped.
fn body_symbols(body: &str) -> Vec<String> {
    let chars: Vec<char> = body.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '`'
            || !chars
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == '_')
        {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len()
            && (chars[j].is_ascii_alphanumeric() || matches!(chars[j], '_' | '.' | ':'))
        {
            j += 1;
        }
        if chars.get(j) != Some(&'`') {
            i += 1;
            continue;
        }
        let sym: String = chars[i + 1..j].iter().collect();
        if !is_id(&sym) {
            let last = sym.rsplit('.').next().unwrap_or_default();
            out.push(last.rsplit("::").next().unwrap_or_default().to_string());
        }
        i = j + 1;
    }
    out
}

/// The terms an item is close to another by: its title's words, the files it cites, its symbols.
#[must_use]
pub fn similar_terms(title: &str, cited_paths: &[String], body: &str) -> Vec<String> {
    let mut terms = title_words(title);
    terms.extend(
        cited_paths
            .iter()
            .map(|p| p.rsplit('/').next().unwrap_or_default().to_string()),
    );
    terms.extend(body_symbols(body));
    let mut seen = std::collections::HashSet::new();
    terms
        .into_iter()
        .filter(|t| {
            let k = t.to_lowercase();
            k.chars().count() > 2 && seen.insert(k)
        })
        .take(30)
        .collect()
}

#[cfg(test)]
#[path = "tests/search.rs"]
mod tests;
