use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{StatusCode, header::AUTHORIZATION};
use axum::middleware::Next;
use axum::response::Response;
use subtle::ConstantTimeEq;

/// Who a key belongs to: the machine it was issued to, and whether it is the owner's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caller {
    pub host: String,
    pub owner: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Keys(Vec<(String, Caller)>);

impl Keys {
    /// Reads `host role key` lines, role being `owner` or `agent`. Blank lines and `#` lines are skipped.
    ///
    /// # Errors
    /// A line that is not three fields, or whose role is neither word.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut keys = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [host, role, key] = fields[..] else {
                return Err(format!("line {}: expected `host role key`", n + 1));
            };
            let owner = match role {
                "owner" => true,
                "agent" => false,
                _ => {
                    return Err(format!(
                        "line {}: role is owner or agent, not {role}",
                        n + 1
                    ));
                }
            };
            keys.push((
                key.to_string(),
                Caller {
                    host: host.to_string(),
                    owner,
                },
            ));
        }
        Ok(Self(keys))
    }

    /// The caller a presented key belongs to. Every stored key is compared, in constant time each.
    #[must_use]
    pub fn caller(&self, presented: &str) -> Option<&Caller> {
        let mut found = None;
        for (key, caller) in &self.0 {
            if bool::from(key.as_bytes().ct_eq(presented.as_bytes())) {
                found = Some(caller);
            }
        }
        found
    }
}

/// Refuses a request without a known bearer key, and hands the handler its `Caller`.
///
/// # Errors
/// 401 when the header is missing, malformed or names no key.
pub async fn require_key(
    State(keys): State<Arc<Keys>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let presented = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let caller = keys
        .caller(presented)
        .cloned()
        .ok_or(StatusCode::UNAUTHORIZED)?;
    req.extensions_mut().insert(caller);
    Ok(next.run(req).await)
}

#[cfg(test)]
#[path = "tests/auth.rs"]
mod tests;
