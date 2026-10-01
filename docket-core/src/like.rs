/// SQLite's `LIKE`: `%` spans anything, `_` one character, letters compared without ASCII case.
#[must_use]
pub fn like(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    matches(&p, &t)
}

fn matches(p: &[char], t: &[char]) -> bool {
    let Some((&head, rest)) = p.split_first() else {
        return t.is_empty();
    };
    if head == '%' {
        return (0..=t.len()).any(|i| matches(rest, &t[i..]));
    }
    let Some((&c, tail)) = t.split_first() else {
        return false;
    };
    (head == '_' || head.eq_ignore_ascii_case(&c)) && matches(rest, tail)
}

/// Whether the text contains the words, as `LIKE '%words%'` reads it.
#[must_use]
pub fn contains(text: &str, words: &str) -> bool {
    like(&format!("%{words}%"), text)
}

#[cfg(test)]
#[path = "tests/like.rs"]
mod tests;
