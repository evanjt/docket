//! A fixed project for the tests: a plan waiting on its work, a plan due for audit, a package half
//! done, tickets in each word, a question, a concept, and a second project.

use std::sync::Mutex;

use docket_core::flow::tally;
use docket_core::member::Tie;
use docket_core::rows::{
    Derived, EventRow, ItemRow, KeySpec, Progress, ProjectRow, Row, Shown, Status,
};
use docket_core::word::Kind;
use serde_json::Value;

use crate::source::{Result, Source};
use docket_core::board::Board;

fn keys() -> Vec<KeySpec> {
    [
        ("T", Kind::Work),
        ("B", Kind::Work),
        ("Q", Kind::Decision),
        ("A", Kind::Audit),
        ("PK", Kind::Package),
        ("CON", Kind::Concept),
    ]
    .into_iter()
    .map(|(key, kind)| KeySpec {
        key: key.into(),
        kind,
        meaning: None,
        turn: None,
    })
    .collect()
}

fn project(slug: &str, skills: &[(&str, &str)]) -> ProjectRow {
    ProjectRow {
        slug: slug.into(),
        keys: keys(),
        themes: Vec::new(),
        skills: skills
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
        updated_at: String::new(),
    }
}

fn item(rid: i64, id: &str, title: &str, state: &str) -> ItemRow {
    let key: String = id.chars().take_while(char::is_ascii_uppercase).collect();
    ItemRow {
        rid,
        project: "o/p".into(),
        num: id[key.len()..].parse().unwrap(),
        key,
        id: id.into(),
        title: title.into(),
        state: state.into(),
        turn: Some("agent".into()),
        opened_at: "2026-09-30T08:00:00Z".into(),
        updated_at: "2026-10-01T08:00:00Z".into(),
        ..ItemRow::default()
    }
}

fn items() -> Vec<ItemRow> {
    let mut claimed = item(2, "T2", "Second member being built", "open");
    claimed.claim_branch = Some("audit/t2-1".into());
    claimed.claim_host = Some("devbox.lan".into());
    claimed.claim_job = Some("t2-b1".into());
    claimed.claim_since = Some("2026-10-01T09:00:00Z".into());
    let mut question = item(5, "Q1", "Which store holds the buns", "open");
    question.turn = Some("user".into());
    question.turn_note = Some("pick one".into());
    let mut blocked = item(7, "B1", "Buns vanish after a wipe", "open");
    blocked.wait_on = Some("item".into());
    blocked.wait_ref = Some("Q1".into());
    blocked.group_name = Some("buns".into());
    let mut plan = item(8, "A1", "Buns stay put", "open");
    plan.wait_on = Some("condition".into());
    plan.wait_ref = Some("all closed".into());
    vec![
        item(1, "T1", "First member, done", "done"),
        claimed,
        item(3, "T3", "Fix the bun cache", "open"),
        item(4, "PK1", "Bun cache has one owner", "open"),
        question,
        item(6, "CON1", "Crusts", "open"),
        blocked,
        plan,
        item(9, "A2", "Orders print at the till", "open"),
        item(10, "T4", "Cache the oven buns", "done"),
    ]
}

fn ties() -> Vec<Tie> {
    let opened = |rid, to| Tie {
        rid,
        opened: true,
        to,
    };
    vec![
        opened(1, 4),
        opened(2, 4),
        opened(4, 8),
        opened(3, 8),
        opened(10, 9),
        Tie {
            rid: 3,
            opened: false,
            to: 6,
        },
    ]
}

fn body(id: &str) -> String {
    match id {
        "T3" => "The cache in PK1 forgets T1's fix.\n\n- Needs Q1 answered first, not X9.".into(),
        "Q1" => "Two stores could hold the buns.\nThe owner picks one; B1 waits on it.".into(),
        _ => format!("Body of {id}."),
    }
}

pub struct Fixture {
    pub items: Mutex<Vec<ItemRow>>,
    pub ties: Vec<Tie>,
    /// Every write asked for, in order: the verb and its body.
    pub posts: Mutex<Vec<(String, Value)>>,
    /// The server's words for refusing every write, when set.
    pub refuse: Mutex<Option<String>>,
    /// The facts of o/p, which a `fact` write changes.
    pub facts: Mutex<Vec<(String, String)>>,
}

impl Default for Fixture {
    fn default() -> Self {
        Self {
            items: Mutex::new(items()),
            ties: ties(),
            posts: Mutex::new(Vec::new()),
            refuse: Mutex::new(None),
            facts: Mutex::new(Vec::new()),
        }
    }
}

impl Fixture {
    pub fn board_now(&self) -> Board {
        Board::new(
            project("o/p", &[]),
            self.items.lock().unwrap().clone(),
            self.ties.clone(),
        )
    }

    fn row(b: &Board, i: &ItemRow) -> Row {
        Row {
            id: i.id.clone(),
            key: i.key.clone(),
            num: i.num,
            project: i.project.clone(),
            title: i.title.clone(),
            state: i.state.clone(),
            word: b.word(i),
            priority: "normal".into(),
            group: i.group_name.clone(),
            turn: i.turn.clone(),
            turn_note: i.turn_note.clone(),
            claim_branch: i.claim_branch.clone(),
            claim_host: i.claim_host.clone(),
            claim_since: i.claim_since.clone(),
            claim_job: i.claim_job.clone(),
            wait_on: i.wait_on.clone(),
            wait_ref: i.wait_ref.clone(),
            body: body(&i.id),
            opened_at: i.opened_at.clone(),
            updated_at: i.updated_at.clone(),
            ..Row::default()
        }
    }

    fn rows(&self, keep: impl Fn(&Board, &ItemRow) -> bool) -> Vec<Row> {
        let b = self.board_now();
        b.items
            .iter()
            .filter(|i| keep(&b, i))
            .map(|i| Self::row(&b, i))
            .collect()
    }
}

impl Fixture {
    /// The writes asked for so far.
    pub fn sent(&self) -> Vec<(String, Value)> {
        self.posts.lock().unwrap().clone()
    }
}

impl Source for Fixture {
    fn post(&self, verb: &str, body: &Value) -> Result<Value> {
        self.posts
            .lock()
            .unwrap()
            .push((verb.to_string(), body.clone()));
        if let Some(why) = self.refuse.lock().unwrap().clone() {
            return Err(why);
        }
        if verb == "fact" {
            let key = body["key"].as_str().unwrap_or_default().to_string();
            let value = body["value"].as_str().unwrap_or_default().to_string();
            let mut facts = self.facts.lock().unwrap();
            facts.retain(|(k, _)| *k != key);
            if !value.is_empty() {
                facts.push((key, value));
            }
        }
        Ok(Value::Null)
    }

    fn projects(&self) -> Result<Vec<ProjectRow>> {
        let run = [("mode", "run"), ("models", "medium=claude:m")];
        let facts = self.facts.lock().unwrap().clone();
        let facts: Vec<(&str, &str)> = facts
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        Ok(vec![project("o/p", &facts), project("o/q", &run)])
    }

    fn status(&self, slug: &str) -> Result<Status> {
        let b = self.board_now();
        let words: Vec<(String, Kind, String)> = b
            .items
            .iter()
            .map(|i| (i.key.clone(), b.kind(i), b.word(i)))
            .collect();
        let (by_word, by_key) = tally(
            words
                .iter()
                .map(|(k, kind, w)| (k.as_str(), *kind, w.as_str())),
        );
        Ok(Status {
            project: slug.into(),
            host: "devbox".into(),
            total: by_word.values().sum(),
            by_word,
            by_key,
        })
    }

    fn next(&self, _: &str, n: usize, under: Option<&str>) -> Result<Vec<Row>> {
        let b = self.board_now();
        let held = under.and_then(|id| b.get(id)).map(|i| b.holds(i));
        let mut out = self.rows(|b, i| {
            b.word(i) == "ready"
                && b.kind(i) == Kind::Work
                && held.as_ref().is_none_or(|h| h.contains(&i.rid))
        });
        out.truncate(n);
        Ok(out)
    }

    fn list(&self, route: &str, _: &str) -> Result<Vec<Row>> {
        Ok(match route {
            "todo" => self.rows(|_, i| i.state == "open" && i.turn.as_deref() == Some("user")),
            "questions" => self.rows(|b, i| b.kind(i) == Kind::Decision && i.state == "open"),
            "wip" => self.rows(|_, i| i.claim_branch.is_some()),
            "groups" => self.rows(|_, i| i.group_name.is_some()),
            _ => Vec::new(),
        })
    }

    fn group(&self, _: &str, name: &str) -> Result<Vec<Row>> {
        Ok(self.rows(|_, i| i.group_name.as_deref() == Some(name)))
    }

    fn derived(&self, _: &str) -> Result<Vec<Derived>> {
        Ok(Vec::new())
    }

    fn search(&self, _: &str, words: &str) -> Result<Vec<Row>> {
        let words = words.to_lowercase();
        Ok(self.rows(|_, i| i.title.to_lowercase().contains(&words)))
    }

    fn show(&self, _: &str, id: &str) -> Result<Shown> {
        let b = self.board_now();
        let Some(i) = b.get(id) else {
            return Err(format!("404: no item {id} in o/p"));
        };
        let opened = b
            .ties
            .iter()
            .filter(|t| t.opened && t.rid == i.rid)
            .filter_map(|t| b.by_rid(t.to).map(|p| p.id.clone()))
            .collect();
        let related = b
            .ties
            .iter()
            .filter(|t| !t.opened && (t.rid == i.rid || t.to == i.rid))
            .filter_map(|t| {
                b.by_rid(if t.rid == i.rid { t.to } else { t.rid })
                    .map(|p| p.id.clone())
            })
            .collect();
        let progress: Option<Progress> = (b.kind(i) == Kind::Package).then(|| b.progress(i));
        Ok(Shown {
            row: Self::row(&b, i),
            related,
            opened,
            cites: Vec::new(),
            progress,
        })
    }

    fn board(&self, slug: &str) -> Result<Board> {
        let mut b = self.board_now();
        b.project = self
            .projects()?
            .into_iter()
            .find(|p| p.slug == slug)
            .ok_or_else(|| format!("404: no project {slug}"))?;
        Ok(b)
    }

    fn log(&self, rid: i64) -> Result<Vec<EventRow>> {
        Ok(vec![EventRow {
            seq: rid,
            project: "o/p".into(),
            rid: Some(rid),
            at: "2026-09-30T08:00:00Z".into(),
            host: "devbox".into(),
            kind: "opened".into(),
            note: Some("filed beside T1".into()),
            ..EventRow::default()
        }])
    }

    fn recent(&self, _: &str) -> Result<Vec<EventRow>> {
        let event = |seq: i64, rid: i64, at: &str, kind: &str| EventRow {
            seq,
            project: "o/p".into(),
            rid: Some(rid),
            at: at.into(),
            host: "devbox".into(),
            kind: kind.into(),
            ..EventRow::default()
        };
        Ok(vec![
            event(9, 2, "2026-10-01T09:00:00Z", "claimed"),
            event(8, 1, "2026-10-01T08:30:00Z", "closed"),
            event(7, 10, "2026-10-01T08:20:00Z", "closed"),
            event(6, 10, "2026-10-01T08:10:00Z", "claimed"),
            event(5, 9, "2026-10-01T08:00:00Z", "opened"),
            event(4, 10, "2026-09-30T09:00:00Z", "opened"),
            event(1, 3, "2026-09-30T08:00:00Z", "opened"),
        ])
    }
}
