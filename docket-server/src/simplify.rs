//! The data moved onto the simple model, after an import: packages become plans, a review claim
//! is given back, a plan that held its audit round closes, every plan is gated on what it opened, and
//! the inbox, later and the release fold into priority. Ids and keys never change. A dry run makes the
//! same writes inside a transaction it rolls back, so its report is the write's.

use std::collections::BTreeSet;

use sea_orm::DatabaseConnection;
use serde_json::Value as Json;

use docket_core::item::{Ctx, Field, Item};
use docket_core::migrate::version;
use docket_core::rules::{self, GATE, prioritise};
use docket_core::word::{Kind, priority};

use crate::store::{Tx, column, json, scalar};
use crate::verbs::Failure;
use crate::verbs::graph::{held_wait, keys_of, release_waiters, settle_audits};

/// The note on a package turned into a plan; it also marks a plan whose round audits are a package's.
const BECAME_A_PLAN: &str = "a plan now: the simple model makes plans the one grouping, audited once when everything it opened is closed";

/// What the step changes in one project, by id in key and number order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub project: String,
    /// Package keys that hold plans now.
    pub plan_keys: Vec<String>,
    /// Open packages that are open plans now.
    pub open_plans: usize,
    /// Review claims on packages given back.
    pub released: Vec<String>,
    /// Plans that held their one audit round, closed.
    pub closed: Vec<String>,
    /// Plans now waiting on what they opened.
    pub gated: Vec<String>,
    /// Plans that waited with nothing open under them, now due for their audit.
    pub due: Vec<String>,
    /// Items taken out of the inbox or later.
    pub unscoped: Vec<String>,
    /// Items at normal priority moved to low: out of the inbox or later, or outside the release.
    pub lowered: Vec<String>,
    /// The release fact removed.
    pub release: Option<String>,
    /// The releases fact written in its place: the release, then the version-named themes in order.
    pub releases: Option<String>,
}

impl Change {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self
            == Change {
                project: self.project.clone(),
                ..Change::default()
            }
    }

    /// What the step does to the project, a line per kind of change.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        if self.is_empty() {
            return vec![format!("{}: nothing to change", self.project)];
        }
        let mut out = vec![format!("{}:", self.project)];
        let ids = |v: &[String]| {
            let shown: Vec<&str> = v.iter().take(12).map(String::as_str).collect();
            let more = v.len().saturating_sub(12);
            if more > 0 {
                format!("{} and {more} more", shown.join(", "))
            } else {
                shown.join(", ")
            }
        };
        if !self.plan_keys.is_empty() {
            out.push(format!(
                "  keys {} hold plans now; {} open packages are open plans",
                self.plan_keys.join(", "),
                self.open_plans
            ));
        }
        for (n, v) in [
            ("review claims given back", &self.released),
            ("plans closed, their audit round held", &self.closed),
            ("plans waiting on what they opened", &self.gated),
            ("plans due for their audit", &self.due),
            ("items out of the inbox or later", &self.unscoped),
            ("items lowered to low priority", &self.lowered),
        ] {
            if !v.is_empty() {
                out.push(format!("  {} {n}: {}", v.len(), ids(v)));
            }
        }
        if let Some(r) = &self.release {
            out.push(format!("  release fact {r} removed"));
        }
        if let Some(r) = &self.releases {
            out.push(format!("  releases set to {r}"));
        }
        out
    }
}

/// Every project moved onto the simple model, or, without `write`, what that would change.
///
/// # Errors
/// The database refuses a read or a write.
pub async fn simplify(
    db: &DatabaseConnection,
    host: &str,
    write: bool,
) -> Result<Vec<Change>, String> {
    let mut tx = Tx::begin(db, host).await.map_err(|e| e.to_string())?;
    let slugs: Vec<String> = column(&tx.conn, "SELECT slug FROM projects ORDER BY slug", vec![])
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for slug in slugs {
        out.push(project(&mut tx, &slug).await.map_err(|e| match e {
            Failure::Refused(t)
            | Failure::NotFound(t)
            | Failure::Forbidden(t)
            | Failure::Invalid(t) => {
                format!("{slug}: {t}")
            }
            Failure::Db(e) => format!("{slug}: {e}"),
        })?);
    }
    if write {
        tx.commit().await.map_err(|e| e.to_string())?;
    } else {
        tx.conn.rollback().await.map_err(|e| e.to_string())?;
    }
    Ok(out)
}

async fn project(tx: &mut Tx, slug: &str) -> Result<Change, Failure> {
    let mut c = Change {
        project: slug.to_string(),
        ..Change::default()
    };
    let before = tx.project(slug).await?;
    let packages = keys_of(&before, &[Kind::Package]);
    let plans_proper = keys_of(&before, &[Kind::Audit]);
    if !packages.is_empty() {
        packages_to_plans(tx, slug, &packages, &mut c).await?;
    }
    if let Some(refused) = held_wait(&tx.conn, &tx.project(slug).await?).await? {
        return Err(refused);
    }
    close_held_rounds(tx, slug, &plans_proper, &mut c).await?;
    gate(tx, slug, &mut c).await?;
    fold_scopes(tx, slug, &mut c).await?;
    fold_release(tx, slug, &mut c).await?;
    Ok(c)
}

fn marks(n: usize) -> String {
    vec!["?"; n].join(", ")
}

async fn open_of(tx: &Tx, slug: &str, keys: &[String]) -> Result<Vec<Item>, Failure> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let mut values: Vec<sea_orm::Value> = vec![slug.into()];
    values.extend(keys.iter().map(|k| k.clone().into()));
    Ok(tx
        .items(
            &format!(
                "SELECT * FROM items WHERE project=? AND state='open' AND key IN ({}) ORDER BY key, num",
                marks(keys.len())
            ),
            values,
        )
        .await?)
}

/// The package keys turned into plan keys, each open package noted, and a review claim given back.
async fn packages_to_plans(
    tx: &mut Tx,
    slug: &str,
    packages: &[String],
    c: &mut Change,
) -> Result<(), Failure> {
    let keys: Json = scalar(
        &tx.conn,
        "SELECT keys FROM projects WHERE slug=?",
        vec![slug.into()],
    )
    .await?
    .unwrap_or_default();
    let turned: Vec<Json> = keys
        .as_array()
        .into_iter()
        .flatten()
        .map(|k| {
            let mut k = k.clone();
            if k["kind"] == "package" {
                k["kind"] = Json::from("audit");
            }
            k
        })
        .collect();
    tx.execute(
        "UPDATE projects SET keys=?, updated_at=? WHERE slug=?",
        vec![
            json(Json::Array(turned)),
            tx.now.clone().into(),
            slug.into(),
        ],
    )
    .await?;
    tx.touch_project(slug);
    c.plan_keys = packages.to_vec();
    let open = open_of(tx, slug, packages).await?;
    c.open_plans = open.len();
    for p in open {
        tx.touch(p.rid);
        tx.event(slug, Some(p.rid), "edited", Some(BECAME_A_PLAN), None, None)
            .await?;
        if let Some(branch) = &p.claim_branch {
            tx.update(p.rid, &rules::unclaimed()).await?;
            tx.event(
                slug,
                Some(p.rid),
                "released",
                Some("a package's review claim given back: the simple model audits a plan once, when everything it opened is closed"),
                Some(branch),
                None,
            )
            .await?;
            c.released.push(p.id.clone());
        }
    }
    Ok(())
}

/// A plan proper whose audit round is recorded has held its one round: it closes, its gaps left open.
/// A plan that was a package is passed over: its rounds audited landed work, not the finished plan.
async fn close_held_rounds(
    tx: &mut Tx,
    slug: &str,
    plans: &[String],
    c: &mut Change,
) -> Result<(), Failure> {
    let forced = Ctx {
        host: tx.host.clone(),
        branch: "main".to_string(),
        now: tx.now.clone(),
        force: true,
    };
    for a in open_of(tx, slug, plans).await? {
        let was_a_package: Option<i64> = scalar(
            &tx.conn,
            "SELECT COUNT(*) FROM events WHERE rid=? AND kind='edited' AND note=?",
            vec![a.rid.into(), BECAME_A_PLAN.into()],
        )
        .await?;
        if was_a_package.unwrap_or(0) > 0 {
            continue;
        }
        let round: Option<Option<Json>> = scalar(
            &tx.conn,
            "SELECT data FROM events WHERE rid=? AND kind='released' AND data->>'audit'='gaps' \
             ORDER BY seq DESC LIMIT 1",
            vec![a.rid.into()],
        )
        .await?;
        let Some(data) = round else { continue };
        let gaps: Vec<&str> = data
            .as_ref()
            .and_then(|d| d["gaps"].as_array())
            .into_iter()
            .flatten()
            .filter_map(Json::as_str)
            .collect();
        let held = if gaps.is_empty() {
            "its audit round held".to_string()
        } else {
            format!("its audit round held, gaps {}", gaps.join(", "))
        };
        let resolution = format!("{held}: the simple model audits a plan once, in one round");
        let cols = rules::close(&a, &forced, Some(&resolution), Kind::Audit)?;
        let row = tx.update(a.rid, &cols).await?;
        tx.event(slug, Some(a.rid), "closed", Some(&resolution), None, None)
            .await?;
        let p = tx.project(slug).await?;
        release_waiters(tx, &p, &row, "closed").await?;
        c.closed.push(a.id);
    }
    Ok(())
}

/// Every open plan settled: waiting while anything it opened is open, due when nothing is.
async fn gate(tx: &mut Tx, slug: &str, c: &mut Change) -> Result<(), Failure> {
    let p = tx.project(slug).await?;
    let plans = open_of(tx, slug, &keys_of(&p, &[Kind::Audit])).await?;
    let gated =
        |r: &Item| r.wait_on.as_deref() == Some("condition") && r.wait_ref.as_deref() == Some(GATE);
    let before: BTreeSet<i64> = plans.iter().filter(|r| gated(r)).map(|r| r.rid).collect();
    let rids: Vec<i64> = plans.iter().map(|r| r.rid).collect();
    settle_audits(tx, &p, &rids).await?;
    for r in &plans {
        let now = tx.fresh(r.rid).await?;
        match (before.contains(&r.rid), gated(&now)) {
            (false, true) => c.gated.push(r.id.clone()),
            (true, false) => c.due.push(r.id.clone()),
            _ => {}
        }
    }
    Ok(())
}

/// The fields that lower an item at normal priority to low, none for any other tier.
fn lowered(r: &Item) -> Result<Vec<Field>, Failure> {
    if priority(&r.tags) == "normal" {
        Ok(prioritise(&r.tags, "low")?)
    } else {
        Ok(Vec::new())
    }
}

/// The inbox and later emptied into the queue, normal items there going to low.
async fn fold_scopes(tx: &mut Tx, slug: &str, c: &mut Change) -> Result<(), Failure> {
    let scoped = tx
        .items(
            "SELECT * FROM items WHERE project=? AND state='open' AND scope IS NOT NULL ORDER BY key, num",
            vec![slug.into()],
        )
        .await?;
    for r in scoped {
        let low = lowered(&r)?;
        let mut cols = vec![Field::Scope(None)];
        cols.extend(low.iter().cloned());
        tx.update(r.rid, &cols).await?;
        let place = r.scope.as_deref().unwrap_or_default();
        let note = if low.is_empty() {
            format!("out of the {place}: the simple model has no inbox or later, priority kept")
        } else {
            format!("out of the {place}: the simple model has no inbox or later, priority low")
        };
        tx.event(slug, Some(r.rid), "edited", Some(&note), None, None)
            .await?;
        c.unscoped.push(r.id.clone());
        if !low.is_empty() {
            c.lowered.push(r.id);
        }
    }
    Ok(())
}

/// The release fact turned into the `releases` fact: the release first, then the themes
/// named as versions, in version order. Release work keeps its priority; `next` orders by release.
async fn fold_release(tx: &mut Tx, slug: &str, c: &mut Change) -> Result<(), Failure> {
    let skills: Json = scalar(
        &tx.conn,
        "SELECT skills FROM projects WHERE slug=?",
        vec![slug.into()],
    )
    .await?
    .unwrap_or_default();
    let Some(release) = skills["release"].as_str().filter(|r| !r.is_empty()) else {
        return Ok(());
    };
    let themes = tx
        .items(
            "SELECT * FROM items WHERE project=? AND state='open' AND theme IS NOT NULL ORDER BY key, num",
            vec![slug.into()],
        )
        .await?;
    let current = release.split_whitespace().next().unwrap_or(release);
    let named: Vec<&str> = themes.iter().filter_map(|r| r.theme.as_deref()).collect();
    let releases = releases_of(current, &named).join(" ");
    let mut rest = skills.clone();
    if let Some(m) = rest.as_object_mut() {
        m.remove("release");
        m.insert("releases".into(), Json::String(releases.clone()));
    }
    tx.execute(
        "UPDATE projects SET skills=?, updated_at=? WHERE slug=?",
        vec![json(rest), tx.now.clone().into(), slug.into()],
    )
    .await?;
    tx.touch_project(slug);
    let said = format!(
        "release {release} removed, releases set to {releases}: the queue orders by release, then priority"
    );
    tx.event(slug, None, "edited", Some(&said), None, None)
        .await?;
    c.release = Some(release.to_string());
    c.releases = Some(releases);
    Ok(())
}

/// The releases in order: the current one, then each theme named as a version (`1.1`, `v2.0`),
/// in version order, each once.
#[must_use]
pub fn releases_of(current: &str, themes: &[&str]) -> Vec<String> {
    let mut later: Vec<(Vec<u64>, &str)> = themes
        .iter()
        .filter(|t| **t != current)
        .filter_map(|t| version(t).map(|v| (v, *t)))
        .collect();
    later.sort();
    later.dedup_by(|a, b| a.1 == b.1);
    std::iter::once(current.to_string())
        .chain(later.into_iter().map(|(_, t)| t.to_string()))
        .collect()
}

#[cfg(test)]
#[path = "tests/simplify.rs"]
mod tests;
