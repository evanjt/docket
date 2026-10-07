//! An item in the shape `docket show --json` prints, read inside the write's transaction.

use sea_orm::{ConnectionTrait, DbErr};

use docket_core::api::{Brief, Cite, ItemView};
use docket_core::assignment::HeldFields;
use docket_core::item::Item;
use docket_core::word::Kind;

use crate::reads::public::{Members, word_of};
use crate::store::{column, id_of, sql};
use crate::verbs::graph::{member_count, package_progress, wait_of};

pub fn brief(row: &Item) -> Brief {
    Brief {
        id: row.id.clone(),
        title: row.title.clone(),
    }
}

pub async fn item_view<C: ConnectionTrait>(c: &C, row: &Item) -> Result<ItemView, DbErr> {
    let kind = row.item_type.kind();
    let members = if kind.is_plan() {
        member_count(c, row.rid).await?
    } else {
        Members::default()
    };
    let progress = if kind == Kind::Package {
        Some(package_progress(c, row.rid).await?)
    } else {
        None
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
    let area = crate::verbs::areas::name_of(c, row.area_id).await?;
    let wait = wait_of(c, row.rid).await?;
    let labels: Vec<String> = crate::verbs::labels::carried(c, row.rid)
        .await?
        .into_iter()
        .map(|l| l.name)
        .collect();
    let held = HeldFields::of(&row.state, row.held.as_ref());
    Ok(ItemView {
        project: row.project.clone(),
        key: row.key.clone(),
        num: row.num,
        id: row.id.clone(),
        title: row.title.clone(),
        state: row.state.clone(),
        turn: held.turn,
        turn_note: held.turn_note,
        asked_at: held.asked_at,
        claim_branch: held.claim_branch,
        claim_host: held.claim_host,
        claim_since: held.claim_since,
        claim_runner: held.claim_runner,
        claim_job: held.claim_job,
        claim_on: held.claim_on,
        wait_on: wait.as_ref().map(|w| w.on.to_string()),
        wait_ref: wait.as_ref().map(|w| w.id.clone()),
        wait_since: wait.as_ref().map(|w| w.since.clone()),
        decision: row.decision.clone(),
        decided_at: row.decided_at.clone(),
        resolution: row.resolution.clone(),
        complexity: row.complexity.clone(),
        release,
        area,
        group: docket_core::label::group_of(&labels).map(str::to_string),
        repo: docket_core::label::repo_of(&labels).map(str::to_string),
        labels,
        body: row.body.clone(),
        conflict: 0,
        opened_at: row.opened_at.clone(),
        updated_at: row.updated_at.clone(),
        word: word_of(&row.state, row.held.as_ref(), wait.is_some(), kind, members).to_string(),
        priority: row.priority.clone(),
        item_type: row.item_type.as_str().to_string(),
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
pub async fn item_views<C: ConnectionTrait>(c: &C, rows: &[Item]) -> Result<Vec<ItemView>, DbErr> {
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(item_view(c, r).await?);
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
