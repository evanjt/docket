//! Every fact of a project with its value and its meaning, the ones the loop needs and lacks in red.

use docket_core::fact::{self, FACTS, Value};
use docket_core::rows::ProjectRow;
use ratatui::style::Style;

use crate::doc::{Doc, seg, wrap};
use crate::style;

/// Room for a fact's name.
const NAME: usize = 16;

#[must_use]
pub fn doc(project: &ProjectRow, width: usize) -> Doc {
    let mut d = Doc::default();
    d.plain(format!("SETTINGS  {}", project.slug), style::bold());
    if fact::gaps(&project.skills).is_empty() {
        d.plain("  The facts let the loop dispatch.", style::word("ready"));
    }
    crate::view::project::why(&mut d, &project.skills, width);
    d.plain(
        "  Read only here: docket skills set KEY \"value\" sets one.",
        style::dim(),
    );
    d.blank();
    for (key, meaning) in FACTS {
        fact_lines(&mut d, project, key, width);
        for part in wrap(meaning, width.saturating_sub(NAME + 4)) {
            d.plain(format!("  {:NAME$}{part}", ""), style::dim());
        }
    }
    d
}

/// A fact's name and its whole value, wrapped under the value's column; red when the loop lacks it.
fn fact_lines(d: &mut Doc, project: &ProjectRow, key: &str, width: usize) {
    let (text, tone) = match fact::value(&project.skills, key) {
        Value::Set(v) => (v, Style::default()),
        Value::Default(v) => (format!("{v} (default)"), style::dim()),
        Value::Unset => ("(not set)".to_string(), style::dim()),
    };
    let (name_tone, value_tone) = if fact::blocks(&project.skills, key) {
        (style::alarm(), style::alarm())
    } else {
        (style::bold(), tone)
    };
    for (i, part) in wrap(&text, width.saturating_sub(NAME + 4))
        .into_iter()
        .enumerate()
    {
        let name = if i == 0 { key } else { "" };
        d.line(vec![
            seg(format!("  {name:<NAME$}"), name_tone),
            seg(part, value_tone),
        ]);
    }
}
