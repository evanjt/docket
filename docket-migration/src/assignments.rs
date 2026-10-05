//! The statements that write assignment rows, shared by the server's event writer, the migration
//! that creates the table, the import and the restore.

use std::collections::BTreeMap;

use sea_orm::{ConnectionTrait, DbErr};

use docket_core::assignment::{Assignment, End, Event, Kind, Now, Outcome, Usage, rebuild, settle};
use docket_core::dump::ItemDump;

use crate::statement;

/// The open row of an item: its id and kind.
///
/// # Errors
/// The database refuses.
pub async fn open<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Option<(i64, Kind)>, DbErr> {
    let row = c
        .query_one_raw(statement(
            "SELECT id, kind FROM assignments WHERE rid=? AND ended_at IS NULL",
            vec![rid.into()],
        ))
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let id: i64 = row.try_get_by_index(0)?;
    let kind: String = row.try_get_by_index(1)?;
    Ok(Kind::parse(&kind).map(|k| (id, k)))
}

/// The open row ended at `at` by `actor`.
///
/// # Errors
/// The database refuses.
pub async fn end<C: ConnectionTrait>(
    c: &C,
    id: i64,
    at: &str,
    end: &End,
    actor: Option<&str>,
) -> Result<(), DbErr> {
    c.execute_raw(statement(
        "UPDATE assignments SET ended_at=?, outcome=?, note=COALESCE(?, note), \
         actor=COALESCE(?, actor) WHERE id=?",
        vec![
            at.into(),
            end.outcome.map(Outcome::as_str).into(),
            end.note.clone().into(),
            actor.map(str::to_string).into(),
            id.into(),
        ],
    ))
    .await?;
    Ok(())
}

/// The open row given the usage of its job.
///
/// # Errors
/// The database refuses.
pub async fn report<C: ConnectionTrait>(c: &C, id: i64, u: &Usage) -> Result<(), DbErr> {
    c.execute_raw(statement(
        "UPDATE assignments SET tokens_in=COALESCE(?, tokens_in), tokens_out=COALESCE(?, tokens_out), \
         cost_reported=COALESCE(?, cost_reported), job_started_at=COALESCE(?, job_started_at), \
         job_ended_at=COALESCE(?, job_ended_at), job_exit=COALESCE(?, job_exit) WHERE id=?",
        vec![
            u.tokens_in.into(),
            u.tokens_out.into(),
            u.cost_reported.into(),
            u.job_started_at.clone().into(),
            u.job_ended_at.clone().into(),
            u.job_exit.into(),
            id.into(),
        ],
    ))
    .await?;
    Ok(())
}

/// One row written for the item.
///
/// # Errors
/// The database refuses, as it does a second open row for one item.
pub async fn insert<C: ConnectionTrait>(c: &C, rid: i64, a: &Assignment) -> Result<(), DbErr> {
    // The need and job columns are added after the table is first rebuilt, so only a row that has
    // one names it.
    let mut columns = String::new();
    let mut marks = String::new();
    let mut values: Vec<sea_orm::Value> = vec![
        rid.into(),
        a.assignee.clone().into(),
        a.kind.as_str().into(),
        a.started_at.clone().into(),
        a.ended_at.clone().into(),
        a.outcome.map(Outcome::as_str).into(),
        a.note.clone().into(),
        a.actor.clone().into(),
        a.branch.clone().into(),
        a.host.clone().into(),
        a.machine.clone().into(),
        a.runner.clone().into(),
        a.model.clone().into(),
        a.effort.clone().into(),
        a.role.clone().into(),
        a.job.clone().into(),
        a.tokens_in.into(),
        a.tokens_out.into(),
        a.cost_reported.into(),
    ];
    let extra: [(&str, Option<sea_orm::Value>); 4] = [
        ("need", a.need.clone().map(Into::into)),
        ("job_started_at", a.job_started_at.clone().map(Into::into)),
        ("job_ended_at", a.job_ended_at.clone().map(Into::into)),
        ("job_exit", a.job_exit.map(Into::into)),
    ];
    for (name, value) in extra {
        if let Some(v) = value {
            columns.push_str(", ");
            columns.push_str(name);
            marks.push_str(", ?");
            values.push(v);
        }
    }
    c.execute_raw(statement(
        &format!(
            "INSERT INTO assignments (rid, assignee, kind, started_at, ended_at, outcome, note, actor, \
             branch, host, machine, runner, model, effort, role, job, tokens_in, tokens_out, \
             cost_reported{columns}) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?{marks})"
        ),
        values,
    ))
    .await?;
    Ok(())
}

/// The rows of every item that has none, rebuilt from its events and made to agree with its claim
/// and turn now. Returns how many rows were written.
///
/// # Errors
/// The database refuses.
pub async fn rebuild_all<C: ConnectionTrait>(c: &C) -> Result<usize, DbErr> {
    let mut events: BTreeMap<i64, Vec<Event>> = BTreeMap::new();
    for r in c
        .query_all_raw(statement(
            "SELECT rid, kind, at, host, branch, note, data::text FROM events \
             WHERE rid IS NOT NULL ORDER BY rid, seq",
            vec![],
        ))
        .await?
    {
        let data: Option<String> = r.try_get_by_index(6)?;
        events
            .entry(r.try_get_by_index(0)?)
            .or_default()
            .push(Event {
                kind: r.try_get_by_index(1)?,
                at: r.try_get_by_index(2)?,
                host: r.try_get_by_index(3)?,
                branch: r.try_get_by_index(4)?,
                note: r.try_get_by_index(5)?,
                data: data.and_then(|d| serde_json::from_str(&d).ok()),
            });
    }
    let items = c
        .query_all_raw(statement(
            "SELECT rid, state, turn, turn_note, asked_at, claim_branch, claim_host, claim_since, \
             claim_runner, claim_job, claim_on, opened_at, updated_at FROM items i \
             WHERE NOT EXISTS (SELECT 1 FROM assignments a WHERE a.rid=i.rid) ORDER BY rid",
            vec![],
        ))
        .await?;
    let mut written = 0;
    for r in items {
        let rid: i64 = r.try_get_by_index(0)?;
        let item = ItemDump {
            state: r.try_get_by_index(1)?,
            turn: r.try_get_by_index(2)?,
            turn_note: r.try_get_by_index(3)?,
            asked_at: r.try_get_by_index(4)?,
            claim_branch: r.try_get_by_index(5)?,
            claim_host: r.try_get_by_index(6)?,
            claim_since: r.try_get_by_index(7)?,
            claim_runner: r.try_get_by_index(8)?,
            claim_job: r.try_get_by_index(9)?,
            claim_on: r.try_get_by_index(10)?,
            opened_at: r.try_get_by_index(11)?,
            updated_at: r.try_get_by_index(12)?,
            ..ItemDump::default()
        };
        let mut rows = rebuild(events.get(&rid).map_or(&[][..], Vec::as_slice));
        settle(&mut rows, &Now::of(&item));
        for a in &rows {
            insert(c, rid, a).await?;
        }
        written += rows.len();
    }
    Ok(written)
}
