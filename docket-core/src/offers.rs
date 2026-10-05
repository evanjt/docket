//! The verbs an item takes as it stands, and what each needs: one answer for every client, drawn from
//! the refusals in `rules`.

use serde::Serialize;

use crate::item::Item;
use crate::word::Kind;

/// One verb an item takes now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Offer {
    pub verb: &'static str,
    /// The request fields the verb is refused without; `until|on` is one of the two.
    pub needs: Vec<&'static str>,
    /// The branch the request carries: the holder's when another branch holds the item.
    pub branch: Option<String>,
}

/// The verbs the rules accept on the item as it stands, in the order a panel lists them. A verb on a
/// claimed item carries the holder's branch, since `rules::require_hold` refuses any other. A plan's
/// `open_under` ids are what `rules::close` takes: `close` is left out where it refuses.
#[must_use]
pub fn offers(row: &Item, kind: Kind, open_under: &[String]) -> Vec<Offer> {
    let offer = |verb, needs: &[&'static str], branch: Option<&String>| Offer {
        verb,
        needs: needs.to_vec(),
        branch: branch.cloned(),
    };
    if row.state != "open" {
        return vec![offer("reopen", &["why"], None)];
    }
    let held = row.claim_branch.as_ref();
    let parked = row.turn.as_deref() == Some("user");
    let mut out = Vec::new();
    if kind == Kind::Decision {
        out.push(offer("answer", &["decision"], None));
    } else if parked {
        out.push(offer("reply", &["note"], None));
    }
    if held.is_none() && row.wait_on.is_none() && !parked && row.conflict == 0 {
        out.push(offer("start", &[], None));
    }
    if row.wait_on.is_some() {
        out.push(offer("resume", &[], None));
    }
    let closable = match kind {
        Kind::Decision => row.decision.is_some(),
        Kind::Audit => held.is_some() || open_under.is_empty(),
        _ => true,
    };
    if closable {
        out.push(offer("close", &["resolution"], held));
    }
    if held.is_some() {
        out.push(offer("release", &[], held));
    }
    if !parked {
        out.push(offer("ask", &["note"], held));
    }
    if row.wait_on.is_none() {
        out.push(offer("wait", &["until|on"], held));
    }
    out.push(offer("drop", &["why"], held));
    out
}

#[cfg(test)]
#[path = "tests/offers.rs"]
mod tests;
