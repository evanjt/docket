//! `docket machines` and `docket machine set|remove`: the machines jobs run on, as the server holds
//! them, with the one this client runs on marked.

use std::fmt::Write as _;

use docket_core::api::{MachineRequest, Machines};
use docket_core::clock;
use docket_core::machine::{self, Machine};

use crate::ctx::Ctx;
use crate::fail::Result;
use crate::py::Py;

/// Every machine, by name. The one whose name is this key's host is marked.
///
/// # Errors
/// The server cannot be reached.
pub fn machines(ctx: &Ctx) -> Result<i32> {
    let v = ctx.api.get("/machines", &[])?;
    if ctx.json {
        ctx.emit(&Py::from_value(&v));
        return Ok(0);
    }
    let listed: Machines = serde_json::from_value(v).unwrap_or_default();
    print!(
        "{}",
        machines_text(&listed.machines, &ctx.host()?, &clock::now())
    );
    Ok(0)
}

/// One machine set or removed; the machines as they stand after.
///
/// # Errors
/// The server refuses: an agent's key, a machine the rules refuse, or removing an unknown one.
pub fn machine(ctx: &Ctx, req: &MachineRequest) -> Result<i32> {
    let out: Machines = ctx.api.post("machine", req)?;
    if ctx.json {
        let v = serde_json::to_value(&out).unwrap_or_default();
        ctx.emit(&Py::from_value(&v));
        return Ok(0);
    }
    print!(
        "{}",
        machines_text(&out.machines, &ctx.host()?, &clock::now())
    );
    Ok(0)
}

/// The request a `docket machine` line makes: only the fields given, runners read from `a,b`.
#[must_use]
pub fn set_request(
    name: &str,
    ssh: Option<&String>,
    slots: Option<i64>,
    runners: Option<&str>,
    note: Option<&String>,
    path: Option<&String>,
    remove: bool,
) -> MachineRequest {
    MachineRequest {
        set: machine::Set {
            name: name.to_string(),
            ssh: ssh.cloned(),
            slots,
            runners: runners.map(machine::runners_of),
            note: note.cloned(),
            path: path.cloned(),
        },
        remove,
    }
}

/// The machines as a table, `*` against the one named `here`, and each runner under a usage limit
/// at `now` with the reset it named.
#[must_use]
pub fn machines_text(machines: &[Machine], here: &str, now: &str) -> String {
    if machines.is_empty() {
        return "no machines: docket machine set NAME --ssh ADDRESS --slots N --runners claude,codex\n"
            .to_string();
    }
    let runners: Vec<String> = machines.iter().map(|m| m.runners.join(",")).collect();
    let name_w = machines
        .iter()
        .map(|m| m.name.len())
        .max()
        .unwrap_or(0)
        .max(4);
    let run_w = runners.iter().map(String::len).max().unwrap_or(0).max(7);
    let mut out = format!("  {:<name_w$}  SLOTS  {:<run_w$}  SSH\n", "NAME", "RUNNERS");
    for (m, r) in machines.iter().zip(&runners) {
        let mark = if m.name == here { "*" } else { " " };
        let _ = write!(
            out,
            "{mark} {:<name_w$}  {:>5}  {r:<run_w$}  {}",
            m.name, m.slots, m.ssh
        );
        if let Some(note) = &m.note {
            let _ = write!(out, "  {note}");
        }
        for runner in &m.runners {
            if let Some(until) = m.limited(runner, now) {
                let _ = write!(out, "  {runner} limited until {until}");
            }
        }
        out.push('\n');
    }
    if machines.iter().any(|m| m.name == here) {
        out.push_str("* is this machine\n");
    } else {
        let _ = writeln!(out, "{here}, this machine, is not one of them");
    }
    out
}

#[cfg(test)]
#[path = "../tests/machines.rs"]
mod tests;
