//! An item in the shape `docket show --json` prints, read inside the write's transaction.

use sea_orm::{ConnectionTrait, DbErr};

use docket_core::api::{Brief, Cite, ItemView};
use docket_core::item::Item;
use docket_core::rules::kind_of;
use docket_core::word::{Facts, Kind, priority, word};

use crate::store::{ProjectRow, column, id_of, sql};
use crate::verbs::graph::{open_member_count, package_progress};

/// The kind of a row's key, work when the project does not name it.
pub fn kind(project: &ProjectRow, row: &Item) -> Kind {
    kind_of(&project.rules, &row.key).unwrap_or(Kind::Work)
}

pub fn brief(row: &Item) -> Brief {
    Brief {
        id: row.id.clone(),
        title: row.title.clone(),
    }
}

pub async fn item_view<C: ConnectionTrait>(
    c: &C,
    project: &ProjectRow,
    row: &Item,
) -> Result<ItemView, DbErr> {
    let kind = kind(project, row);
    let (open_members, progress) = if kind == Kind::Package {
        (
            open_member_count(c, row.rid).await?,
            Some(package_progress(c, row.rid).await?),
        )
    } else {
        (0, None)
    };
    let facts = Facts {
        state: &row.state,
        kind,
        claimed: row.claim_branch.is_some(),
        waiting: row.wait_on.is_some(),
        turn: row.turn.as_deref(),
    };
    let superseded_by = match row.superseded_by {
        Some(rid) => id_of(c, rid).await?,
        None => None,
    };
    let (related, origin, cites) = links(c, row.rid).await?;
    let parent = match row.parent_rid {
        Some(rid) => id_of(c, rid).await?,
        None => None,
    };
    let children = column(
        c,
        "SELECT id FROM items WHERE parent_rid=? ORDER BY key, num",
        vec![row.rid.into()],
    )
    .await?;
    let release = match row.release_id {
        Some(id) => {
            crate::store::scalar(c, "SELECT name FROM releases WHERE id=?", vec![id.into()]).await?
        }
        None => None,
    };
    Ok(ItemView {
        project: row.project.clone(),
        key: row.key.clone(),
        num: row.num,
        id: row.id.clone(),
        title: row.title.clone(),
        state: row.state.clone(),
        turn: row.turn.clone(),
        turn_note: row.turn_note.clone(),
        asked_at: row.asked_at.clone(),
        claim_branch: row.claim_branch.clone(),
        claim_host: row.claim_host.clone(),
        claim_since: row.claim_since.clone(),
        claim_runner: row.claim_runner.clone(),
        claim_job: row.claim_job.clone(),
        claim_on: row.claim_on.clone(),
        wait_on: row.wait_on.clone(),
        wait_ref: row.wait_ref.clone(),
        wait_since: row.wait_since.clone(),
        decision: row.decision.clone(),
        decided_at: row.decided_at.clone(),
        resolution: row.resolution.clone(),
        scope: row.scope.clone(),
        complexity: row.complexity.clone(),
        theme: row.theme.clone(),
        release,
        rank: row.rank,
        tags: row.tags.clone(),
        body: row.body.clone(),
        conflict: row.conflict,
        opened_at: row.opened_at.clone(),
        updated_at: row.updated_at.clone(),
        group: row.group_name.clone(),
        word: word(&facts, open_members).to_string(),
        priority: priority(&row.tags).to_string(),
        superseded_by,
        related,
        parent,
        origin,
        children,
        cites,
        progress,
    })
}

/// The views of several rows, in their order.
pub async fn item_views<C: ConnectionTrait>(
    c: &C,
    project: &ProjectRow,
    rows: &[Item],
) -> Result<Vec<ItemView>, DbErr> {
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(item_view(c, project, r).await?);
    }
    Ok(out)
}

type Links = (Vec<String>, Vec<String>, Vec<Cite>);

async fn links<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Links, DbErr> {
    let (mut related, mut origin, mut cites) = (Vec::new(), Vec::new(), Vec::new());
    let rows = c
        .query_all_raw(sql(
            "SELECT kind, to_rid, to_path, to_line FROM links WHERE rid=? \
             ORDER BY kind, to_rid NULLS FIRST, to_path NULLS FIRST, to_line NULLS FIRST, id",
            vec![rid.into()],
        ))
        .await?;
    for l in rows {
        let kind: String = l.try_get_by_index(0)?;
        let to_rid: Option<i64> = l.try_get_by_index(1)?;
        match (kind.as_str(), to_rid) {
            ("related", Some(to)) => related.extend(id_of(c, to).await?),
            ("origin", Some(to)) => origin.extend(id_of(c, to).await?),
            _ => cites.push(Cite {
                path: l.try_get_by_index::<Option<String>>(2)?.unwrap_or_default(),
                line: l.try_get_by_index(3)?,
                kind,
            }),
        }
    }
    Ok((related, origin, cites))
}
