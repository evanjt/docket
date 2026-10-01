//! Lines of text in which ids, counts and lists are hot spots: Tab moves between them, Enter opens one.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// A list route the browser can show, each taking the project alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Route {
    Todo,
    Questions,
    Groups,
    Wip,
    Waiting,
    Research,
    Derived,
    Done,
    Dropped,
    Next,
}

impl Route {
    pub const ALL: [Route; 10] = [
        Route::Next,
        Route::Todo,
        Route::Questions,
        Route::Groups,
        Route::Wip,
        Route::Waiting,
        Route::Research,
        Route::Derived,
        Route::Done,
        Route::Dropped,
    ];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Route::Todo => "todo",
            Route::Questions => "questions",
            Route::Groups => "groups",
            Route::Wip => "wip",
            Route::Waiting => "waiting",
            Route::Research => "research",
            Route::Derived => "derived",
            Route::Done => "done",
            Route::Dropped => "dropped",
            Route::Next => "next",
        }
    }
}

/// What a browser lists.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Listing {
    /// The tickets that carry one state word.
    Word(String),
    /// What `/search` finds for the words.
    Search(String),
    Route(Route),
    /// An item and everything tied to it.
    Ties(String),
    /// The items of one group.
    Group(String),
}

impl Listing {
    #[must_use]
    pub fn title(&self) -> String {
        match self {
            Listing::Word(w) => w.clone(),
            Listing::Search(q) => format!("search {q}"),
            Listing::Route(r) => r.name().to_string(),
            Listing::Ties(id) => id.clone(),
            Listing::Group(name) => format!("group {name}"),
        }
    }
}

/// What a hot spot opens.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Item(String),
    List(Listing),
    Project(String),
    /// A project fact on the settings page, which Enter edits.
    Fact(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub text: String,
    pub style: Style,
    pub target: Option<Target>,
}

#[must_use]
pub fn seg(text: impl Into<String>, style: Style) -> Seg {
    Seg {
        text: text.into(),
        style,
        target: None,
    }
}

#[must_use]
pub fn spot(text: impl Into<String>, style: Style, target: Target) -> Seg {
    Seg {
        text: text.into(),
        style,
        target: Some(target),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Doc {
    pub lines: Vec<Vec<Seg>>,
}

impl Doc {
    pub fn line(&mut self, segs: Vec<Seg>) {
        self.lines.push(segs);
    }

    pub fn plain(&mut self, text: impl Into<String>, style: Style) {
        self.lines.push(vec![seg(text, style)]);
    }

    pub fn blank(&mut self) {
        self.lines.push(Vec::new());
    }

    /// Every hot spot in reading order: its line and what it opens.
    #[must_use]
    pub fn spots(&self) -> Vec<(usize, Target)> {
        let mut out = Vec::new();
        for (y, line) in self.lines.iter().enumerate() {
            out.extend(line.iter().filter_map(|s| s.target.clone().map(|t| (y, t))));
        }
        out
    }

    /// Where each hot spot sits, in the order of `spots`: its line, its first column and its width.
    #[must_use]
    pub fn places(&self) -> Vec<(usize, u16, u16)> {
        let mut out = Vec::new();
        for (y, line) in self.lines.iter().enumerate() {
            let mut x = 0u16;
            for s in line {
                let width = u16::try_from(Span::raw(s.text.as_str()).width()).unwrap_or(u16::MAX);
                if s.target.is_some() {
                    out.push((y, x, width));
                }
                x = x.saturating_add(width);
            }
        }
        out
    }

    /// The lines to draw, the selected spot reversed. An item spot is underlined, as a link reads.
    #[must_use]
    pub fn render(&self, selected: Option<usize>) -> Vec<Line<'static>> {
        let mut n = 0;
        let mut out = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            let spans: Vec<Span> = line
                .iter()
                .map(|s| {
                    let Some(target) = &s.target else {
                        return Span::styled(s.text.clone(), s.style);
                    };
                    let mut style = s.style.add_modifier(Modifier::BOLD);
                    if selected == Some(n) {
                        style = style.add_modifier(Modifier::REVERSED);
                    } else if matches!(target, Target::Item(_)) {
                        style = style.add_modifier(Modifier::UNDERLINED);
                    }
                    n += 1;
                    Span::styled(s.text.clone(), style)
                })
                .collect();
            out.push(Line::from(spans));
        }
        out
    }

    /// Prose wrapped to a width, each id the lookup knows a hot spot in its word's colour.
    pub fn prose(&mut self, text: &str, width: usize, lookup: &dyn Fn(&str) -> Option<Style>) {
        for line in wrap(text, width) {
            self.lines.push(linked(&line, Style::default(), lookup));
        }
    }
}

/// One line cut into plain runs and hot spots at the ids the lookup knows.
#[must_use]
pub fn linked(line: &str, base: Style, lookup: &dyn Fn(&str) -> Option<Style>) -> Vec<Seg> {
    let mut segs = Vec::new();
    let mut from = 0;
    for (start, end) in ids_in(line) {
        let id = &line[start..end];
        let Some(style) = lookup(id) else {
            continue;
        };
        if start > from {
            segs.push(seg(&line[from..start], base));
        }
        segs.push(spot(id, style, Target::Item(id.to_string())));
        from = end;
    }
    if from < line.len() {
        segs.push(seg(&line[from..], base));
    }
    segs
}

/// Byte ranges of every word shaped like an id: one to three capitals and a number, standing alone.
#[must_use]
pub fn ids_in(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_alphanumeric() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        if docket_core::search::is_id(&text[start..i]) {
            out.push((start, i));
        }
    }
    out
}

/// Text wrapped at spaces to a width, each paragraph keeping its indent and a list item hanging.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut out = Vec::new();
    for para in text.lines() {
        if para.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let indent = para.len() - para.trim_start().len();
        let hang = indent + bullet(para.trim_start());
        let mut line = para[..indent].to_string();
        for word in para.split_whitespace() {
            let used = line.chars().count();
            if used > hang && used + 1 + word.chars().count() > width {
                out.push(std::mem::replace(&mut line, " ".repeat(hang)));
            } else if used > indent && !line.ends_with(' ') {
                line.push(' ');
            }
            line.push_str(word);
        }
        out.push(line);
    }
    out
}

/// How far a list item's text hangs: `- `, `* ` or `12. `.
fn bullet(text: &str) -> usize {
    if text.starts_with("- ") || text.starts_with("* ") {
        return 2;
    }
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && text[digits..].starts_with(". ") {
        return digits + 2;
    }
    0
}

/// Where a pointer into a document's hot spots stands, and how far the document is scrolled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub index: Option<usize>,
    pub target: Option<Target>,
    pub top: usize,
}

impl Cursor {
    /// Finds the selected target again after a refetch, by what it opens, else by its place.
    pub fn settle(&mut self, spots: &[(usize, Target)]) {
        if let Some(t) = &self.target
            && let Some(i) = spots.iter().position(|(_, s)| s == t)
        {
            self.index = Some(i);
            return;
        }
        self.index = self
            .index
            .filter(|_| !spots.is_empty())
            .map(|i| i.min(spots.len() - 1));
        self.target = self.index.map(|i| spots[i].1.clone());
    }

    /// One spot on or back, wrapping round.
    pub fn step(&mut self, spots: &[(usize, Target)], forward: bool) {
        if spots.is_empty() {
            return;
        }
        let n = spots.len();
        let i = match (self.index, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        self.index = Some(i);
        self.target = Some(spots[i].1.clone());
    }

    /// Scrolls so the selected line shows in a window of the height.
    pub fn follow(&mut self, spots: &[(usize, Target)], height: usize) {
        let Some(y) = self.index.and_then(|i| spots.get(i)).map(|(y, _)| *y) else {
            return;
        };
        if y < self.top {
            self.top = y;
        } else if height > 0 && y >= self.top + height {
            self.top = y + 1 - height;
        }
    }

    pub fn scroll(&mut self, by: isize, lines: usize, height: usize) {
        let most = lines.saturating_sub(height);
        self.top = self.top.saturating_add_signed(by).min(most);
    }

    /// Lets go of the selected spot once it has scrolled out of a window of the height, so the window
    /// stays where it was put.
    pub fn release(&mut self, spots: &[(usize, Target)], height: usize) {
        let Some(y) = self.index.and_then(|i| spots.get(i)).map(|(y, _)| *y) else {
            return;
        };
        if y < self.top || y >= self.top + height {
            self.index = None;
            self.target = None;
        }
    }

    #[must_use]
    pub fn selected(&self) -> Option<&Target> {
        self.target.as_ref().filter(|_| self.index.is_some())
    }
}

#[cfg(test)]
#[path = "tests/doc.rs"]
mod tests;
