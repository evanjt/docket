//! The `:` line: any verb the command line has, read into the one request it makes or the page it opens.
//! A verb that takes an id runs on the selected item when the line names none.

use serde_json::{Map, Value, json};

use docket_core::search::is_id;

use crate::doc::{Listing, Route, Target};

use self::Slot::{Id, Ids, MaybeRest, Rest, Word};

/// What a palette line does.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// `POST /do/{verb}` with the body.
    Post(String, Value),
    Open(Target),
    Search(String),
    /// The project's settings page.
    Settings,
}

/// One positional argument of a verb.
#[derive(Clone, Copy)]
enum Slot {
    /// One id, the selected item's when the line names none.
    Id(&'static str),
    /// Every id named, else the marked rows or the selected one.
    Ids(&'static str),
    /// One word.
    Word(&'static str),
    /// The rest of the line.
    Rest(&'static str),
    /// The rest of the line, if any.
    MaybeRest(&'static str),
}

/// A write verb: the names it answers to, the route it posts to, its positionals and its flags.
struct Spec {
    names: &'static [&'static str],
    route: &'static str,
    slots: &'static [Slot],
    /// `(--flag, field)`; a field starting with `!` is a switch.
    flags: &'static [(&'static str, &'static str)],
}

const RUN: [(&str, &str); 2] = [("--runner", "runner"), ("--model", "model")];

const SPECS: [Spec; 21] = [
    Spec {
        names: &["new"],
        route: "new",
        slots: &[Word("key"), Rest("title")],
        flags: &[
            ("--body", "body"),
            ("--turn", "turn"),
            ("--complexity", "complexity"),
            ("--priority", "priority"),
            ("--theme", "theme"),
            ("--group", "group"),
        ],
    },
    Spec {
        names: &["add"],
        route: "add",
        slots: &[Rest("title")],
        flags: &[("--key", "key"), ("--body", "body"), ("--from", "from")],
    },
    Spec {
        names: &["start"],
        route: "start",
        slots: &[Id("id")],
        flags: &[
            ("--runner", "runner"),
            ("--job", "job"),
            ("--model", "model"),
            ("--on", "on"),
            ("--role", "role"),
        ],
    },
    Spec {
        names: &["release"],
        route: "release",
        slots: &[Id("id"), MaybeRest("note")],
        flags: &[
            ("--bounce", "!bounce"),
            ("--rebase", "rebase"),
            RUN[0],
            RUN[1],
        ],
    },
    Spec {
        names: &["close", "built"],
        route: "close",
        slots: &[Id("id"), MaybeRest("resolution")],
        flags: &[("--gates", "gates"), RUN[0], RUN[1]],
    },
    Spec {
        names: &["drop"],
        route: "drop",
        slots: &[Id("id"), MaybeRest("why")],
        flags: &[("--superseded-by", "superseded_by")],
    },
    Spec {
        names: &["reopen"],
        route: "reopen",
        slots: &[Id("id"), Rest("why")],
        flags: &[],
    },
    Spec {
        names: &["wait", "block"],
        route: "wait",
        slots: &[Id("id")],
        flags: &[("--on", "on"), ("--until", "until")],
    },
    Spec {
        names: &["resume"],
        route: "resume",
        slots: &[Id("id"), MaybeRest("note")],
        flags: &[],
    },
    Spec {
        names: &["ask", "park", "manual"],
        route: "ask",
        slots: &[Id("id"), Rest("note")],
        flags: &[],
    },
    Spec {
        names: &["reply"],
        route: "reply",
        slots: &[Id("id"), Rest("note")],
        flags: &[],
    },
    Spec {
        names: &["answer"],
        route: "answer",
        slots: &[Id("id"), Rest("decision")],
        flags: &[("--derived", "derived")],
    },
    Spec {
        names: &["decide"],
        route: "decide",
        slots: &[Id("id"), Rest("choice")],
        flags: &[("--basis", "basis")],
    },
    Spec {
        names: &["rate"],
        route: "rate",
        slots: &[Id("id"), Word("level")],
        flags: &[],
    },
    Spec {
        names: &["priority"],
        route: "priority",
        slots: &[Ids("ids"), Word("tier")],
        flags: &[],
    },
    Spec {
        names: &["edit"],
        route: "edit",
        slots: &[Id("id")],
        flags: &[
            ("--append", "append"),
            ("--body", "body"),
            ("--set", "*set"),
        ],
    },
    Spec {
        names: &["link"],
        route: "link",
        slots: &[Ids("a"), Word("kind"), Id("b")],
        flags: &[("--remove", "!remove")],
    },
    Spec {
        names: &["key"],
        route: "key",
        slots: &[Word("key"), Word("kind"), Rest("meaning")],
        flags: &[("--turn", "turn")],
    },
    Spec {
        names: &["reindex"],
        route: "reindex",
        slots: &[],
        flags: &[],
    },
    Spec {
        names: &["retry"],
        route: "retry",
        slots: &[Id("id"), MaybeRest("note")],
        flags: &[],
    },
    Spec {
        names: &["skills"],
        route: "fact",
        slots: &[Word("key"), MaybeRest("value")],
        flags: &[],
    },
];

/// The list routes the palette opens as a page.
const LISTS: [(&str, Route); 12] = [
    ("next", Route::Next),
    ("todo", Route::Todo),
    ("questions", Route::Questions),
    ("q", Route::Questions),
    ("wip", Route::Wip),
    ("waiting", Route::Waiting),
    ("blocked", Route::Waiting),
    ("research", Route::Research),
    ("derived", Route::Derived),
    ("done", Route::Done),
    ("dropped", Route::Dropped),
    ("groups", Route::Groups),
];

/// What the line asks for, in the project, on the selection: the marked rows, else the selected one,
/// whose id comes first.
///
/// # Errors
/// An unknown verb or flag, a missing argument, or an id the selection cannot supply, in words.
pub fn parse(line: &str, project: &str, selection: &[String]) -> Result<Command, String> {
    let words = split(line)?;
    let Some((verb, rest)) = words.split_first() else {
        return Err("type a verb, as close T3 abc1234 or priority high".into());
    };
    let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
    if let Some(cmd) = navigation(verb, &rest, project, selection)? {
        return Ok(cmd);
    }
    let Some(spec) = SPECS.iter().find(|s| s.names.contains(&verb.as_str())) else {
        return Err(format!(
            "docket {verb} is not a verb the screen runs: run it in a terminal"
        ));
    };
    let rest = if spec.route == "fact" && rest.first() == Some(&"set") {
        &rest[1..]
    } else {
        &rest[..]
    };
    let mut body = Map::new();
    body.insert("project".into(), json!(project));
    body.insert("force".into(), json!(false));
    let positional = flags(spec, rest, &mut body)?;
    slots(verb, spec, &positional, selection, &mut body)?;
    Ok(Command::Post(spec.route.to_string(), Value::Object(body)))
}

/// The reads the screen has a page for.
fn navigation(
    verb: &str,
    rest: &[&str],
    project: &str,
    selection: &[String],
) -> Result<Option<Command>, String> {
    let first = rest.first().copied().filter(|w| is_id(w));
    let id = || {
        first
            .map(str::to_string)
            .or_else(|| selection.first().cloned())
            .ok_or_else(|| format!("{verb} needs an id: select a row or name one"))
    };
    Ok(Some(match verb {
        "show" | "log" => Command::Open(Target::Item(id()?)),
        "status" => Command::Open(Target::Project(project.to_string())),
        "skills" if rest.is_empty() || rest == ["show"] => Command::Settings,
        "deps" | "audit" if first.is_some() || rest.is_empty() => {
            Command::Open(Target::List(Listing::Ties(id()?)))
        }
        "search" if !rest.is_empty() => Command::Search(rest.join(" ")),
        "groups" if !rest.is_empty() => Command::Open(Target::List(Listing::Group(rest.join(" ")))),
        _ => match LISTS.iter().find(|(name, _)| *name == verb) {
            Some((_, route)) => Command::Open(Target::List(Listing::Route(*route))),
            None => return Ok(None),
        },
    }))
}

/// The flags read into the body; what is left is positional.
fn flags<'a>(
    spec: &Spec,
    words: &[&'a str],
    body: &mut Map<String, Value>,
) -> Result<Vec<&'a str>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        i += 1;
        if !w.starts_with("--") || w == "--" {
            out.push(w);
            continue;
        }
        let (name, inline) = match w.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (w, None),
        };
        if name == "--force" {
            body.insert("force".into(), json!(true));
            continue;
        }
        let Some((_, field)) = spec.flags.iter().find(|(f, _)| *f == name) else {
            return Err(format!("{} takes no {name}", spec.names[0]));
        };
        if let Some(switch) = field.strip_prefix('!') {
            body.insert(switch.into(), json!(true));
            continue;
        }
        let value = if let Some(v) = inline {
            v
        } else {
            let v = words
                .get(i)
                .ok_or_else(|| format!("{name} needs a value"))?;
            i += 1;
            *v
        };
        flag_value(field, value, body)?;
    }
    Ok(out)
}

/// A flag's value in its field; `--set FIELD=VALUE` adds to a list.
fn flag_value(field: &str, value: &str, body: &mut Map<String, Value>) -> Result<(), String> {
    let Some(list) = field.strip_prefix('*') else {
        body.insert(field.into(), json!(value));
        return Ok(());
    };
    let Some((f, v)) = value.split_once('=') else {
        return Err(format!("--{list} takes FIELD=VALUE, not {value}"));
    };
    let entry = json!({ "field": f, "value": v });
    if let Value::Array(a) = body.entry(list).or_insert_with(|| json!([])) {
        a.push(entry);
    }
    Ok(())
}

/// The positionals read into their slots, the selection standing in for an id not named.
fn slots(
    verb: &str,
    spec: &Spec,
    words: &[&str],
    selection: &[String],
    body: &mut Map<String, Value>,
) -> Result<(), String> {
    let mut at = 0;
    let need = |name: &str| format!("{verb} needs {name}");
    for slot in spec.slots {
        match *slot {
            Id(field) => {
                let id = if words.get(at).is_some_and(|w| is_id(w)) {
                    at += 1;
                    words[at - 1].to_string()
                } else {
                    selection
                        .first()
                        .cloned()
                        .ok_or_else(|| need("an id: select a row or name one"))?
                };
                body.insert(field.into(), json!(id));
            }
            Ids(field) => {
                let named: Vec<String> = words[at..]
                    .iter()
                    .take_while(|w| is_id(w))
                    .map(|w| (*w).to_string())
                    .collect();
                at += named.len();
                let ids = if named.is_empty() {
                    selection.to_vec()
                } else {
                    named
                };
                if ids.is_empty() {
                    return Err(need("ids: mark rows or name them"));
                }
                body.insert(field.into(), json!(ids));
            }
            Word(field) => {
                let w = words.get(at).ok_or_else(|| need(field))?;
                at += 1;
                body.insert(field.into(), json!(w));
            }
            Rest(field) | MaybeRest(field) => {
                let text = words[at..].join(" ");
                at = words.len();
                if text.is_empty() && matches!(slot, Rest(_)) {
                    return Err(need(field));
                }
                if !text.is_empty() {
                    body.insert(field.into(), json!(text));
                }
            }
        }
    }
    if at < words.len() {
        return Err(format!("{verb} takes no {}", words[at..].join(" ")));
    }
    Ok(())
}

/// Words split at spaces, a quoted run kept whole.
///
/// # Errors
/// A quote left open.
pub fn split(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut word: Option<String> = None;
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => {
                quote = Some(c);
                word.get_or_insert_with(String::new);
            }
            (None, c) if c.is_whitespace() => out.extend(word.take()),
            (_, c) => word.get_or_insert_with(String::new).push(c),
        }
    }
    if quote.is_some() {
        return Err("a quote is left open".into());
    }
    out.extend(word);
    Ok(out)
}

#[cfg(test)]
#[path = "tests/palette.rs"]
mod tests;
