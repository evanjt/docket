//! Carrying a closed item's leading sha onto the commit a history rewrite made of it.

use std::collections::BTreeMap;

/// The shortest sha a resolution leads with that is read as one: what `git rev-parse --short` gives.
pub const LEAST: usize = 7;

/// The old shas a map names, each with its new one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Map(BTreeMap<String, String>);

/// One resolution rewritten: the sha it led with, the one it leads with now, and the text after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remapped {
    pub old: String,
    pub new: String,
    pub resolution: String,
}

impl Map {
    /// Lines of `old new`, hex shas; blank lines and the `old new` header a filter-repo
    /// `commit-map` begins with are skipped.
    ///
    /// # Errors
    /// A line that is not two hex shas, naming it.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut map = BTreeMap::new();
        for (n, line) in text.lines().enumerate() {
            let mut parts = line.split_whitespace();
            let (Some(old), Some(new), None) = (parts.next(), parts.next(), parts.next()) else {
                if line.trim().is_empty() {
                    continue;
                }
                return Err(format!("line {}: expected `old new`, got {line:?}", n + 1));
            };
            if (old, new) == ("old", "new") {
                continue;
            }
            for sha in [old, new] {
                if sha.len() < LEAST || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(format!("line {}: {sha:?} is not a sha", n + 1));
                }
            }
            map.insert(old.to_ascii_lowercase(), new.to_ascii_lowercase());
        }
        Ok(Self(map))
    }

    #[must_use]
    pub fn from_pairs(pairs: &[(String, String)]) -> Self {
        Self(
            pairs
                .iter()
                .map(|(o, n)| (o.to_ascii_lowercase(), n.to_ascii_lowercase()))
                .collect(),
        )
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The pairs, for a caller to send or check against the repository.
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(o, n)| (o.as_str(), n.as_str()))
    }

    /// The new sha of the one old sha the prefix names; none when it names none or several.
    fn find(&self, prefix: &str) -> Option<&str> {
        let mut hits = self
            .0
            .range(prefix.to_string()..)
            .take_while(|(old, _)| old.starts_with(prefix));
        let (_, new) = hits.next()?;
        hits.next().is_none().then_some(new.as_str())
    }

    /// The resolution with its leading sha replaced by the new one at the same length, the text
    /// after it as it was. None when it leads with no sha, or one the map does not name once.
    #[must_use]
    pub fn remap(&self, resolution: &str) -> Option<Remapped> {
        let lead = resolution.bytes().take_while(u8::is_ascii_hexdigit).count();
        let after = resolution.as_bytes().get(lead);
        if lead < LEAST || after.is_some_and(u8::is_ascii_alphanumeric) {
            return None;
        }
        let old = &resolution[..lead];
        let new = self.find(&old.to_ascii_lowercase())?.get(..lead)?;
        (new != old.to_ascii_lowercase()).then(|| Remapped {
            old: old.to_string(),
            new: new.to_string(),
            resolution: format!("{new}{}", &resolution[lead..]),
        })
    }
}

#[cfg(test)]
#[path = "tests/remap.rs"]
mod tests;
