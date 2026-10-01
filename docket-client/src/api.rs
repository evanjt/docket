//! The server's read routes as typed calls, and its change stream as an iterator.

use std::io::{BufRead, BufReader, Read};
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use docket_core::api::{Common, FactRequest, FactSet, Facts};
use docket_core::rows::{Derived, EventRow, ItemRow, LinkRow, ProjectRow, Row, Shown, Status};

use crate::config::Config;

/// The most rows one page of a stored list carries.
pub const PAGE: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The server answered and said no: the status and its words.
    Refused(u16, String),
    /// The server could not be reached, or the answer was not the shape expected.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Refused(code, why) => write!(f, "{code}: {why}"),
            Error::Failed(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Api {
    base: String,
    key: String,
    http: Client,
}

impl Api {
    /// # Errors
    /// The HTTP client could not be built.
    pub fn new(config: &Config) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Failed(e.to_string()))?;
        Ok(Self {
            base: config.server.clone(),
            key: config.key.clone(),
            http,
        })
    }

    fn send(&self, path: &str, query: &[(&str, String)]) -> Result<Response> {
        let resp = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.key)
            .query(query)
            .send()
            .map_err(|e| Error::Failed(format!("{}: {e}", self.base)))?;
        if resp.status().is_success() {
            return Ok(resp);
        }
        Err(refusal(resp))
    }

    /// A write verb, `POST /do/{verb}`, its answer read as `T`.
    ///
    /// # Errors
    /// A refusal, the network, or an answer of another shape.
    pub fn post<T: DeserializeOwned>(&self, verb: &str, body: &impl Serialize) -> Result<T> {
        let resp = self
            .http
            .post(format!("{}/do/{verb}", self.base))
            .bearer_auth(&self.key)
            .header("content-type", "application/json")
            .body(serde_json::to_vec(body).map_err(|e| Error::Failed(e.to_string()))?)
            .send()
            .map_err(|e| Error::Failed(format!("{}: {e}", self.base)))?;
        if !resp.status().is_success() {
            return Err(refusal(resp));
        }
        let text = resp.text().map_err(|e| Error::Failed(e.to_string()))?;
        serde_json::from_str(&text).map_err(|e| Error::Failed(format!("{verb}: {e}")))
    }

    /// One route's answer, read as `T`.
    ///
    /// # Errors
    /// A refusal, the network, or an answer of another shape.
    pub fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let text = self
            .send(path, query)?
            .text()
            .map_err(|e| Error::Failed(e.to_string()))?;
        serde_json::from_str(&text).map_err(|e| Error::Failed(format!("{path}: {e}")))
    }

    /// Every row of a stored list matching a filter, a page at a time.
    ///
    /// # Errors
    /// As `get`.
    pub fn all<T: DeserializeOwned>(
        &self,
        path: &str,
        filter: &Value,
        sort: &str,
    ) -> Result<Vec<T>> {
        let mut out = Vec::new();
        loop {
            let start = out.len();
            let page: Vec<T> = self.get(
                path,
                &[
                    ("filter", filter.to_string()),
                    ("sort", sort.to_string()),
                    ("range", format!("[{start},{}]", start + PAGE - 1)),
                ],
            )?;
            let n = page.len();
            out.extend(page);
            if n < PAGE {
                return Ok(out);
            }
        }
    }

    /// The server's change stream: `hello` once connected, then `change` after each commit.
    ///
    /// # Errors
    /// A refusal or the network.
    pub fn changes(&self) -> Result<Changes> {
        let resp = Client::builder()
            .timeout(None)
            .build()
            .map_err(|e| Error::Failed(e.to_string()))?
            .get(format!("{}/changes", self.base))
            .bearer_auth(&self.key)
            .send()
            .map_err(|e| Error::Failed(format!("{}: {e}", self.base)))?;
        if !resp.status().is_success() {
            return Err(Error::Refused(resp.status().as_u16(), "changes".into()));
        }
        Ok(Changes {
            lines: BufReader::new(Box::new(resp)),
        })
    }
}

/// The event names of a server-sent stream, in order; it ends when the connection does.
pub struct Changes {
    lines: BufReader<Box<dyn Read + Send>>,
}

impl Changes {
    /// A stream over any reader, for a stream that does not come from the network.
    #[must_use]
    pub fn over(reader: Box<dyn Read + Send>) -> Self {
        Self {
            lines: BufReader::new(reader),
        }
    }
}

impl Iterator for Changes {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        let mut name = None;
        loop {
            let mut line = String::new();
            if self.lines.read_line(&mut line).ok()? == 0 {
                return None;
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if let Some(name) = name.take() {
                    return Some(name);
                }
            } else if let Some(n) = line.strip_prefix("event:") {
                name = Some(n.trim().to_string());
            } else if line.starts_with("data:") && name.is_none() {
                name = Some("message".to_string());
            }
        }
    }
}

/// The words of a refused request: a verb's refusal, or any route's error.
fn refusal(resp: Response) -> Error {
    let code = resp.status().as_u16();
    let text = resp.text().unwrap_or_default();
    let why = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| {
            v["refused"]
                .as_str()
                .or_else(|| v["error"].as_str())
                .map(str::to_string)
        })
        .unwrap_or(text);
    Error::Refused(code, why)
}

fn of(slug: &str) -> Vec<(&'static str, String)> {
    vec![("project", slug.to_string())]
}

impl Api {
    /// # Errors
    /// As `get`.
    pub fn projects(&self) -> Result<Vec<ProjectRow>> {
        self.all("/projects", &json!({}), "[\"slug\",\"ASC\"]")
    }

    /// # Errors
    /// As `get`.
    pub fn status(&self, slug: &str) -> Result<Status> {
        self.get("/status", &of(slug))
    }

    /// `/next`, under an item when one is named.
    ///
    /// # Errors
    /// As `get`.
    pub fn next(&self, slug: &str, n: usize, under: Option<&str>) -> Result<Vec<Row>> {
        let mut q = of(slug);
        q.push(("n", n.to_string()));
        if let Some(id) = under {
            q.push(("under", id.to_string()));
        }
        self.get("/next", &q)
    }

    /// A list route that takes the project alone: `todo`, `questions`, `wip`, `waiting`, `research`,
    /// `groups`, `done`, `dropped`.
    ///
    /// # Errors
    /// As `get`.
    pub fn list(&self, route: &str, slug: &str) -> Result<Vec<Row>> {
        self.get(&format!("/{route}"), &of(slug))
    }

    /// The items of one group, by rank and number.
    ///
    /// # Errors
    /// As `get`.
    pub fn group(&self, slug: &str, name: &str) -> Result<Vec<Row>> {
        let mut q = of(slug);
        q.push(("name", name.to_string()));
        self.get("/groups", &q)
    }

    /// # Errors
    /// As `get`.
    pub fn search(&self, slug: &str, words: &str, n: usize) -> Result<Vec<Row>> {
        let mut q = of(slug);
        q.push(("q", words.to_string()));
        q.push(("n", n.to_string()));
        q.push(("state", "any".to_string()));
        self.get("/search", &q)
    }

    /// # Errors
    /// As `get`.
    pub fn show(&self, slug: &str, id: &str) -> Result<Shown> {
        self.get(&format!("/show/{id}"), &of(slug))
    }

    /// The facts a project sets, and when the loop last ticked for it.
    ///
    /// # Errors
    /// As `get`.
    pub fn facts(&self, slug: &str) -> Result<Facts> {
        self.get("/facts", &of(slug))
    }

    /// One fact set, or unset by an empty value.
    ///
    /// # Errors
    /// As `post`.
    pub fn set_fact(&self, slug: &str, key: &str, value: &str) -> Result<FactSet> {
        let req = FactRequest {
            common: Common {
                project: slug.to_string(),
                ..Common::default()
            },
            key: key.to_string(),
            value: value.to_string(),
        };
        self.post("fact", &req)
    }

    /// # Errors
    /// As `get`.
    pub fn derived(&self, slug: &str) -> Result<Vec<Derived>> {
        self.get("/derived", &of(slug))
    }

    /// Every stored item of a project, bodies left out.
    ///
    /// # Errors
    /// As `get`.
    pub fn items(&self, slug: &str) -> Result<Vec<ItemRow>> {
        self.all("/items", &json!({ "project": slug }), "[\"rid\",\"ASC\"]")
    }

    /// The links of one kind leaving any of the rids.
    ///
    /// # Errors
    /// As `get`.
    pub fn links_from(&self, rids: &[i64], kind: &str) -> Result<Vec<LinkRow>> {
        let mut out = Vec::new();
        for chunk in rids.chunks(PAGE) {
            out.extend(self.all::<LinkRow>(
                "/links",
                &json!({ "rid": chunk, "kind": kind }),
                "[\"rowid\",\"ASC\"]",
            )?);
        }
        Ok(out)
    }

    /// The links of one kind reaching any of the rids.
    ///
    /// # Errors
    /// As `get`.
    pub fn links_to(&self, rids: &[i64], kind: &str) -> Result<Vec<LinkRow>> {
        let mut out = Vec::new();
        for chunk in rids.chunks(PAGE) {
            out.extend(self.all::<LinkRow>(
                "/links",
                &json!({ "to_rid": chunk, "kind": kind }),
                "[\"rowid\",\"ASC\"]",
            )?);
        }
        Ok(out)
    }

    /// One item's events, oldest first.
    ///
    /// # Errors
    /// As `get`.
    pub fn log(&self, rid: i64) -> Result<Vec<EventRow>> {
        self.all("/events", &json!({ "rid": rid }), "[\"seq\",\"ASC\"]")
    }

    /// A project's newest events of the given kinds, newest first.
    ///
    /// # Errors
    /// As `get`.
    pub fn recent(&self, slug: &str, kinds: &[&str], n: usize) -> Result<Vec<EventRow>> {
        self.get(
            "/events",
            &[
                (
                    "filter",
                    json!({ "project": slug, "kind": kinds }).to_string(),
                ),
                ("sort", "[\"seq\",\"DESC\"]".to_string()),
                ("range", format!("[0,{}]", n.max(1) - 1)),
            ],
        )
    }
}

#[cfg(test)]
#[path = "tests/api.rs"]
mod tests;
