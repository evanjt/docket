//! JSON as Python's `json.dumps` writes it, so a stored value reads the same from either writer.

use std::fmt::Write;

use serde_json::{Map, Value};

/// The `json.dumps` arguments that change the text: `sort_keys`, `ensure_ascii` and `indent`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub sort_keys: bool,
    pub ascii: bool,
    pub indent: Option<usize>,
}

/// An event's structured fields, from the stored text or nothing.
#[must_use]
pub fn data_of(data: Option<&str>) -> Map<String, Value> {
    match data.map(serde_json::from_str::<Value>) {
        Some(Ok(Value::Object(m))) => m,
        _ => Map::new(),
    }
}

/// The value with `, ` and `: ` separators and non-ASCII escaped, keys sorted when asked.
#[must_use]
pub fn dumps(value: &Value, sort_keys: bool) -> String {
    dumps_styled(
        value,
        Style {
            sort_keys,
            ascii: true,
            indent: None,
        },
    )
}

/// The value as `json.dumps` writes it with the given arguments.
#[must_use]
pub fn dumps_styled(value: &Value, style: Style) -> String {
    let mut out = String::new();
    write(value, style, 0, &mut out);
    out
}

fn write(value: &Value, style: Style, depth: usize, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => write_str(s, style.ascii, out),
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|item| {
                    let mut part = String::new();
                    write(item, style, depth + 1, &mut part);
                    part
                })
                .collect();
            write_container(&parts, ('[', ']'), style, depth, out);
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            if style.sort_keys {
                entries.sort_by(|a, b| a.0.cmp(b.0));
            }
            let parts: Vec<String> = entries
                .into_iter()
                .map(|(k, v)| {
                    let mut part = String::new();
                    write_str(k, style.ascii, &mut part);
                    part.push_str(": ");
                    write(v, style, depth + 1, &mut part);
                    part
                })
                .collect();
            write_container(&parts, ('{', '}'), style, depth, out);
        }
    }
}

/// Written parts between brackets: on one line, or one per line under `indent`.
fn write_container(
    parts: &[String],
    (open, close): (char, char),
    style: Style,
    depth: usize,
    out: &mut String,
) {
    out.push(open);
    match style.indent {
        Some(width) if !parts.is_empty() => {
            let inner = format!("\n{}", " ".repeat(width * (depth + 1)));
            out.push_str(&inner);
            out.push_str(&parts.join(&format!(",{inner}")));
            out.push('\n');
            out.push_str(&" ".repeat(width * depth));
        }
        _ => out.push_str(&parts.join(", ")),
    }
    out.push(close);
}

fn write_str(s: &str, ascii: bool, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (ascii && (c as u32) > 0x7e) => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
#[path = "tests/pyjson.rs"]
mod tests;
