//! The `opened` links of an earlier store split into one parent plan per item and `origin` links,
//! as the move onto the core plans them (`docket_core::migrate::plan`): an opener that is a plan, or
//! the nearest plan up the chain of one that is not, becomes the parent, the earliest open when there
//! are several; the other plans become `related`, and an opener that is not a plan an `origin`. Every
//! item whose file changes is left for the dump.

use std::collections::BTreeMap;

use docket_core::migrate::{Change, OldItem, OldProject, Rows, Rules, plan};
use sea_orm::{ConnectionTrait, DbErr};

use crate::statement;

const LINK_KINDS: &str = "ALTER TABLE links ADD CONSTRAINT links_kind_check \
    CHECK (kind IN ('related', 'origin', 'cites_file', 'cites_test'));";

/// Every project with an opened link, and its keys.
const PROJECTS: &str = "SELECT slug, keys, themes, skills FROM projects p WHERE EXISTS \
    (SELECT 1 FROM links l JOIN items i ON i.rid=l.rid WHERE i.project=p.slug AND l.kind='opened') \
    ORDER BY slug";

/// A project's items, with the fields the plan reads.
const ITEMS: &str = "SELECT rid, id, title, state, turn, wait_on, wait_ref, theme, group_name, tags, \
    opened_at, updated_at FROM items WHERE project=? ORDER BY rid";

/// A project's item links, as the id each runs to.
const LINKS: &str = "SELECT l.rid, l.kind, t.id FROM links l JOIN items i ON i.rid=l.rid \
    JOIN items t ON t.rid=l.to_rid WHERE i.project=? AND l.kind IN ('opened', 'related') ORDER BY l.id";

async fn rows_of<C: ConnectionTrait>(
    c: &C,
    slug: &str,
) -> Result<(Vec<OldItem>, BTreeMap<String, i64>), DbErr> {
    let mut items: BTreeMap<i64, OldItem> = BTreeMap::new();
    let mut rids = BTreeMap::new();
    for r in c.query_all_raw(statement(ITEMS, vec![slug.into()])).await? {
        let rid: i64 = r.try_get_by_index(0)?;
        let tags: serde_json::Value = r.try_get_by_index(9)?;
        let item = OldItem {
            project: slug.to_string(),
            id: r.try_get_by_index(1)?,
            title: r.try_get_by_index(2)?,
            state: r.try_get_by_index(3)?,
            turn: r.try_get_by_index(4)?,
            wait_on: r.try_get_by_index(5)?,
            wait_ref: r.try_get_by_index(6)?,
            theme: r.try_get_by_index(7)?,
            group: r.try_get_by_index(8)?,
            tags: serde_json::from_value(tags).unwrap_or_default(),
            opened_at: r.try_get_by_index(10)?,
            updated_at: r.try_get_by_index(11)?,
            ..OldItem::default()
        };
        rids.insert(item.id.clone(), rid);
        items.insert(rid, item);
    }
    for r in c.query_all_raw(statement(LINKS, vec![slug.into()])).await? {
        let rid: i64 = r.try_get_by_index(0)?;
        let kind: String = r.try_get_by_index(1)?;
        let to: String = r.try_get_by_index(2)?;
        if let Some(i) = items.get_mut(&rid) {
            match kind.as_str() {
                "opened" => i.opened.push(to),
                _ => i.related.push(to),
            }
        }
    }
    Ok((items.into_values().collect(), rids))
}

async fn link<C: ConnectionTrait>(c: &C, rid: i64, kind: &str, to: i64) -> Result<(), DbErr> {
    c.execute_raw(statement(
        "INSERT INTO links (rid, kind, to_rid) SELECT ?, ?, ? WHERE NOT EXISTS \
         (SELECT 1 FROM links WHERE rid=? AND kind=? AND to_rid=?)",
        vec![
            rid.into(),
            kind.into(),
            to.into(),
            rid.into(),
            kind.into(),
            to.into(),
        ],
    ))
    .await?;
    Ok(())
}

async fn pending<C: ConnectionTrait>(c: &C, slug: &str, rid: i64) -> Result<(), DbErr> {
    c.execute_raw(statement(
        "INSERT INTO pending_dump (rid, project) VALUES (?, ?) ON CONFLICT DO NOTHING",
        vec![rid.into(), slug.into()],
    ))
    .await?;
    Ok(())
}

/// Lets `links` take the `opened` rows of an earlier store until `split` turns them into parents.
///
/// # Errors
/// The database refuses.
pub async fn admit_opened<C: ConnectionTrait>(c: &C) -> Result<(), DbErr> {
    c.execute_unprepared("ALTER TABLE links DROP CONSTRAINT links_kind_check")
        .await?;
    Ok(())
}

/// Each project's opened links, split by the plan into parents, origins and related ties, after which
/// `links` takes only the kinds it keeps.
///
/// # Errors
/// The database refuses.
pub async fn split<C: ConnectionTrait>(c: &C) -> Result<(), DbErr> {
    for p in c.query_all_raw(statement(PROJECTS, vec![])).await? {
        let project = OldProject {
            slug: p.try_get_by_index(0)?,
            keys: p.try_get_by_index(1)?,
            themes: p.try_get_by_index(2)?,
            skills: p.try_get_by_index(3)?,
            ..OldProject::default()
        };
        let slug = project.slug.clone();
        let (items, rids) = rows_of(c, &slug).await?;
        let rows = Rows {
            project: &project,
            items: items.iter().collect(),
            events: Vec::new(),
        };
        let rid = |id: &str| rids.get(id).copied();
        for i in items.iter().filter(|i| !i.opened.is_empty()) {
            if let Some(r) = rid(&i.id) {
                pending(c, &slug, r).await?;
            }
        }
        for change in plan(&rows, Rules::default()).changes {
            match change {
                Change::Parent { id, parent } => {
                    if let (Some(r), Some(to)) = (rid(&id), rid(&parent)) {
                        c.execute_raw(statement(
                            "UPDATE items SET parent_rid=? WHERE rid=?",
                            vec![to.into(), r.into()],
                        ))
                        .await?;
                    }
                }
                Change::Origin { id, from } => {
                    if let (Some(r), Some(to)) = (rid(&id), rid(&from)) {
                        link(c, r, "origin", to).await?;
                    }
                }
                Change::Related { id, to, .. } => {
                    if let (Some(r), Some(to)) = (rid(&id), rid(&to)) {
                        link(c, r, "related", to).await?;
                        link(c, to, "related", r).await?;
                        pending(c, &slug, to).await?;
                    }
                }
                _ => {}
            }
        }
    }
    c.execute_unprepared("DELETE FROM links WHERE kind='opened'")
        .await?;
    c.execute_unprepared(LINK_KINDS).await?;
    Ok(())
}
