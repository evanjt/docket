//! JSON as Python's `json.dumps` writes it, so a stored value reads the same from either writer.

use std::fmt::Write;

use serde_json::{Map, Value};

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
    let mut out = String::new();
    write(value, sort_keys, &mut out);
    out
}

fn write(value: &Value, sort_keys: bool, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => write_str(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write(item, sort_keys, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            if sort_keys {
                entries.sort_by(|a, b| a.0.cmp(b.0));
            }
            out.push('{');
            for (i, (k, v)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_str(k, out);
                out.push_str(": ");
                write(v, sort_keys, out);
            }
            out.push('}');
        }
    }
}

fn write_str(s: &str, out: &mut String) {
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
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
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
