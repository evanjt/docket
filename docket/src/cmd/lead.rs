//! `docket lead show|take|renew|give`: the project's lead claim. The session defaults to the branch
//! acting, so a lead working from one checkout renews its own claim without naming it.

use std::fmt::Write as _;

use docket_core::api::{LeadRequest, LeadState};
use docket_core::lead::Lead;

use crate::ctx::Ctx;
use crate::fail::Result;
use crate::py::Py;

/// Shows the claim, or takes, renews or gives it back.
///
/// # Errors
/// The project cannot be resolved, or the server refuses the act.
pub fn lead(ctx: &mut Ctx, what: &str, session: Option<&String>) -> Result<i32> {
    let state: LeadState = if what == "show" {
        let q = ctx.query(&[])?;
        serde_json::from_value(ctx.api.get("/lead", &q)?).unwrap_or_default()
    } else {
        let common = ctx.common(false)?;
        let session = session
            .cloned()
            .or_else(|| common.branch.clone())
            .unwrap_or_default();
        let req = LeadRequest {
            common,
            act: what.to_string(),
            session,
        };
        ctx.api.post("lead", &req)?
    };
    if ctx.json {
        let v = serde_json::to_value(&state).unwrap_or_default();
        ctx.emit(&Py::from_value(&v));
    } else {
        print!("{}", lead_text(&state));
    }
    Ok(0)
}

/// What an act did, if one was asked, then who leads and when the claim lapses.
#[must_use]
pub fn lead_text(state: &LeadState) -> String {
    let slug = &state.project;
    let mut out = String::new();
    match (state.outcome.as_deref(), &state.previous) {
        (Some("took over"), Some(was)) => {
            let _ = writeln!(
                out,
                "took over the lead of {slug} from {} on {}",
                was.session, was.host
            );
        }
        (Some("took"), _) => {
            let _ = writeln!(out, "took the lead of {slug}");
        }
        (Some("renewed"), _) => {
            let _ = writeln!(out, "renewed the lead of {slug}");
        }
        (Some("gave"), _) => {
            let _ = writeln!(out, "gave back the lead of {slug}");
        }
        _ => {}
    }
    let Some(lead) = &state.lead else {
        let _ = writeln!(out, "no lead holds {slug}: docket lead take");
        return out;
    };
    out.push_str(&holder(slug, lead));
    let at = state.lapses_at.as_deref().unwrap_or("?");
    if state.lapsed {
        let _ = writeln!(
            out,
            "it lapsed at {at}: the next docket lead take takes it over"
        );
    } else {
        let _ = writeln!(
            out,
            "last renewed {}; it lapses at {at} unless renewed",
            lead.renewed_at
        );
    }
    out
}

fn holder(slug: &str, lead: &Lead) -> String {
    let branch = lead
        .branch
        .as_deref()
        .map(|b| format!(", branch {b}"))
        .unwrap_or_default();
    format!(
        "{slug} is led by {} on {}{branch}, since {}\n",
        lead.session, lead.host, lead.since
    )
}

#[cfg(test)]
#[path = "../tests/lead.rs"]
mod tests;
