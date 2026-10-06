//! One write's transaction: the rows, their events and links, the index and the dump mark.

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, FromQueryResult, Statement,
    TransactionTrait, Value,
};
use serde_json::Value as Json;
use tokio::sync::{Mutex, OwnedMutexGuard};

use docket_core::assignment::{self, Ask, Claim, Held};
use docket_core::clock;
use docket_core::item::{Field, Item, Project, Refused};
use docket_core::jsontext;
use docket_core::text::{citations, split_id};
use docket_core::word::ItemType;
use docket_migration::assignments;

use crate::changes::CHANNEL;
use crate::entities::{item, project};
use crate::verbs::Failure;

/// A project row, with the columns a verb reads.
#[derive(Clone, Debug)]
pub struct ProjectRow {
    pub rules: Project,
    pub worktree_hint: Option<String>,
}

/// The item as stored, its open assignment not yet read.
pub fn to_item(m: item::Model) -> Item {
    Item {
        rid: m.rid,
        project: m.project,
        key: m.key,
        num: m.num,
        id: m.id,
        title: m.title,
        state: m.state,
        held: None,
        decision: m.decision,
        decided_at: m.decided_at,
        resolution: m.resolution,
        superseded_by: m.superseded_by,
        parent_rid: m.parent_rid,
        complexity: m.complexity,
        release_id: m.release_id,
        area_id: m.area_id,
        item_type: ItemType::parse(&m.item_type).unwrap_or_default(),
        priority: m.priority,
        body: m.body,
        opened_at: m.opened_at,
        updated_at: m.updated_at,
    }
}

/// The project as a verb reads it.
pub fn project_row(m: project::Model) -> ProjectRow {
    ProjectRow {
        rules: Project { slug: m.slug },
        worktree_hint: m.worktree_hint,
    }
}

/// The advisory lock every write transaction holds, so writes commit one at a time and their events
/// become visible in `seq` order.
pub(crate) const WRITES: i64 = 0x0064_6f63_6b65_7401;

/// Events written by this process. A uid carries the count after its moment, so a log ordered by
/// `(at, uid)` keeps the order events were written in within one second, and the rows rebuilt from
/// it, such as assignments, come out as they were written.
static WRITTEN: AtomicU64 = AtomicU64::new(0);

/// A statement for text written with `?` marks.
pub fn sql(text: &str, values: Vec<Value>) -> Statement {
    docket_migration::statement(text, values)
}

/// A JSON value to bind to a `jsonb` column.
pub fn json(value: Json) -> Value {
    Value::Json(Some(Box::new(value)))
}

/// The open assignment rows: who holds each item now. Every read of a claim or of the owner's turn
/// goes through it.
const HELD: &str = "SELECT rid, kind, branch, host, started_at, runner, job, machine, note \
                    FROM assignments WHERE ended_at IS NULL";

/// A condition on `items.rid`: an open claim holds the item.
pub const CLAIMED: &str =
    "rid IN (SELECT rid FROM assignments WHERE ended_at IS NULL AND kind='claim')";

/// A condition on `items.rid`: an open ask holds the item, so it is the owner's turn.
pub const ASKED: &str =
    "rid IN (SELECT rid FROM assignments WHERE ended_at IS NULL AND kind='ask')";

/// A condition on `items.rid`: an open claim made on the host bound to it holds the item.
pub const CLAIMED_ON: &str =
    "rid IN (SELECT rid FROM assignments WHERE ended_at IS NULL AND kind='claim' AND host=?)";

/// When the item's open assignment began, to order a query of `items` by.
pub const HELD_SINCE: &str =
    "(SELECT a.started_at FROM assignments a WHERE a.rid=items.rid AND a.ended_at IS NULL)";

/// The open items of a project that a claim holds, on one host when one is named, oldest claim first.
pub async fn claimed<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    host: Option<&str>,
) -> Result<Vec<Item>, DbErr> {
    let mut values: Vec<Value> = vec![slug.into()];
    let on = match host {
        Some(host) => {
            values.push(host.into());
            format!(" AND {CLAIMED_ON}")
        }
        None => String::new(),
    };
    let text = format!(
        "SELECT * FROM items WHERE project=? AND state='open' AND {CLAIMED}{on} \
         ORDER BY {HELD_SINCE}, rid"
    );
    items(c, &text, values).await
}

/// How many rids one read of the open rows names.
const HELD_PAGE: usize = 1000;

#[derive(FromQueryResult)]
struct HeldRow {
    rid: i64,
    kind: String,
    branch: Option<String>,
    host: String,
    started_at: String,
    runner: Option<String>,
    job: Option<String>,
    machine: Option<String>,
    note: Option<String>,
}

impl HeldRow {
    fn held(self) -> Option<(i64, Held)> {
        let held = match assignment::Kind::parse(&self.kind)? {
            assignment::Kind::Claim => Held::Claim(Claim {
                branch: self.branch.unwrap_or_default(),
                host: self.host,
                since: self.started_at,
                runner: self.runner,
                job: self.job,
                on: self.machine,
            }),
            assignment::Kind::Ask => Held::Ask(Ask {
                since: self.started_at,
                note: self.note,
            }),
        };
        Some((self.rid, held))
    }
}

async fn held_where<C: ConnectionTrait>(
    c: &C,
    tail: &str,
    values: Vec<Value>,
) -> Result<HashMap<i64, Held>, DbErr> {
    Ok(
        HeldRow::find_by_statement(sql(&format!("{HELD} {tail}"), values))
            .all(c)
            .await?
            .into_iter()
            .filter_map(HeldRow::held)
            .collect(),
    )
}

/// The open assignment of each of the rids that has one.
pub async fn held<C: ConnectionTrait>(c: &C, rids: &[i64]) -> Result<HashMap<i64, Held>, DbErr> {
    let mut out = HashMap::new();
    for page in rids.chunks(HELD_PAGE) {
        let marks = vec!["?"; page.len()].join(", ");
        let values = page.iter().map(|r| (*r).into()).collect();
        out.extend(held_where(c, &format!("AND rid IN ({marks})"), values).await?);
    }
    Ok(out)
}

/// The open assignment of each item of a project that has one.
pub async fn held_in<C: ConnectionTrait>(c: &C, slug: &str) -> Result<HashMap<i64, Held>, DbErr> {
    held_where(
        c,
        "AND rid IN (SELECT rid FROM items WHERE project=?)",
        vec![slug.into()],
    )
    .await
}

/// The open assignment of one item.
pub async fn held_of<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Option<Held>, DbErr> {
    Ok(held(c, &[rid]).await?.remove(&rid))
}

/// Item rows for a query, each as the rules read it, with its open assignment.
pub async fn items<C: ConnectionTrait>(
    c: &C,
    text: &str,
    values: Vec<Value>,
) -> Result<Vec<Item>, DbErr> {
    let rows = c.query_all_raw(sql(text, values)).await?;
    let mut out = rows
        .iter()
        .map(|r| item::Model::from_query_result(r, "").map(to_item))
        .collect::<Result<Vec<Item>, DbErr>>()?;
    let rids: Vec<i64> = out.iter().map(|i| i.rid).collect();
    let mut held = held(c, &rids).await?;
    for i in &mut out {
        i.held = held.remove(&i.rid);
    }
    Ok(out)
}

/// One item row for a query, or none.
pub async fn item_row<C: ConnectionTrait>(
    c: &C,
    text: &str,
    values: Vec<Value>,
) -> Result<Option<Item>, DbErr> {
    Ok(items(c, text, values).await?.into_iter().next())
}

/// One value, the first column of the first row.
pub async fn scalar<C: ConnectionTrait, T: sea_orm::TryGetable>(
    c: &C,
    text: &str,
    values: Vec<Value>,
) -> Result<Option<T>, DbErr> {
    match c.query_one_raw(sql(text, values)).await? {
        Some(row) => row.try_get_by_index::<T>(0).map(Some),
        None => Ok(None),
    }
}

/// Every value of the first column.
pub async fn column<C: ConnectionTrait, T: sea_orm::TryGetable>(
    c: &C,
    text: &str,
    values: Vec<Value>,
) -> Result<Vec<T>, DbErr> {
    let rows = c.query_all_raw(sql(text, values)).await?;
    rows.iter().map(|r| r.try_get_by_index::<T>(0)).collect()
}

pub async fn project<C: ConnectionTrait>(c: &C, slug: &str) -> Result<ProjectRow, Failure> {
    let rows = c
        .query_all_raw(sql(
            "SELECT * FROM projects WHERE slug=?",
            vec![slug.into()],
        ))
        .await?;
    let Some(row) = rows.first() else {
        return Err(Failure::NotFound(format!("no project {slug}")));
    };
    Ok(project_row(project::Model::from_query_result(row, "")?))
}

pub async fn by_rid<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Option<Item>, DbErr> {
    item_row(c, "SELECT * FROM items WHERE rid=?", vec![rid.into()]).await
}

/// Every column of `items` but the body, which comes back empty. For reads that only derive an
/// item's state, so a table of large bodies is never loaded to compute it.
pub const STATE_COLUMNS: &str = "rid, project, key, num, id, title, state, decision, decided_at, \
    resolution, superseded_by, parent_rid, complexity, release_id, area_id, type, priority, '' AS body, \
    opened_at, updated_at";

pub async fn id_of<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Option<String>, DbErr> {
    scalar(c, "SELECT id FROM items WHERE rid=?", vec![rid.into()]).await
}

/// An item by id within a project, or none.
pub async fn item_opt<C: ConnectionTrait>(
    c: &C,
    slug: &str,
    id: &str,
) -> Result<Option<Item>, Failure> {
    let (key, num) = split_id(id)?;
    Ok(item_row(
        c,
        "SELECT * FROM items WHERE project=? AND key=? AND num=?",
        vec![slug.into(), key.into(), num.into()],
    )
    .await?)
}

/// An item by id within a project.
pub async fn item_of<C: ConnectionTrait>(c: &C, slug: &str, id: &str) -> Result<Item, Failure> {
    let (key, num) = split_id(id)?;
    item_opt(c, slug, id)
        .await?
        .ok_or_else(|| Failure::NotFound(format!("no item {key}{num} in {slug}")))
}

/// What one column of a change is called and holds.
fn column_of(f: &Field) -> (&'static str, Value) {
    match f.clone() {
        Field::Title(v) => ("title", v.into()),
        Field::State(v) => ("state", v.into()),
        Field::Decision(v) => ("decision", v.into()),
        Field::DecidedAt(v) => ("decided_at", v.into()),
        Field::Resolution(v) => ("resolution", v.into()),
        Field::SupersededBy(v) => ("superseded_by", v.into()),
        Field::ParentRid(v) => ("parent_rid", v.into()),
        Field::Complexity(v) => ("complexity", v.into()),
        Field::ReleaseId(v) => ("release_id", v.into()),
        Field::AreaId(v) => ("area_id", v.into()),
        Field::Priority(v) => ("priority", v.into()),
        Field::Body(v) => ("body", v.into()),
    }
}

/// The columns a new item is inserted with.
#[derive(Clone, Debug, Default)]
pub struct NewItem {
    pub project: String,
    pub key: String,
    pub num: i64,
    pub title: String,
    pub body: String,
    pub complexity: Option<String>,
    pub release_id: Option<i64>,
    pub area_id: Option<i64>,
    pub item_type: ItemType,
    pub priority: String,
    /// The labels it is given, by name.
    pub labels: Vec<String>,
}

/// One command's transaction: the write lock, the writes, the dump marks, the change notice, COMMIT.
pub struct Tx {
    pub conn: DatabaseTransaction,
    pub host: String,
    pub now: String,
    /// Whose key the writes are made with, owner or agent, recorded on the assignment rows they move.
    pub actor: Option<String>,
    touched: BTreeSet<i64>,
    touched_projects: BTreeSet<String>,
    _queue: OwnedMutexGuard<()>,
}

/// Writers of this process queue here, holding no pool connection, so at most one of them waits on the
/// advisory lock and the pool keeps its connections for reads.
static QUEUE: LazyLock<Arc<Mutex<()>>> = LazyLock::new(|| Arc::new(Mutex::new(())));

impl Tx {
    pub async fn begin(db: &DatabaseConnection, host: &str) -> Result<Self, DbErr> {
        let queue = Arc::clone(&QUEUE).lock_owned().await;
        let tx = db.begin().await?;
        tx.execute_unprepared(&format!("SELECT pg_advisory_xact_lock({WRITES})"))
            .await?;
        Ok(Self {
            conn: tx,
            host: host.to_string(),
            now: clock::now(),
            actor: None,
            touched: BTreeSet::new(),
            touched_projects: BTreeSet::new(),
            _queue: queue,
        })
    }

    pub async fn project(&self, slug: &str) -> Result<ProjectRow, Failure> {
        project(&self.conn, slug).await
    }

    /// An item by id, its row locked until the transaction ends.
    pub async fn item(&self, slug: &str, id: &str) -> Result<Item, Failure> {
        let (key, num) = split_id(id)?;
        item_row(
            &self.conn,
            "SELECT * FROM items WHERE project=? AND key=? AND num=? FOR UPDATE",
            vec![slug.into(), key.clone().into(), num.into()],
        )
        .await?
        .ok_or_else(|| Failure::NotFound(format!("no item {key}{num} in {slug}")))
    }

    pub async fn by_rid(&self, rid: i64) -> Result<Option<Item>, DbErr> {
        by_rid(&self.conn, rid).await
    }

    /// The row after the update, re-read.
    pub async fn fresh(&self, rid: i64) -> Result<Item, Failure> {
        self.by_rid(rid)
            .await?
            .ok_or_else(|| Failure::NotFound(format!("no item with rid {rid}")))
    }

    pub async fn execute(&self, text: &str, values: Vec<Value>) -> Result<u64, DbErr> {
        Ok(self
            .conn
            .execute_raw(sql(text, values))
            .await?
            .rows_affected())
    }

    pub fn touch(&mut self, rid: i64) {
        self.touched.insert(rid);
    }

    pub fn touch_project(&mut self, slug: &str) {
        self.touched_projects.insert(slug.to_string());
    }

    /// The columns set and the index rebuilt; the row as it reads after.
    pub async fn update(&mut self, rid: i64, changes: &[Field]) -> Result<Item, Failure> {
        let mut sets = Vec::new();
        let mut values = Vec::new();
        for (col, v) in changes.iter().map(column_of) {
            sets.push(format!("{col}=?"));
            values.push(v);
        }
        sets.push("updated_at=?".to_string());
        values.push(self.now.clone().into());
        values.push(rid.into());
        let text = format!(
            "UPDATE items SET {} WHERE rid=? RETURNING *",
            sets.join(", ")
        );
        let row = item_row(&self.conn, &text, values)
            .await?
            .ok_or_else(|| Failure::NotFound(format!("no item with rid {rid}")))?;
        self.touch(rid);
        self.index(rid).await?;
        Ok(row)
    }

    pub async fn insert(&mut self, cols: NewItem) -> Result<Item, Failure> {
        let text = "INSERT INTO items (project, key, num, title, state, body, complexity, \
                    release_id, area_id, type, priority, opened_at, updated_at) \
                    VALUES (?, ?, ?, ?, 'open', ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *";
        let slug = cols.project.clone();
        let values: Vec<Value> = vec![
            cols.project.into(),
            cols.key.into(),
            cols.num.into(),
            cols.title.into(),
            cols.body.into(),
            cols.complexity.into(),
            cols.release_id.into(),
            cols.area_id.into(),
            cols.item_type.as_str().into(),
            cols.priority.into(),
            self.now.clone().into(),
            self.now.clone().into(),
        ];
        let row = item_row(&self.conn, text, values)
            .await?
            .ok_or_else(|| Failure::Db(DbErr::RecordNotInserted))?;
        for name in &cols.labels {
            crate::verbs::labels::give(&self.conn, &slug, row.rid, name).await?;
        }
        self.touch(row.rid);
        self.index(row.rid).await?;
        Ok(row)
    }

    /// The next number under a key, the key locked until the transaction ends.
    pub async fn next_num(&self, slug: &str, key: &str) -> Result<i64, DbErr> {
        self.execute(
            "SELECT pg_advisory_xact_lock(hashtext(?), hashtext(?))",
            vec![slug.into(), key.into()],
        )
        .await?;
        Ok(scalar::<_, i64>(
            &self.conn,
            "SELECT COALESCE(MAX(num), 0) + 1 FROM items WHERE project=? AND key=?",
            vec![slug.into(), key.into()],
        )
        .await?
        .unwrap_or(1))
    }

    /// The search row and the citation links of one item, rebuilt from its body.
    pub async fn index(&self, rid: i64) -> Result<(), Failure> {
        let r = self.fresh(rid).await?;
        self.execute(
            "DELETE FROM links WHERE rid=? AND kind IN ('cites_file', 'cites_test')",
            vec![rid.into()],
        )
        .await?;
        let cites = citations(&r.body);
        for c in &cites {
            self.execute(
                "INSERT INTO links (rid, kind, to_path, to_line) VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
                vec![
                    rid.into(),
                    c.kind.into(),
                    c.path.clone().into(),
                    c.line.into(),
                ],
            )
            .await?;
        }
        let mut files: Vec<&str> = cites.iter().map(|c| c.path.as_str()).collect();
        files.sort_unstable();
        files.dedup();
        self.execute(
            "INSERT INTO search (rid, id, title, body, files) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT (rid) DO UPDATE SET id=EXCLUDED.id, title=EXCLUDED.title, body=EXCLUDED.body, \
             files=EXCLUDED.files",
            vec![
                rid.into(),
                r.id.into(),
                r.title.into(),
                r.body.into(),
                files.join(" ").into(),
            ],
        )
        .await?;
        Ok(())
    }

    /// An event on an item, or on the project when rid is None.
    pub async fn event(
        &self,
        project: &str,
        rid: Option<i64>,
        kind: &str,
        note: Option<&str>,
        branch: Option<&str>,
        data: Option<&Json>,
    ) -> Result<(), DbErr> {
        let given = data.filter(|d| d.as_object().is_some_and(|m| !m.is_empty()));
        let data = given.map_or(Value::Json(None), |d| json(d.clone()));
        let uid = format!(
            "{}-{:012}-{}-{:06x}",
            self.now,
            WRITTEN.fetch_add(1, Ordering::Relaxed),
            self.host,
            rand::random::<u32>() & 0x00ff_ffff
        );
        self.execute(
            "INSERT INTO events (uid, project, rid, at, host, branch, kind, note, data) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            vec![
                uid.into(),
                project.into(),
                rid.into(),
                self.now.clone().into(),
                self.host.clone().into(),
                branch.map(str::to_string).into(),
                kind.into(),
                note.map(str::to_string).into(),
                data,
            ],
        )
        .await?;
        if let Some(rid) = rid {
            let e = assignment::Event {
                kind: kind.to_string(),
                at: self.now.clone(),
                host: self.host.clone(),
                branch: branch.map(str::to_string),
                note: note.map(str::to_string),
                data: given.cloned(),
            };
            self.assign(rid, &e).await?;
        }
        Ok(())
    }

    /// The note of the item's open ask set, what it needs of the owner; refused when the owner
    /// holds no ask on it.
    pub async fn note_ask(&mut self, rid: i64, note: Option<&str>) -> Result<(), Failure> {
        let set = self
            .execute(
                "UPDATE assignments SET note=? WHERE rid=? AND ended_at IS NULL AND kind='ask'",
                vec![note.map(str::to_string).into(), rid.into()],
            )
            .await?;
        if set == 0 {
            let id = id_of(&self.conn, rid).await?.unwrap_or_default();
            return Err(Failure::Refused(format!(
                "{id} is not the owner's turn, so it has no note for the owner: ask it to hand it over."
            )));
        }
        self.touch(rid);
        Ok(())
    }

    /// The item's assignment rows moved as the event says, in the event's transaction.
    async fn assign(&self, rid: i64, e: &assignment::Event) -> Result<(), DbErr> {
        let open = assignments::open(&self.conn, rid).await?;
        let step = assignment::step(e, open.map(|(_, kind)| kind));
        if let (Some(end), Some((id, _))) = (&step.end, open) {
            assignments::end(&self.conn, id, &e.at, end, self.actor.as_deref()).await?;
        }
        if let (Some(u), Some((id, _))) = (&step.usage, open) {
            assignments::report(&self.conn, id, u).await?;
        }
        if let Some(mut row) = step.open {
            row.actor.clone_from(&self.actor);
            assignments::insert(&self.conn, rid, &row).await?;
        }
        Ok(())
    }

    /// An item link set or removed. The unique index never collides on item links, so it is checked.
    pub async fn set_link(
        &mut self,
        rid: i64,
        kind: &str,
        to_rid: i64,
        remove: bool,
    ) -> Result<(), DbErr> {
        if remove {
            self.execute(
                "DELETE FROM links WHERE rid=? AND kind=? AND to_rid=?",
                vec![rid.into(), kind.into(), to_rid.into()],
            )
            .await?;
        } else {
            let have: Option<i64> = scalar(
                &self.conn,
                "SELECT COUNT(*) FROM links WHERE rid=? AND kind=? AND to_rid=?",
                vec![rid.into(), kind.into(), to_rid.into()],
            )
            .await?;
            if have.unwrap_or(0) == 0 {
                self.execute(
                    "INSERT INTO links (rid, kind, to_rid) VALUES (?, ?, ?)",
                    vec![rid.into(), kind.into(), to_rid.into()],
                )
                .await?;
            }
        }
        self.touch(rid);
        Ok(())
    }

    /// The dump marks for every row touched, the change notice, then COMMIT.
    pub async fn commit(self) -> Result<(), DbErr> {
        for rid in &self.touched {
            if let Some(r) = self.by_rid(*rid).await? {
                self.execute(
                    "INSERT INTO pending_dump (rid, project) VALUES (?, ?) \
                     ON CONFLICT (rid) DO UPDATE SET project=EXCLUDED.project",
                    vec![(*rid).into(), r.project.into()],
                )
                .await?;
            }
        }
        if !self.touched_projects.is_empty() {
            let had: Option<String> = scalar(
                &self.conn,
                "SELECT v FROM meta WHERE k='pending_dump_projects'",
                vec![],
            )
            .await?;
            let mut slugs: BTreeSet<String> = had
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or_default();
            slugs.extend(self.touched_projects.iter().cloned());
            self.execute(
                "INSERT INTO meta (k, v) VALUES ('pending_dump_projects', ?) \
                 ON CONFLICT (k) DO UPDATE SET v=EXCLUDED.v",
                vec![jsontext::dumps(&serde_json::json!(slugs), false).into()],
            )
            .await?;
        }
        let payload = crate::changes::payload(&self.touched_projects).replace('\'', "''");
        self.conn
            .execute_unprepared(&format!("NOTIFY {CHANNEL}, '{payload}'"))
            .await?;
        self.conn.commit().await
    }
}

impl From<Refused> for Failure {
    fn from(r: Refused) -> Self {
        Failure::Refused(r.0)
    }
}

impl From<DbErr> for Failure {
    fn from(e: DbErr) -> Self {
        Failure::Db(e)
    }
}

#[cfg(test)]
#[path = "tests/store.rs"]
mod tests;
