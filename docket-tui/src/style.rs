//! One rule for every colour: it names the state an item is in or is moved into. A verb takes the colour
//! of the word it leads to, so `claimed` is the yellow of `in progress`.

use ratatui::style::{Color, Modifier, Style};

/// The colour of a state word.
#[must_use]
pub fn word(w: &str) -> Style {
    let s = Style::default();
    match w {
        "ready" => s.fg(Color::Green),
        "in progress" | "building" => s.fg(Color::Yellow),
        "audit due" => s.fg(Color::Cyan),
        "waiting on owner" => s.fg(Color::Magenta),
        "blocked" => s.fg(Color::Blue),
        "dropped" | "FAILED" => s.fg(Color::Red),
        "done" => s.add_modifier(Modifier::DIM),
        _ => s,
    }
}

/// The word a verb leads to, `None` for one that leads to no state of its own.
#[must_use]
pub fn verb_tint(verb: &str) -> Option<&'static str> {
    match verb {
        "closed" => Some("done"),
        "released" | "replied" | "resumed" => Some("ready"),
        "claimed" => Some("in progress"),
        "parked" => Some("waiting on owner"),
        "blocked" => Some("blocked"),
        "dropped" | "lost" => Some("dropped"),
        _ => None,
    }
}

/// A verb in the colour of the word it leads to, bold when it leads to none.
#[must_use]
pub fn verb(v: &str) -> Style {
    verb_tint(v).map_or_else(|| Style::default().add_modifier(Modifier::BOLD), word)
}

#[must_use]
pub fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

#[must_use]
pub fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

/// What marks the list row or hot spot under the pointer; the keyboard cursor is reversed.
pub const HOVER: Modifier = Modifier::ITALIC.union(Modifier::UNDERLINED);

/// What stops the work: a missing fact, NO LOOP, a failure.
#[must_use]
pub fn alarm() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}

#[must_use]
pub fn bar() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}
