//! Output as Python prints it: JSON from `json.dumps`, floats from `repr`, strings cut by character.

use std::fmt::Write;

use serde_json::Value;

/// A JSON value whose objects keep the order they were built in.
#[derive(Clone, Debug, PartialEq)]
pub enum Py {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Py>),
    Dict(Vec<(String, Py)>),
}

impl Py {
    /// A server value, its objects in the order serde gives them.
    #[must_use]
    pub fn from_value(v: &Value) -> Self {
        match v {
            Value::Null => Py::Null,
            Value::Bool(b) => Py::Bool(*b),
            Value::Number(n) => match n.as_i64() {
                Some(i) => Py::Int(i),
                None => Py::Float(n.as_f64().unwrap_or(0.0)),
            },
            Value::String(s) => Py::Str(s.clone()),
            Value::Array(a) => Py::List(a.iter().map(Py::from_value).collect()),
            Value::Object(o) => Py::Dict(
                o.iter()
                    .map(|(k, v)| (k.clone(), Py::from_value(v)))
                    .collect(),
            ),
        }
    }

    /// An object with the named fields of `v`, in that order, a missing one as null.
    #[must_use]
    pub fn pick(v: &Value, keys: &[&str]) -> Self {
        Py::Dict(
            keys.iter()
                .map(|k| ((*k).to_string(), Py::from_value(&v[*k])))
                .collect(),
        )
    }

    pub fn str(s: impl Into<String>) -> Self {
        Py::Str(s.into())
    }

    /// A list of strings.
    #[must_use]
    pub fn strs<S: AsRef<str>>(items: &[S]) -> Self {
        Py::List(items.iter().map(|s| Py::str(s.as_ref())).collect())
    }
}

/// `json.dumps(value, indent=2, ensure_ascii=False)`.
#[must_use]
pub fn dumps_indent(v: &Py) -> String {
    let mut out = String::new();
    write(v, Some(2), 0, &mut out);
    out
}

/// `json.dumps(value, ensure_ascii=False)`, on one line.
#[must_use]
pub fn dumps_line(v: &Py) -> String {
    let mut out = String::new();
    write(v, None, 0, &mut out);
    out
}

fn write(v: &Py, indent: Option<usize>, depth: usize, out: &mut String) {
    match v {
        Py::Null => out.push_str("null"),
        Py::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Py::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Py::Float(f) => out.push_str(&float_repr(*f)),
        Py::Str(s) => write_str(s, out),
        Py::List(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|x| {
                    let mut s = String::new();
                    write(x, indent, depth + 1, &mut s);
                    s
                })
                .collect();
            container('[', ']', &parts, indent, depth, out);
        }
        Py::Dict(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|(k, x)| {
                    let mut s = String::new();
                    write_str(k, &mut s);
                    s.push_str(": ");
                    write(x, indent, depth + 1, &mut s);
                    s
                })
                .collect();
            container('{', '}', &parts, indent, depth, out);
        }
    }
}

fn container(
    open: char,
    close: char,
    parts: &[String],
    indent: Option<usize>,
    depth: usize,
    out: &mut String,
) {
    out.push(open);
    if parts.is_empty() {
        out.push(close);
        return;
    }
    match indent {
        None => out.push_str(&parts.join(", ")),
        Some(n) => {
            let inner = " ".repeat(n * (depth + 1));
            out.push('\n');
            out.push_str(&inner);
            out.push_str(&parts.join(&format!(",\n{inner}")));
            out.push('\n');
            out.push_str(&" ".repeat(n * depth));
        }
    }
    out.push(close);
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
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A float as Python's `repr` prints it: the shortest digits that read back, fixed between 1e-4 and
/// 1e16, an exponent of at least two digits outside.
#[must_use]
pub fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let sci = format!("{f:e}");
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let neg = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let sign = if neg { "-" } else { "" };
    if (-4..16).contains(&exp) {
        return format!("{sign}{}", fixed(&digits, exp));
    }
    let (head, tail) = digits.split_at(1);
    let point = if tail.is_empty() {
        String::new()
    } else {
        format!(".{tail}")
    };
    let esign = if exp < 0 { '-' } else { '+' };
    format!("{sign}{head}{point}e{esign}{:02}", exp.unsigned_abs())
}

fn fixed(digits: &str, exp: i32) -> String {
    if exp < 0 {
        let zeros = "0".repeat(usize::try_from(-exp - 1).unwrap_or(0));
        return format!("0.{zeros}{digits}");
    }
    let whole = usize::try_from(exp).unwrap_or(0) + 1;
    if digits.len() <= whole {
        format!("{digits}{}.0", "0".repeat(whole - digits.len()))
    } else {
        format!("{}.{}", &digits[..whole], &digits[whole..])
    }
}

/// The first n characters, as a Python slice reads them.
#[must_use]
pub fn cut(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// An optional value as an f-string prints it: `None` when absent.
#[must_use]
pub fn or_none(v: Option<&str>) -> &str {
    v.unwrap_or("None")
}

#[cfg(test)]
#[path = "tests/py.rs"]
mod tests;
