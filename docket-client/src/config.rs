//! Where the server is and the key a client presents: `DOCKET_SERVER` and `DOCKET_KEY`, else
//! `~/.config/docket/client`.

use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub server: String,
    pub key: String,
}

/// `$XDG_CONFIG_HOME/docket/client`, or `~/.config/docket/client`.
#[must_use]
pub fn path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("docket").join("client"))
}

/// One value from `name = value` lines; blank lines and `#` lines are skipped.
#[must_use]
pub fn field(text: &str, name: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == name)
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Config {
    /// The environment first, then the file, each value on its own.
    ///
    /// # Errors
    /// Either value missing from both, named with where to set it.
    pub fn load() -> Result<Self, String> {
        let file = path().and_then(|p| std::fs::read_to_string(p).ok());
        Self::resolve(
            env::var("DOCKET_SERVER").ok(),
            env::var("DOCKET_KEY").ok(),
            file.as_deref(),
        )
    }

    /// The pure half of `load`: the environment's values over the file's text.
    ///
    /// # Errors
    /// Either value missing from both.
    pub fn resolve(
        server: Option<String>,
        key: Option<String>,
        file: Option<&str>,
    ) -> Result<Self, String> {
        let from_file = |name| file.and_then(|t| field(t, name));
        let server = server
            .filter(|v| !v.is_empty())
            .or_else(|| from_file("server"));
        let key = key.filter(|v| !v.is_empty()).or_else(|| from_file("key"));
        let shown = path().map_or_else(
            || "~/.config/docket/client".to_string(),
            |p| p.display().to_string(),
        );
        match (server, key) {
            (Some(server), Some(key)) => Ok(Self {
                server: server.trim_end_matches('/').to_string(),
                key,
            }),
            (None, _) => Err(format!(
                "no server: set DOCKET_SERVER, or `server = http://host:port` in {shown}"
            )),
            (_, None) => Err(format!("no key: set DOCKET_KEY, or `key = ...` in {shown}")),
        }
    }
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
