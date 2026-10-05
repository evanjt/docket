//! The screen's writes: each key or typed line is one request through the source. A refusal is shown and
//! changes nothing; a write that lands reads the page again.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Serialize;
use serde_json::Value;

use docket_core::api::{
    AnswerRequest, Common, FactRequest, LinkRequest, PriorityRequest, RateRequest, ReplyRequest,
    RetryRequest,
};
use docket_core::flow::DERIVED;

use crate::app::App;
use crate::doc::{Listing, Target};
use crate::page::{Browser, Page};
use crate::palette::{self, Command};
use crate::source::Source;

/// What Enter does with the typed line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    Answer(String),
    Reply(String),
    Fact(String),
    Link(Vec<String>),
    Palette(Vec<String>),
    /// One key picks the tier.
    Priority(Vec<String>),
    /// One key picks the complexity.
    Rate(Vec<String>),
}

/// A line being typed at the foot of the screen for a write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub ask: Ask,
    pub label: String,
    pub text: String,
}

/// The note a retry from the screen carries.
pub const RETRY_NOTE: &str = "retried from the screen";

const TIERS: [(char, &str); 4] = [
    ('c', "critical"),
    ('h', "high"),
    ('n', "normal"),
    ('l', "low"),
];
const LEVELS: [(char, &str); 3] = [('h', "high"), ('m', "medium"), ('l', "low")];

/// A request body as JSON, for the source to post.
fn body(req: &impl Serialize) -> Value {
    serde_json::to_value(req).unwrap_or_default()
}

impl Prompt {
    /// The keys a one-key choice takes, with their words, for the foot of the screen.
    #[must_use]
    pub fn choices(&self) -> Option<&'static [(char, &'static str)]> {
        match self.ask {
            Ask::Priority(_) => Some(&TIERS),
            Ask::Rate(_) => Some(&LEVELS),
            _ => None,
        }
    }
}

impl<S: Source> App<S> {
    fn common(&self) -> Common {
        Common {
            project: self.page.slug().unwrap_or_default().to_string(),
            branch: None,
            force: false,
        }
    }

    /// The row the pointer is on, in whichever list the page has; on the project page, the id selected.
    #[must_use]
    pub fn selected_row(&self) -> Option<String> {
        let id = match &self.page {
            Page::Browser(b) => b.selected().map(|e| e.id.clone()),
            Page::Queue(q) => q.rows.get(q.at).map(|r| r.id.clone()),
            Page::Plans(p) => p.rows.get(p.sel).map(|r| r.id.clone()),
            Page::Project(_) => match self.cursor().selected() {
                Some(Target::Item(id)) => Some(id.clone()),
                _ => None,
            },
            _ => None,
        };
        id.filter(|i| !i.is_empty())
    }

    /// The ids a list page shows, in its order.
    fn listed(&self) -> Vec<String> {
        match &self.page {
            Page::Browser(b) => b.entries.iter().map(|e| e.id.clone()).collect(),
            Page::Queue(q) => q.rows.iter().map(|r| r.id.clone()).collect(),
            Page::Plans(p) => p.rows.iter().map(|r| r.id.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// What a move on several acts on: the marked rows in list order, else the row the pointer is on.
    #[must_use]
    pub fn selection(&self) -> Vec<String> {
        let Some(marks) = self.page.marks().filter(|m| !m.is_empty()) else {
            return self.selected_row().into_iter().collect();
        };
        let mut out: Vec<String> = self
            .listed()
            .into_iter()
            .filter(|id| marks.contains(id))
            .collect();
        out.extend(
            marks
                .iter()
                .filter(|m| !out.contains(m))
                .cloned()
                .collect::<Vec<_>>(),
        );
        out
    }

    /// Marks or unmarks the row the pointer is on.
    pub fn toggle_mark(&mut self) {
        let Some(id) = self.selected_row() else {
            return;
        };
        if let Some(marks) = self.page.marks_mut()
            && !marks.remove(&id)
        {
            marks.insert(id);
        }
    }

    /// One request; a refusal is put at the foot of the screen and nothing else moves.
    pub fn send(&mut self, verb: &str, body: &Value) -> bool {
        match self.source.post(verb, body) {
            Ok(_) => true,
            Err(why) => {
                self.flash = Some(why);
                false
            }
        }
    }

    /// After a write lands: say so, drop the marks, and read the page again.
    fn landed(&mut self, said: String) {
        if let Some(m) = self.page.marks_mut() {
            m.clear();
        }
        self.changed();
        self.flash = Some(said);
    }

    /// The keys that write, and marking; false when the key is none of them.
    pub fn write_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('x') => self.toggle_mark(),
            KeyCode::Char(' ') if matches!(self.page, Page::Browser(_) | Page::Queue(_)) => {
                self.toggle_mark();
            }
            KeyCode::Char('X') => {
                if let Some(m) = self.page.marks_mut() {
                    m.clear();
                }
            }
            KeyCode::Char('a') => self.ask_one(Ask::Answer),
            KeyCode::Char('r') => self.ask_one(Ask::Reply),
            KeyCode::Char('R') => self.retry(),
            KeyCode::Char('!') => self.ask_many(Ask::Priority),
            KeyCode::Char('c') => self.ask_many(Ask::Rate),
            KeyCode::Char('L') => self.ask_many(Ask::Link),
            KeyCode::Char(':') if self.page.slug().is_some() => {
                let selection = self.selection();
                self.start_prompt("", Ask::Palette(selection), String::new());
            }
            _ => return false,
        }
        true
    }

    pub fn start_prompt(&mut self, label: &str, ask: Ask, text: String) {
        self.prompt = Some(Prompt {
            label: label.to_string(),
            ask,
            text,
        });
    }

    /// A line typed for the item the pointer is on.
    fn ask_one(&mut self, make: fn(String) -> Ask) {
        let Some(id) = self.selected_row() else {
            self.flash = Some("select an item first".into());
            return;
        };
        let label = match make(id.clone()) {
            Ask::Answer(_) => self.answer_label(&id),
            _ => format!("reply to {id}: "),
        };
        self.start_prompt(&label, make(id), String::new());
    }

    /// `answer Q7: `, or `overturn Q7 (derived ...): ` when an agent derived its decision.
    fn answer_label(&self, id: &str) -> String {
        let slug = self.page.slug().unwrap_or_default();
        let decision = self
            .boards
            .get(slug)
            .and_then(|b| b.get(id))
            .and_then(|i| i.decision.clone())
            .filter(|d| d.starts_with(DERIVED));
        match decision {
            Some(d) => {
                let d: String = d.chars().take(60).collect();
                format!("overturn {id} ({d}): ")
            }
            None => format!("answer {id}: "),
        }
    }

    /// A choice or a line that acts on the selection.
    fn ask_many(&mut self, make: fn(Vec<String>) -> Ask) {
        let ids = self.selection();
        if ids.is_empty() {
            self.flash = Some("select an item first, or mark rows with x".into());
            return;
        }
        let shown = ids.join(", ");
        let label = match make(Vec::new()) {
            Ask::Priority(_) => format!("priority of {shown}: "),
            Ask::Rate(_) => format!("complexity of {shown}: "),
            _ => format!("link {shown} (related ID or opened ID): "),
        };
        self.start_prompt(&label, make(ids), String::new());
    }

    fn retry(&mut self) {
        let Some(id) = self.selected_row() else {
            self.flash = Some("select an item first".into());
            return;
        };
        let req = RetryRequest {
            common: self.common(),
            id: id.clone(),
            note: Some(RETRY_NOTE.into()),
        };
        if self.send("retry", &body(&req)) {
            self.landed(format!("retried {id}"));
        }
    }

    /// One key while a line is typed: Enter sends it, Esc drops it, Ctrl-E opens it in `$EDITOR`.
    pub fn prompting(&mut self, k: KeyEvent) -> bool {
        let Some(p) = &mut self.prompt else {
            return false;
        };
        if let Some(choices) = p.choices() {
            if let KeyCode::Char(c) = k.code
                && let Some((_, word)) = choices.iter().find(|(key, _)| *key == c)
            {
                let ask = p.ask.clone();
                self.prompt = None;
                self.choose(ask, word);
            } else if k.code == KeyCode::Esc {
                self.prompt = None;
            }
            return true;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc => self.prompt = None,
            KeyCode::Enter => self.submit(),
            KeyCode::Backspace => {
                p.text.pop();
            }
            KeyCode::Char('e') if ctrl => self.editor = Some(p.text.clone()),
            KeyCode::Char('u') if ctrl => p.text.clear(),
            KeyCode::Char(c) if !ctrl => p.text.push(c),
            _ => {}
        }
        true
    }

    /// The text back from `$EDITOR`: sent as the prompt's line, or dropped when it came back empty.
    pub fn edited(&mut self, text: Option<String>) {
        let text = text.map(|t| t.trim_end().to_string()).unwrap_or_default();
        if text.trim().is_empty() {
            self.prompt = None;
            self.flash = Some("nothing sent: the editor came back empty".into());
            return;
        }
        if let Some(p) = &mut self.prompt {
            p.text = text;
            self.submit();
        }
    }

    fn choose(&mut self, ask: Ask, word: &str) {
        match ask {
            Ask::Priority(ids) => {
                let req = PriorityRequest {
                    common: self.common(),
                    ids: ids.clone(),
                    tier: word.to_string(),
                };
                if self.send("priority", &body(&req)) {
                    self.landed(format!("{} {word}", ids.join(", ")));
                }
            }
            Ask::Rate(ids) => self.rate(&ids, word),
            _ => {}
        }
    }

    /// One `rate` per item, as the verb takes one id; the first refusal stops the rest.
    fn rate(&mut self, ids: &[String], level: &str) {
        for id in ids {
            let req = RateRequest {
                common: self.common(),
                id: id.clone(),
                level: level.to_string(),
            };
            if !self.send("rate", &body(&req)) {
                return;
            }
        }
        self.landed(format!("{} rated {level}", ids.join(", ")));
    }

    /// Enter on a typed line: its one request, or the page it names.
    fn submit(&mut self) {
        let Some(p) = self.prompt.take() else {
            return;
        };
        let text = p.text.trim().to_string();
        let common = self.common();
        let (verb, req, said) = match p.ask {
            Ask::Answer(id) => {
                let req = AnswerRequest {
                    common,
                    id: id.clone(),
                    decision: text,
                    derived: None,
                    carried_by: Vec::new(),
                };
                ("answer", body(&req), format!("answered {id}"))
            }
            Ask::Reply(id) => {
                let req = ReplyRequest {
                    common,
                    id: id.clone(),
                    note: text,
                };
                ("reply", body(&req), format!("replied to {id}"))
            }
            Ask::Fact(key) => return self.set_fact(common, &key, &text),
            Ask::Link(ids) => return self.link(common, &ids, &text),
            Ask::Palette(selection) => return self.palette(&text, &selection),
            Ask::Priority(_) | Ask::Rate(_) => return,
        };
        if self.send(verb, &req) {
            self.landed(said);
        }
    }

    fn set_fact(&mut self, common: Common, key: &str, value: &str) {
        let req = FactRequest {
            common,
            key: key.to_string(),
            value: value.to_string(),
            all_projects: false,
        };
        let landed = self.send("fact", &body(&req));
        if let Page::Settings(s) = &mut self.page {
            s.refused =
                (!landed).then(|| (key.to_string(), self.flash.clone().unwrap_or_default()));
        }
        if landed {
            let said = if value.is_empty() { "unset" } else { "set" };
            self.landed(format!("{said} {key}"));
        }
    }

    fn link(&mut self, common: Common, ids: &[String], text: &str) {
        let words: Vec<&str> = text.split_whitespace().collect();
        let [kind, to] = words[..] else {
            self.flash = Some("link takes a kind and an id: related CON2, or opened PK3".into());
            return;
        };
        let req = LinkRequest {
            common,
            a: ids.to_vec(),
            kind: kind.to_string(),
            b: to.to_string(),
            remove: false,
        };
        if self.send("link", &body(&req)) {
            self.landed(format!("linked {} {kind} {to}", ids.join(", ")));
        }
    }

    /// A `:` line: a write is sent, a read opens its page.
    fn palette(&mut self, line: &str, selection: &[String]) {
        let slug = self.page.slug().unwrap_or_default().to_string();
        match palette::parse(line, &slug, selection) {
            Err(why) => self.flash = Some(why),
            Ok(Command::Post(verb, req)) => {
                if self.send(&verb, &req) {
                    self.landed(format!("done: {line}"));
                }
            }
            Ok(Command::Open(target)) => self.open(target),
            Ok(Command::Search(words)) => {
                self.go(Page::Browser(Browser::new(&slug, Listing::Search(words))));
            }
            Ok(Command::Settings) => self.open_settings(),
        }
    }
}

#[cfg(test)]
#[path = "tests/write.rs"]
mod tests;
