//! The text the dump repository carries: one file per item, and an event log and a project file per
//! project, each byte for byte what the Python dump writes for the same rows.

use std::collections::BTreeMap;
use std::fmt::Write;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::pyjson::{Style, data_of, dumps_styled};

/// A frontmatter value: one line, non-ASCII kept.
const LINE: Style = Style {
    sort_keys: false,
    ascii: false,
    indent: None,
};
const EVENT: Style = Style {
    sort_keys: true,
    ascii: false,
    indent: None,
};
const PROJECT: Style = Style {
    sort_keys: true,
    ascii: false,
    indent: Some(2),
};

/// Written only when set, so every file dumped before them stays byte-identical.
pub const OPTIONAL: [&str; 10] = [
    "claim_runner",
    "claim_job",
    "claim_on",
    "scope",
    "release",
    "area",
    "parent",
    "origin",
    "depends",
    "labels",
];

/// Fields an older item file carries that are read and never written: `opened`, which a restore
/// splits into a parent and origins.
pub const LEGACY: [&str; 1] = ["opened"];

/// The frontmatter of an item file, in order.
pub const FIELDS: [&str; 36] = [
    "id",
    "title",
    "state",
    "turn",
    "turn_note",
    "asked_at",
    "claim_branch",
    "claim_host",
    "claim_since",
    "claim_runner",
    "claim_job",
    "claim_on",
    "wait_on",
    "wait_ref",
    "wait_since",
    "decision",
    "decided_at",
    "resolution",
    "superseded_by",
    "scope",
    "complexity",
    "group",
    "theme",
    "release",
    "area",
    "rank",
    "type",
    "priority",
    "tags",
    "related",
    "parent",
    "origin",
    "depends",
    "labels",
    "opened_at",
    "updated_at",
];

/// One item as its dump file holds it: ids for every reference, the project and body beside.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemDump {
    pub project: String,
    pub id: String,
    pub title: String,
    pub state: String,
    pub turn: Option<String>,
    pub turn_note: Option<String>,
    pub asked_at: Option<String>,
    pub claim_branch: Option<String>,
    pub claim_host: Option<String>,
    pub claim_since: Option<String>,
    pub claim_runner: Option<String>,
    pub claim_job: Option<String>,
    pub claim_on: Option<String>,
    pub wait_on: Option<String>,
    pub wait_ref: Option<String>,
    pub wait_since: Option<String>,
    pub decision: Option<String>,
    pub decided_at: Option<String>,
    pub resolution: Option<String>,
    pub superseded_by: Option<String>,
    pub scope: Option<String>,
    pub complexity: Option<String>,
    pub group: Option<String>,
    pub theme: Option<String>,
    /// The release it is in by name; none is the backlog.
    pub release: Option<String>,
    /// The area it is in by name.
    pub area: Option<String>,
    pub rank: Option<i64>,
    /// What the item is; a file written before the column carries none, and a restore reads it from
    /// the key.
    #[serde(rename = "type")]
    pub item_type: String,
    /// A file written before the column carries none, and a restore reads it from the tags.
    pub priority: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,
    /// The plan the item belongs to.
    pub parent: Option<String>,
    /// What spawned the item; none when nothing is recorded.
    pub origin: Option<Vec<String>>,
    /// The `opened` links an item file carried before parents and origins, read so an older dump
    /// restores; never written.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub opened: Vec<String>,
    /// What the item depends on; none when it depends on nothing.
    pub depends: Option<Vec<String>>,
    /// The labels it was given, never those it inherits; none when it was given none.
    pub labels: Option<Vec<String>>,
    pub opened_at: String,
    pub updated_at: String,
    pub body: String,
}

/// One event as its log line holds it, with the id of its item and its data as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDump {
    pub project: String,
    pub uid: String,
    pub at: String,
    pub host: String,
    pub branch: Option<String>,
    pub kind: String,
    pub note: Option<String>,
    pub item: Option<String>,
    pub data: Option<String>,
}

/// A project row with its JSON columns decoded, the fields of `project.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDump {
    pub slug: String,
    pub keys: Value,
    pub remotes: Value,
    pub themes: Value,
    pub cite_roots: Value,
    pub repos: Value,
    pub fleet_repo: Option<String>,
    pub integration_ref: Option<String>,
    pub worktree_hint: Option<String>,
    pub test_hint: Option<String>,
    pub skills: Value,
    pub created_at: String,
    pub updated_at: String,
    /// Every release, shipped or not, in position order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub releases: Vec<crate::release::Release>,
    /// Every area, in position order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<crate::area::Area>,
    /// Every label, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<crate::label::Label>,
}

/// What changed after a cursor: every project, the items to rewrite and the events to add. A full
/// page carries every item and every event, and its event logs replace the ones on disk.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpPage {
    pub cursor: i64,
    pub full: bool,
    pub projects: Vec<ProjectDump>,
    pub items: Vec<ItemDump>,
    pub events: Vec<EventDump>,
}

#[must_use]
pub fn item_path(slug: &str, id: &str) -> String {
    format!("{slug}/items/{id}.md")
}

#[must_use]
pub fn events_path(slug: &str) -> String {
    format!("{slug}/events.jsonl")
}

#[must_use]
pub fn project_path(slug: &str) -> String {
    format!("{slug}/project.json")
}

/// An item file: the frontmatter in field order, then the body.
#[must_use]
pub fn render_item(item: &ItemDump) -> String {
    let Ok(Value::Object(fields)) = serde_json::to_value(item) else {
        return String::new();
    };
    let mut text = String::from("---\n");
    for k in FIELDS {
        let mut v = fields.get(k).cloned().unwrap_or(Value::Null);
        if OPTIONAL.contains(&k) && v.is_null() {
            continue;
        }
        if let (Value::Array(ids), "related" | "origin" | "depends" | "labels") = (&mut v, k) {
            ids.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        }
        let _ = writeln!(text, "{k}: {}", dumps_styled(&v, LINE));
    }
    text.push_str("---\n");
    let body = item.body.trim_end_matches('\n');
    if !body.is_empty() {
        text.push_str(body);
        text.push('\n');
    }
    text
}

/// One line of `events.jsonl`; `data` appears only on an event that has it.
#[must_use]
pub fn event_line(e: &EventDump) -> String {
    let mut keep = Map::new();
    keep.insert("uid".into(), e.uid.clone().into());
    keep.insert("at".into(), e.at.clone().into());
    keep.insert("host".into(), e.host.clone().into());
    keep.insert("branch".into(), e.branch.clone().into());
    keep.insert("kind".into(), e.kind.clone().into());
    keep.insert("note".into(), e.note.clone().into());
    keep.insert("item".into(), e.item.clone().into());
    if let Some(data) = e.data.as_deref().filter(|d| !d.is_empty()) {
        let value = serde_json::from_str(data).unwrap_or_else(|_| Value::String(data.into()));
        keep.insert("data".into(), value);
    }
    dumps_styled(&Value::Object(keep), EVENT)
}

/// `project.json`: the row's fields, sorted and indented.
#[must_use]
pub fn project_text(p: &ProjectDump) -> String {
    let value = serde_json::to_value(p).unwrap_or_default();
    format!("{}\n", dumps_styled(&value, PROJECT))
}

/// An event log with the events added, each uid once, in `(at, uid)` order.
#[must_use]
pub fn merge_events(before: &str, events: &[&EventDump]) -> String {
    let mut lines: BTreeMap<(String, String), String> = BTreeMap::new();
    for line in before.lines().filter(|l| !l.trim().is_empty()) {
        lines.insert(order_of(line), line.to_string());
    }
    for e in events {
        lines.insert((e.at.clone(), e.uid.clone()), event_line(e));
    }
    lines.into_values().map(|l| l + "\n").collect()
}

/// The `(at, uid)` a log line sorts by; a line that is not an event sorts first, by its text.
fn order_of(line: &str) -> (String, String) {
    let parsed: Value = serde_json::from_str(line).unwrap_or_default();
    match (parsed["at"].as_str(), parsed["uid"].as_str()) {
        (Some(at), Some(uid)) => (at.to_string(), uid.to_string()),
        _ => (String::new(), line.to_string()),
    }
}

/// Every file a page writes, `(path, text)`. `read` gives an event log as it is on disk, which an
/// incremental page adds to.
pub fn files(page: &DumpPage, read: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = page
        .projects
        .iter()
        .map(|p| (project_path(&p.slug), project_text(p)))
        .collect();
    out.extend(
        page.items
            .iter()
            .map(|i| (item_path(&i.project, &i.id), render_item(i))),
    );
    let mut logs: BTreeMap<&str, Vec<&EventDump>> = BTreeMap::new();
    if page.full {
        for p in &page.projects {
            logs.entry(p.slug.as_str()).or_default();
        }
    }
    for e in &page.events {
        logs.entry(e.project.as_str()).or_default().push(e);
    }
    for (slug, events) in logs {
        let path = events_path(slug);
        let before = if page.full {
            String::new()
        } else {
            read(&path).unwrap_or_default()
        };
        out.push((path, merge_events(&before, &events)));
    }
    out
}

/// One commit line for several writes: the first three, then how many more.
#[must_use]
pub fn commit_subject(messages: &[String]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for m in messages {
        if !seen.contains(&m.as_str()) {
            seen.push(m);
        }
    }
    if seen.is_empty() {
        return "docket write".to_string();
    }
    let mut line = seen[..seen.len().min(3)].join("; ");
    if seen.len() > 3 {
        let _ = write!(line, "; and {} more", seen.len() - 3);
    }
    line
}

/// One message per write, as the command that made it names its commit.
#[must_use]
pub fn messages(events: &[EventDump]) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0;
    for end in 1..=events.len() {
        if end == events.len() || !follows(&events[start], &events[end]) {
            out.extend(write_message(&events[start..end]));
            start = end;
        }
    }
    out
}

/// Whether an event belongs to the write `first` opened. A write's own events share its moment,
/// host and project; what it releases or settles carries no branch; a write over several items
/// repeats its note, and a fold names the package it folds into.
fn follows(first: &EventDump, e: &EventDump) -> bool {
    if (&first.at, &first.host, &first.project) != (&e.at, &e.host, &e.project) {
        return false;
    }
    if e.branch.is_none() && matches!(e.kind.as_str(), "resumed" | "waited") {
        return true;
    }
    if let Some(into) = fold_into(first) {
        return fold_into(e) == Some(into);
    }
    let repeats = ["priority ", "link ", "unlink ", "parent ", "to the "];
    match (first.note.as_deref(), e.note.as_deref()) {
        (Some(lead), Some(note)) if first.kind == "edited" && e.kind == "edited" => {
            note == lead && repeats.iter().any(|p| lead.starts_with(p))
        }
        _ => false,
    }
}

/// The package a fold's event names as the one folded into.
fn fold_into(e: &EventDump) -> Option<&str> {
    let note = e.note.as_deref()?;
    match e.kind.as_str() {
        "edited" if note.starts_with("moved from ") => note.rsplit(" to ").next(),
        "edited" if note.starts_with("folded ") => e.item.as_deref(),
        "dropped" => note.strip_prefix("folded into "),
        _ => None,
    }
}

fn write_message(write: &[EventDump]) -> Option<String> {
    if let Some(m) = fold_message(write) {
        return Some(m);
    }
    let first = write.first()?;
    let id = first.item.as_deref()?;
    let verb = match first.kind.as_str() {
        "opened" => "Open",
        "claimed" => "Start",
        "released" => "Unclaim",
        "closed" => "Close",
        "dropped" => "Drop",
        "reopened" => "Reopen",
        "waited" => "Wait",
        "resumed" => "Resume",
        "asked" => "Ask",
        "replied" => "Reply",
        "decided" if data_of(first.data.as_deref()).contains_key("derived") => {
            return Some(format!("Decide on {id}"));
        }
        "decided" => "Answer",
        "edited" => return Some(edit_message(write, id)),
        _ => return None,
    };
    Some(format!("{verb} {id}"))
}

/// A fold's last event names the packages folded and the one they went into.
fn fold_message(write: &[EventDump]) -> Option<String> {
    write.iter().rev().find_map(|e| {
        let parts = e.note.as_deref()?.strip_prefix("folded ")?;
        let into = e.item.as_deref().filter(|_| e.kind == "edited")?;
        Some(format!("Fold {parts} into {into}"))
    })
}

/// An `edited` write, told apart by its note: priority, rate, link, parent, a move, or an edit.
fn edit_message(write: &[EventDump], id: &str) -> String {
    let note = write[0].note.as_deref().unwrap_or("");
    let alike = |prefixes: &[&str]| {
        write
            .iter()
            .filter(|e| e.kind == "edited")
            .filter_map(|e| e.note.as_deref())
            .filter(|n| prefixes.iter().any(|p| n.starts_with(p)))
            .count()
    };
    if note.starts_with("priority ") {
        return match alike(&["priority "]) {
            n if n > 1 => format!("Prioritise {n} items"),
            _ => format!("Prioritise {id}"),
        };
    }
    if note.starts_with("complexity ") {
        return format!("Rate {id}");
    }
    if note.starts_with("link ") || note.starts_with("unlink ") {
        let to = note.rsplit(' ').next().unwrap_or("");
        return match alike(&["link ", "unlink "]) {
            n if n > 1 => format!("Link {n} items to {to}"),
            _ => format!("Link {id}"),
        };
    }
    if let Some(plan) = note.strip_prefix("parent ") {
        return match alike(&["parent "]) {
            n if n > 1 => format!("Put {n} items under {plan}"),
            _ => format!("Put {id} under {plan}"),
        };
    }
    if let Some(rest) = note.strip_prefix("to the ") {
        let place = rest.split(": ").next().unwrap_or(rest);
        return match alike(&["to the "]) {
            n if n > 1 => format!("Move {n} items to the {place}"),
            _ => format!("Move {id} to the {place}"),
        };
    }
    format!("Edit {id}")
}

/// `Key K` for each key spec a project file gains or changes; a write that sets a key logs no event.
#[must_use]
pub fn key_messages(before: Option<&str>, after: &ProjectDump) -> Vec<String> {
    let Some(before) = before else {
        return Vec::new();
    };
    let parsed: Value = serde_json::from_str(before).unwrap_or_default();
    let old = parsed["keys"].as_array().cloned().unwrap_or_default();
    after
        .keys
        .as_array()
        .into_iter()
        .flatten()
        .filter(|spec| !old.contains(spec))
        .filter_map(|spec| spec["key"].as_str())
        .map(|k| format!("Key {k}"))
        .collect()
}

/// The messages of a page's commit: one per write, then a key change, which logs no event.
pub fn page_messages(page: &DumpPage, read: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut out = messages(&page.events);
    for p in &page.projects {
        out.extend(key_messages(read(&project_path(&p.slug)).as_deref(), p));
    }
    out
}

/// The frontmatter fields and the body of an item file.
///
/// # Errors
/// A file without both fences, a field the format does not have, or a value that is not JSON.
pub fn parse_item(text: &str) -> Result<(Map<String, Value>, String), String> {
    let rest = text
        .strip_prefix("---\n")
        .ok_or("item file does not start with a frontmatter fence")?;
    let end = rest
        .find("\n---\n")
        .ok_or("item file has no closing frontmatter fence")?;
    let mut fields = Map::new();
    for line in rest[..end].split('\n').filter(|l| !l.trim().is_empty()) {
        let (k, v) = line.split_once(": ").unwrap_or((line, ""));
        if !FIELDS.contains(&k) && !LEGACY.contains(&k) {
            return Err(format!("unknown field {k:?} in item file"));
        }
        let value = serde_json::from_str(v).map_err(|e| format!("{k}: {e}"))?;
        fields.insert(k.to_string(), value);
    }
    let body = &rest[end + 5..];
    Ok((fields, body.strip_suffix('\n').unwrap_or(body).to_string()))
}

/// An item of a project from its parsed file. A list written as null reads as empty.
///
/// # Errors
/// A field holding the wrong type, or a missing id.
pub fn item_from(
    project: &str,
    mut fields: Map<String, Value>,
    body: String,
) -> Result<ItemDump, String> {
    for k in ["tags", "related", "opened"] {
        if fields.get(k).is_some_and(Value::is_null) {
            fields.remove(k);
        }
    }
    if !fields.get("id").is_some_and(Value::is_string) {
        return Err("item file has no id".to_string());
    }
    fields.insert("project".into(), project.into());
    fields.insert("body".into(), body.into());
    serde_json::from_value(Value::Object(fields)).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "tests/dump.rs"]
mod tests;
