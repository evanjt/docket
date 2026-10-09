//! `project`, `reindex` and `rename`: a checkout's project matched or created, a project's index
//! rebuilt, and a project moved to a new slug.

use std::collections::{BTreeMap, BTreeSet};

use axum::Json;
use axum::extract::{Extension, State};
use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult};
use serde_json::Value;

use docket_core::api::{
    Common, ProjectRenameRequest, ProjectRenamed, ProjectRequest, ProjectResolved, Reindexed,
    RenamedLabel,
};
use docket_core::assignment::Held;
use docket_core::jsontext;
use docket_core::project::{matches, slug_valid};

use crate::auth::Caller;
use crate::store::{Tx, column, held_in, json, scalar, sql};
use crate::verbs::{Call, Failure, require_owner};

/// The project a checkout belongs to: matched by its remote's slug, a shared remote or its directory
/// name, else created. Its remotes are recorded on the project either way.
///
/// # Errors
/// 409 when the checkout could be several projects, or none is found and creating is not asked.
pub async fn project(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<ProjectRequest>,
) -> Result<Json<ProjectResolved>, Failure> {
    let mut tx = Tx::begin(&db, &caller.host).await?;
    let slugs: Vec<String> =
        column(&tx.conn, "SELECT slug FROM projects ORDER BY slug", vec![]).await?;
    let mut known = Vec::with_capacity(slugs.len());
    for slug in slugs {
        known.push((slug.clone(), remotes_of(&tx, &slug).await?));
    }
    let found = matches(
        &known,
        req.candidate.as_deref(),
        &req.remotes,
        &req.basename,
    );
    if found.len() > 1 {
        return Ok(Json(ProjectResolved {
            slug: None,
            how: "ambiguous".into(),
            matches: found,
        }));
    }
    let (slug, how) = match found.into_iter().next() {
        Some(slug) => (slug, "matched"),
        None if !req.create => {
            return Ok(Json(ProjectResolved {
                slug: None,
                how: "none".into(),
                matches: Vec::new(),
            }));
        }
        None => {
            let slug = req
                .candidate
                .clone()
                .unwrap_or_else(|| req.basename.clone());
            create(&mut tx, &slug, &req.remotes).await?;
            (slug, "created")
        }
    };
    add_remotes(&mut tx, &slug, &req.remotes).await?;
    tx.commit().await?;
    Ok(Json(ProjectResolved {
        slug: Some(slug),
        how: how.into(),
        matches: Vec::new(),
    }))
}

async fn remotes_of(tx: &Tx, slug: &str) -> Result<Vec<String>, Failure> {
    let remotes: Option<Value> = scalar(
        &tx.conn,
        "SELECT remotes FROM projects WHERE slug=?",
        vec![slug.into()],
    )
    .await?;
    Ok(remotes
        .and_then(|r| serde_json::from_value(r).ok())
        .unwrap_or_default())
}

fn sorted_unique(urls: impl IntoIterator<Item = String>) -> Value {
    let mut urls: Vec<String> = urls.into_iter().collect();
    urls.sort();
    urls.dedup();
    Value::from(urls)
}

async fn create(tx: &mut Tx, slug: &str, remotes: &[String]) -> Result<(), Failure> {
    tx.execute(
        "INSERT INTO projects (slug, remotes, created_at, updated_at) VALUES (?, ?, ?, ?)",
        vec![
            slug.into(),
            json(sorted_unique(remotes.iter().cloned())),
            tx.now.clone().into(),
            tx.now.clone().into(),
        ],
    )
    .await?;
    tx.touch_project(slug);
    Ok(())
}

/// The checkout's remotes added to the project's, when they bring one it lacks.
async fn add_remotes(tx: &mut Tx, slug: &str, urls: &[String]) -> Result<(), Failure> {
    if urls.is_empty() {
        return Ok(());
    }
    let have = remotes_of(tx, slug).await?;
    let all = sorted_unique(have.iter().chain(urls).cloned());
    if all == Value::from(have) {
        return Ok(());
    }
    tx.execute(
        "UPDATE projects SET remotes=?, updated_at=? WHERE slug=?",
        vec![json(all), tx.now.clone().into(), slug.into()],
    )
    .await?;
    tx.touch_project(slug);
    Ok(())
}

/// The search rows and citation links of every item of a project, rebuilt from their bodies. No item
/// changes, so nothing is marked for the dump.
///
/// # Errors
/// 404 for an unknown project.
pub async fn reindex(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<Common>,
) -> Result<Json<Reindexed>, Failure> {
    let tx = Tx::begin(&db, &caller.host).await?;
    tx.project(&req.project).await?;
    let rids: Vec<i64> = column(
        &tx.conn,
        "SELECT rid FROM items WHERE project=? ORDER BY rid",
        vec![req.project.clone().into()],
    )
    .await?;
    for rid in &rids {
        tx.index(*rid).await?;
    }
    tx.commit().await?;
    Ok(Json(Reindexed {
        project: req.project,
        items: rids.len(),
    }))
}

/// The tables whose rows reach a project through a foreign key, which the rename cascades.
const CASCADED: [&str; 8] = [
    "items",
    "events",
    "leads",
    "releases",
    "areas",
    "labels",
    "publications",
    "roots",
];

/// The tables that carry a slug in a plain column, rewritten by hand.
const PLAIN: [&str; 2] = ["pending_dump", "chores"];

#[derive(FromQueryResult)]
struct LabelRow {
    id: i64,
    project: String,
    name: String,
}

/// The project takes a new slug: every row of every table follows it, the labels of other projects
/// that name it as `repo:@OLD[/PATH]` are renamed with it, and a `renamed` event records the old
/// slug. Refused while a lead holds the project, while a job runs on one of its items, and, without
/// `force`, while a claim holds one.
///
/// # Errors
/// 403 on an agent's key; 404 when the project is unknown; 400 for a slug outside the shape or
/// equal to the old one; 409 for the refusals above and for a new slug already taken.
pub async fn rename(
    State(db): State<DatabaseConnection>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<ProjectRenameRequest>,
) -> Result<Json<ProjectRenamed>, Failure> {
    require_owner(&caller, "projects rename")?;
    let old = req.common.project.clone();
    let new = req.new.trim().to_string();
    slug_valid(&new)?;
    if new == old {
        return Err(Failure::Invalid(format!("{old} is already the slug")));
    }
    let mut call = Call::begin(&db, &caller, &req.common).await?;
    let taken: Option<i64> = scalar(
        &call.tx.conn,
        "SELECT COUNT(*) FROM projects WHERE slug=?",
        vec![new.clone().into()],
    )
    .await?;
    if taken.unwrap_or(0) > 0 {
        return Err(Failure::Refused(format!("a project {new} already exists")));
    }
    refuse_while_held(&call, &old).await?;
    let labels = labels_naming(&call.tx.conn, &old, &new).await?;
    let mut moved = BTreeMap::new();
    for table in CASCADED.iter().chain(PLAIN.iter()) {
        let n: Option<i64> = scalar(
            &call.tx.conn,
            &format!("SELECT COUNT(*) FROM {table} WHERE project=?"),
            vec![old.clone().into()],
        )
        .await?;
        moved.insert((*table).to_string(), n.unwrap_or(0).unsigned_abs());
    }
    let out = ProjectRenamed {
        dry_run: req.dry_run,
        old: old.clone(),
        new: new.clone(),
        moved,
        labels: labels.iter().map(|(_, l)| l.clone()).collect(),
    };
    if req.dry_run {
        return Ok(Json(out));
    }
    apply_rename(&mut call, &old, &new, &labels).await?;
    call.tx.commit().await?;
    Ok(Json(out))
}

/// Refused while a lead holds the project, while a job runs on one of its items, and, without
/// `force`, while a claim holds one.
async fn refuse_while_held(call: &Call, old: &str) -> Result<(), Failure> {
    let led: Option<String> = scalar(
        &call.tx.conn,
        "SELECT host FROM leads WHERE project=?",
        vec![old.into()],
    )
    .await?;
    if let Some(host) = led {
        return Err(Failure::Refused(format!(
            "{old} is led from {host}: give the lead back first, then rename"
        )));
    }
    let mut jobs = Vec::new();
    let mut claims = Vec::new();
    for (rid, held) in held_in(&call.tx.conn, old).await? {
        if let Held::Claim(c) = held {
            if c.job.is_some() {
                jobs.push(rid);
            } else {
                claims.push(rid);
            }
        }
    }
    if !jobs.is_empty() {
        return Err(Failure::Refused(format!(
            "{} of {old}'s items are held by running jobs ({}): wait for them to end",
            jobs.len(),
            ids_of(&call.tx.conn, &jobs).await?
        )));
    }
    if !claims.is_empty() && !call.ctx.force {
        return Err(Failure::Refused(format!(
            "{} of {old}'s items are claimed ({}): pass --force to rename under the claims",
            claims.len(),
            ids_of(&call.tx.conn, &claims).await?
        )));
    }
    Ok(())
}

/// Every label, in any project, that names the project as `repo:@OLD` or `repo:@OLD/PATH`, with the
/// name it takes; refused when a project already has a label of that name.
async fn labels_naming<C: ConnectionTrait>(
    c: &C,
    old: &str,
    new: &str,
) -> Result<Vec<(i64, RenamedLabel)>, Failure> {
    let prefix = format!("repo:@{old}");
    let naming = LabelRow::find_by_statement(sql(
        "SELECT id, project, name FROM labels WHERE name=? OR name LIKE ? ORDER BY project, name",
        vec![prefix.clone().into(), format!("{prefix}/%").into()],
    ))
    .all(c)
    .await?;
    let mut labels = Vec::with_capacity(naming.len());
    for row in naming {
        let renamed = format!("repo:@{new}{}", &row.name[prefix.len()..]);
        let clash: Option<i64> = scalar(
            c,
            "SELECT COUNT(*) FROM labels WHERE project=? AND lower(name)=lower(?) AND id<>?",
            vec![
                row.project.clone().into(),
                renamed.clone().into(),
                row.id.into(),
            ],
        )
        .await?;
        if clash.unwrap_or(0) > 0 {
            return Err(Failure::Refused(format!(
                "{} already has a label {renamed}, which its {} would become",
                row.project, row.name
            )));
        }
        labels.push((
            row.id,
            RenamedLabel {
                project: row.project,
                old: row.name,
                new: renamed,
            },
        ));
    }
    Ok(labels)
}

/// The writes of a rename: the slug, which the foreign keys cascade; the plain columns; the dump
/// list; the labels; the event; and a touch on every item so the dump and open clients follow.
async fn apply_rename(
    call: &mut Call,
    old: &str,
    new: &str,
    labels: &[(i64, RenamedLabel)],
) -> Result<(), Failure> {
    let now = call.tx.now.clone();
    call.tx
        .execute(
            "UPDATE projects SET slug=?, updated_at=? WHERE slug=?",
            vec![new.into(), now.into(), old.into()],
        )
        .await?;
    for table in PLAIN {
        call.tx
            .execute(
                &format!("UPDATE {table} SET project=? WHERE project=?"),
                vec![new.into(), old.into()],
            )
            .await?;
    }
    let pending: Option<String> = scalar(
        &call.tx.conn,
        "SELECT v FROM meta WHERE k='pending_dump_projects'",
        vec![],
    )
    .await?;
    if let Some(v) = pending {
        let mut slugs: BTreeSet<String> = serde_json::from_str(&v).unwrap_or_default();
        if slugs.remove(old) {
            slugs.insert(new.to_string());
            call.tx
                .execute(
                    "UPDATE meta SET v=? WHERE k='pending_dump_projects'",
                    vec![jsontext::dumps(&serde_json::json!(slugs), false).into()],
                )
                .await?;
        }
    }
    for (id, label) in labels {
        call.tx
            .execute(
                "UPDATE labels SET name=? WHERE id=?",
                vec![label.new.clone().into(), (*id).into()],
            )
            .await?;
        for rid in column::<_, i64>(
            &call.tx.conn,
            "SELECT rid FROM item_labels WHERE label_id=?",
            vec![(*id).into()],
        )
        .await?
        {
            call.tx.touch(rid);
        }
    }
    call.tx
        .event(
            new,
            None,
            "renamed",
            Some(&format!("{old} -> {new}")),
            None,
            Some(&serde_json::json!({ "old": old, "new": new })),
        )
        .await?;
    for rid in column::<_, i64>(
        &call.tx.conn,
        "SELECT rid FROM items WHERE project=?",
        vec![new.into()],
    )
    .await?
    {
        call.tx.touch(rid);
    }
    call.tx.touch_project(new);
    Ok(())
}

/// The ids of the rids, in rid order, joined for a message.
async fn ids_of<C: ConnectionTrait>(c: &C, rids: &[i64]) -> Result<String, Failure> {
    let marks = vec!["?"; rids.len()].join(", ");
    let ids: Vec<String> = column(
        c,
        &format!("SELECT id FROM items WHERE rid IN ({marks}) ORDER BY rid"),
        rids.iter().map(|r| (*r).into()).collect(),
    )
    .await?;
    Ok(ids.join(", "))
}
