//! Every fact of a project with its value and its meaning. Each name is a hot spot: Enter edits its value
//! in place.

use docket_core::fact::{self, FACTS, Value};
use docket_core::rows::ProjectRow;
use ratatui::style::Style;

use crate::doc::{Doc, Target, seg, spot, wrap};
use crate::page::Settings;
use crate::style;
use crate::write::{Ask, Prompt};

/// Room for a fact's name.
const NAME: usize = 16;

/// The fact being typed, and the fact whose last value the server refused.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Editing {
    pub typing: Option<(String, String)>,
    pub refused: Option<(String, String)>,
}

impl Editing {
    #[must_use]
    pub fn of(page: &Settings, prompt: Option<&Prompt>) -> Self {
        let typing = prompt.and_then(|p| match &p.ask {
            Ask::Fact(key) => Some((key.clone(), p.text.clone())),
            _ => None,
        });
        Self {
            typing,
            refused: page.refused.clone(),
        }
    }
}

#[must_use]
pub fn doc(project: &ProjectRow, width: usize, editing: &Editing) -> Doc {
    let mut d = Doc::default();
    d.plain(format!("SETTINGS  {}", project.slug), style::bold());
    d.plain(
        "  Enter edits the selected fact in place; an empty value unsets it.",
        style::dim(),
    );
    d.blank();
    for (key, meaning) in FACTS {
        fact_lines(&mut d, project, key, width, editing);
        for part in wrap(meaning, width.saturating_sub(NAME + 4)) {
            d.plain(format!("  {:NAME$}{part}", ""), style::dim());
        }
    }
    d
}

/// A fact's name and its whole value, wrapped under the value's column. The value being typed stands in
/// its place, and a refusal of it shows beneath.
fn fact_lines(d: &mut Doc, project: &ProjectRow, key: &str, width: usize, editing: &Editing) {
    let (text, tone) = match (&editing.typing, fact::value(&project.skills, key)) {
        (Some((k, typed)), _) if k == key => (format!("{typed}_"), style::bold()),
        (_, Value::Set(v)) => (v, Style::default()),
        (_, Value::Default(v)) => (format!("{v} (default)"), style::dim()),
        (_, Value::Unset) => ("(not set)".to_string(), style::dim()),
    };
    let (name_tone, value_tone) = (style::bold(), tone);
    for (i, part) in wrap(&text, width.saturating_sub(NAME + 4))
        .into_iter()
        .enumerate()
    {
        let name = if i == 0 {
            spot(key.to_string(), name_tone, Target::Fact(key.to_string()))
        } else {
            seg(String::new(), name_tone)
        };
        let pad = NAME.saturating_sub(if i == 0 { key.len() } else { 0 });
        d.line(vec![
            seg("  ", name_tone),
            name,
            seg(" ".repeat(pad), Style::default()),
            seg(part, value_tone),
        ]);
    }
    if let Some((k, why)) = &editing.refused
        && k == key
    {
        for part in wrap(&format!("refused: {why}"), width.saturating_sub(NAME + 4)) {
            d.plain(format!("  {:NAME$}{part}", ""), style::alarm());
        }
    }
}
