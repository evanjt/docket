//! The server's read routes as typed calls, and its change stream as an iterator.

use std::io::{BufRead, BufReader, Read};
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use docket_core::api::{
    Common, FactRequest, FactSet, Facts, LeadRequest, LeadState, MachineRequest, Machines,
    PublicationRequest, Publications,
};
use docket_core::assignment::Held;
use docket_core::metrics::ReleaseRow;
use docket_core::rows::{
    Derived, EventRow, HeldRow, ItemRow, LinkRow, ProjectRow, Row, Shown, Status,
};

use crate::config::Config;

/// The most rows one page of a stored list carries.
pub const PAGE: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The server answered and said no: the status and its words.
    Refused(u16, String),
    /// The server could not be reached, or a read's answer was not the shape expected.
    Failed(String),
    /// A write was answered 2xx in a shape this client cannot read, so it has landed.
    Unreadable(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Refused(code, why) => write!(f, "{code}: {why}"),
            Error::Failed(why) | Error::Unreadable(why) => write!(f, "{why}"),
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
    /// A refusal, the network, or an answer of another shape, which is `Unreadable`.
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
        serde_json::from_str(&text).map_err(|e| Error::Unreadable(format!("{verb}: {e}")))
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

/// The events of a server-sent stream, in order; it ends when the connection does.
pub struct Changes {
    lines: BufReader<Box<dyn Read + Send>>,
}

/// One event of the stream: its name and its data lines joined.
#[derive(Debug, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub data: String,
}

impl Event {
    /// Whether a `change` is news to the project `shown`. A change naming other projects is not;
    /// one naming none, or whose data cannot be read, may be any project's, and no `shown` is a
    /// page of no single project.
    #[must_use]
    pub fn concerns(&self, shown: Option<&str>) -> bool {
        let Some(shown) = shown else { return true };
        let Ok(data) = serde_json::from_str::<serde_json::Value>(&self.data) else {
            return true;
        };
        match data.get("projects").and_then(|p| p.as_array()) {
            Some(projects) if !projects.is_empty() => projects.iter().any(|p| p == shown),
            _ => true,
        }
    }
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
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        let mut name = None;
        let mut data: Vec<String> = Vec::new();
        loop {
            let mut line = String::new();
            if self.lines.read_line(&mut line).ok()? == 0 {
                return None;
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if name.is_some() || !data.is_empty() {
                    return Some(Event {
                        name: name.take().unwrap_or_else(|| "message".to_string()),
                        data: data.join("\n"),
                    });
                }
            } else if let Some(n) = line.strip_prefix("event:") {
                name = Some(n.trim().to_string());
            } else if let Some(d) = line.strip_prefix("data:") {
                data.push(d.strip_prefix(' ').unwrap_or(d).to_string());
            }
        }
    }
}

/// The words of a refused request: a verb's refusal, or any route's error.
fn refusal(resp: Response) -> Error {
    let code = resp.status().as_u16();
    let path = resp.url().path().to_string();
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
    if why.trim().is_empty() {
        return Error::Refused(code, unexplained(code, &path));
    }
    Error::Refused(code, why)
}

/// What to say when the server answers an error with no words of its own.
fn unexplained(code: u16, path: &str) -> String {
    if code == 404 {
        return format!("the server has no route {path}; it may be older than this client");
    }
    format!("the server answered {code} for {path} with no reason")
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

    /// `/metrics?scope=releases`: one row per unshipped release, the current first.
    ///
    /// # Errors
    /// As `get`.
    pub fn release_rows(&self, slug: &str) -> Result<Vec<ReleaseRow>> {
        let mut query = of(slug).clone();
        query.push(("scope", "releases".to_string()));
        let mut body: Value = self.get("/metrics", &query)?;
        Ok(serde_json::from_value(body["releases"].take()).unwrap_or_default())
    }

    /// `/metrics` over the whole project: its progress, its opened, closed and dropped per day for
    /// `days` days, and its areas.
    ///
    /// # Errors
    /// As `get`.
    pub fn project_metrics(&self, slug: &str, days: u32) -> Result<docket_core::metrics::Whole> {
        let mut query = of(slug).clone();
        query.push(("scope", "project".to_string()));
        query.push(("days", days.to_string()));
        self.get("/metrics", &query)
    }

    /// `/check`: the integrity problems of a project, each an object with its `kind`.
    ///
    /// # Errors
    /// As `get`.
    pub fn check(&self, slug: &str) -> Result<Vec<Value>> {
        self.get("/check", &of(slug))
    }

    /// `/next`, under an item when one is named.
    ///
    /// # Errors
    /// As `get`.
    pub fn next(
        &self,
        slug: &str,
        n: usize,
        narrow: &[(&'static str, String)],
    ) -> Result<Vec<Row>> {
        let mut q = of(slug);
        q.push(("n", n.to_string()));
        q.extend(narrow.iter().cloned());
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
    pub fn set_fact(
        &self,
        slug: &str,
        key: &str,
        value: &str,
        all_projects: bool,
    ) -> Result<FactSet> {
        let req = FactRequest {
            common: Common {
                project: slug.to_string(),
                ..Common::default()
            },
            key: key.to_string(),
            value: value.to_string(),
            all_projects,
        };
        self.post("fact", &req)
    }

    /// Every machine jobs run on, by name.
    ///
    /// # Errors
    /// As `get`.
    pub fn machines(&self) -> Result<Machines> {
        self.get("/machines", &[])
    }

    /// One machine set or removed, on the owner's key; the machines as they stand after.
    ///
    /// # Errors
    /// As `post`.
    pub fn set_machine(&self, req: &MachineRequest) -> Result<Machines> {
        self.post("machine", req)
    }

    /// Who leads a project, and whether the claim lapsed.
    ///
    /// # Errors
    /// As `get`.
    pub fn lead(&self, slug: &str) -> Result<LeadState> {
        self.get("/lead", &of(slug))
    }

    /// The project's lead claim taken, renewed or given back by a session on this key's host.
    ///
    /// # Errors
    /// As `post`.
    pub fn lead_act(
        &self,
        slug: &str,
        branch: Option<&str>,
        act: &str,
        session: &str,
    ) -> Result<LeadState> {
        let req = LeadRequest {
            common: Common {
                project: slug.to_string(),
                branch: branch.map(str::to_string),
                force: false,
            },
            act: act.to_string(),
            session: session.to_string(),
        };
        self.post("lead", &req)
    }

    /// A project's publications, newest first: the first is where the next squash starts.
    ///
    /// # Errors
    /// As `get`.
    pub fn publications(&self, slug: &str) -> Result<Publications> {
        self.get("/publications", &of(slug))
    }

    /// One publication a squash wrote, recorded; the project's publications as they stand after.
    ///
    /// # Errors
    /// As `post`.
    pub fn record_publication(&self, req: &PublicationRequest) -> Result<Publications> {
        self.post("publication", req)
    }

    /// # Errors
    /// As `get`.
    pub fn derived(&self, slug: &str) -> Result<Vec<Derived>> {
        self.get("/derived", &of(slug))
    }

    /// A project's releases in the order they ship, the shipped ones too with `all`.
    ///
    /// # Errors
    /// As `get`.
    pub fn releases(&self, slug: &str, all: bool) -> Result<Vec<docket_core::release::Row>> {
        let mut query = of(slug);
        if all {
            query.push(("all", "true".to_string()));
        }
        self.get("/releases", &query)
    }

    /// A project's areas in position order.
    ///
    /// # Errors
    /// As `get`.
    pub fn areas(&self, slug: &str) -> Result<Vec<docket_core::area::Row>> {
        self.get("/areas", &of(slug))
    }

    /// Every stored item of a project, bodies left out, each with the claim or the owner's ask its
    /// open assignment holds.
    ///
    /// # Errors
    /// As `get`.
    pub fn items(&self, slug: &str) -> Result<Vec<ItemRow>> {
        let mut rows: Vec<ItemRow> =
            self.all("/items", &json!({ "project": slug }), "[\"rid\",\"ASC\"]")?;
        let held: Vec<HeldRow> = self.get("/held", &of(slug))?;
        let held: std::collections::HashMap<i64, Held> =
            held.into_iter().map(|h| (h.rid, h.held)).collect();
        for r in &mut rows {
            r.hold(held.get(&r.rid));
        }
        Ok(rows)
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
                "[\"id\",\"ASC\"]",
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
                "[\"id\",\"ASC\"]",
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
