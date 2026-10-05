//! Where the work ref's landings are cut into published groups.

use std::collections::{BTreeMap, BTreeSet};

/// One item a snapshot brought in, with the plan it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Landed {
    pub item: String,
    pub plan: Option<String>,
}

/// A first-parent snapshot of the work ref and the items whose close sha it brought in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub sha: String,
    pub date: String,
    pub items: Vec<Landed>,
}

/// One published group: the snapshot it ends on and what that says about plans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub end: String,
    pub date: String,
    /// Plans whose last open item landed within the group.
    pub finished: Vec<String>,
    /// Plans with items landed in the group that are not finished by its end.
    pub partial: Vec<String>,
}

/// Cuts `snapshots`, oldest first, into the commit groups. A group ends on a snapshot where a
/// plan with no open item left landed its last item, and the last group ends on the final
/// snapshot. `open` counts each plan's items still open; a plan absent from it has none. With a
/// `cap`, the adjacent pair covering the fewest snapshots is merged until at most that many
/// groups remain. Snapshots keep their order.
#[must_use]
pub fn cut(
    snapshots: &[Snapshot],
    open: &BTreeMap<String, usize>,
    cap: Option<usize>,
) -> Vec<Group> {
    if snapshots.is_empty() {
        return Vec::new();
    }
    let mut last: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, snap) in snapshots.iter().enumerate() {
        for plan in snap.items.iter().filter_map(|l| l.plan.as_deref()) {
            last.insert(plan, i);
        }
    }
    let done = |plan: &str| open.get(plan).copied().unwrap_or(0) == 0;
    let tip = snapshots.len() - 1;
    let mut ends: BTreeSet<usize> = last
        .iter()
        .filter(|(plan, _)| done(plan))
        .map(|(_, &i)| i)
        .collect();
    ends.insert(tip);
    // Each span is the first and last snapshot index of a group.
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for end in ends {
        spans.extend([(start, end)]);
        start = end + 1;
    }
    while spans.len() > cap.unwrap_or(usize::MAX).max(1) {
        let at = (0..spans.len() - 1)
            .min_by_key(|&i| spans[i + 1].1 - spans[i].0)
            .unwrap_or(0);
        spans[at].1 = spans[at + 1].1;
        spans.remove(at + 1);
    }
    spans
        .into_iter()
        .map(|(from, to)| {
            let mut landed: BTreeSet<&str> = BTreeSet::new();
            for snap in &snapshots[from..=to] {
                landed.extend(snap.items.iter().filter_map(|l| l.plan.as_deref()));
            }
            let (finished, partial): (Vec<&str>, Vec<&str>) = landed
                .into_iter()
                .partition(|plan| done(plan) && last[plan] <= to);
            Group {
                end: snapshots[to].sha.clone(),
                date: snapshots[to].date.clone(),
                finished: finished.into_iter().map(str::to_string).collect(),
                partial: partial.into_iter().map(str::to_string).collect(),
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/cut.rs"]
mod tests;
