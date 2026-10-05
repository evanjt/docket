//! One write's transaction: the rows, their events and links, the index and the dump mark.

use std::collections::BTreeSet;
use std::sync::{Arc, LazyLock};

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, FromQueryResult, Statement,
    TransactionTrait, Value,
};
use serde_json::Value as Json;
use tokio::sync::{Mutex, OwnedMutexGuard};

use docket_core::clock;
use docket_core::item::{Field, Item, KeySpec, Project, Refused};
use docket_core::pyjson;
use docket_core::text::{citations, split_id};

use crate::changes::CHANNEL;
use crate::entities::{item, project};
use crate::verbs::Failure;

/// A project row, with the keys parsed and the raw columns a verb reads.
#[derive(Clone, Debug)]
pub struct ProjectRow {
    pub rules: Project,
    pub keys: Vec<Json>,
    pub worktree_hint: Option<String>,
}

/// The item as stored, tags decoded.
pub fn to_item(m: item::Model) -> Item {
    Item {
        rid: m.rid,
        project: m.project,
        key: m.key,
        num: m.num,
        id: m.id,
        title: m.title,
        state: m.state,
        turn: m.turn,
        turn_note: m.turn_note,
        asked_at: m.asked_at,
        claim_branch: m.claim_branch,
        claim_host: m.claim_host,
        claim_since: m.claim_since,
        claim_runner: m.claim_runner,
        claim_job: m.claim_job,
        claim_on: m.claim_on,
        wait_on: m.wait_on,
        wait_item: m.wait_item,
        wait_ref: m.wait_ref,
        wait_since: m.wait_since,
        decision: m.decision,
        decided_at: m.decided_at,
        resolution: m.resolution,
        superseded_by: m.superseded_by,
        scope: m.scope,
        complexity: m.complexity,
        group_name: m.group_name,
        theme: m.theme,
        rank: m.rank,
        tags: serde_json::from_value(m.tags).unwrap_or_default(),
        body: m.body,
        conflict: m.conflict,
        opened_at: m.opened_at,
        updated_at: m.updated_at,
    }
}

/// A project's keys from its stored JSON, skipping a spec the rules cannot read.
pub fn project_row(m: project::Model) -> ProjectRow {
    let keys: Vec<Json> = serde_json::from_value(m.keys).unwrap_or_default();
    let specs = keys
        .iter()
        .filter_map(|k| serde_json::from_value::<KeySpec>(k.clone()).ok())
        .collect();
    ProjectRow {
        rules: Project {
            slug: m.slug,
            keys: specs,
        },
        keys,
        worktree_hint: m.worktree_hint,
    }
}

/// The advisory lock every write transaction holds, so writes commit one at a time and their events
/// become visible in `seq` order.
pub(crate) const WRITES: i64 = 0x0064_6f63_6b65_7401;

/// A statement for text written with `?` marks.
pub fn sql(text: &str, values: Vec<Value>) -> Statement {
    docket_migration::statement(text, values)
}

/// A JSON value to bind to a `jsonb` column.
pub fn json(value: Json) -> Value {
    Value::Json(Some(Box::new(value)))
}

/// Item rows for a query, each as the rules read it.
pub async fn items<C: ConnectionTrait>(
    c: &C,
    text: &str,
    values: Vec<Value>,
) -> Result<Vec<Item>, DbErr> {
    let rows = c.query_all_raw(sql(text, values)).await?;
    rows.iter()
        .map(|r| item::Model::from_query_result(r, "").map(to_item))
        .collect()
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
pub const STATE_COLUMNS: &str = "rid, project, key, num, id, title, state, turn, turn_note, asked_at, \
    claim_branch, claim_host, claim_since, claim_runner, claim_job, claim_on, wait_on, wait_item, \
    wait_ref, wait_since, decision, decided_at, resolution, superseded_by, scope, complexity, \
    group_name, theme, rank, tags, '' AS body, conflict, opened_at, updated_at";

/// An item by rid without its body.
pub async fn state_by_rid<C: ConnectionTrait>(c: &C, rid: i64) -> Result<Option<Item>, DbErr> {
    item_row(
        c,
        &format!("SELECT {STATE_COLUMNS} FROM items WHERE rid=?"),
        vec![rid.into()],
    )
    .await
}

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
        Field::Turn(v) => ("turn", v.into()),
        Field::TurnNote(v) => ("turn_note", v.into()),
        Field::AskedAt(v) => ("asked_at", v.into()),
        Field::ClaimBranch(v) => ("claim_branch", v.into()),
        Field::ClaimHost(v) => ("claim_host", v.into()),
        Field::ClaimSince(v) => ("claim_since", v.into()),
        Field::ClaimRunner(v) => ("claim_runner", v.into()),
        Field::ClaimJob(v) => ("claim_job", v.into()),
        Field::ClaimOn(v) => ("claim_on", v.into()),
        Field::WaitOn(v) => ("wait_on", v.into()),
        Field::WaitItem(v) => ("wait_item", v.into()),
        Field::WaitRef(v) => ("wait_ref", v.into()),
        Field::WaitSince(v) => ("wait_since", v.into()),
        Field::Decision(v) => ("decision", v.into()),
        Field::DecidedAt(v) => ("decided_at", v.into()),
        Field::Resolution(v) => ("resolution", v.into()),
        Field::SupersededBy(v) => ("superseded_by", v.into()),
        Field::Scope(v) => ("scope", v.into()),
        Field::Complexity(v) => ("complexity", v.into()),
        Field::GroupName(v) => ("group_name", v.into()),
        Field::Theme(v) => ("theme", v.into()),
        Field::Rank(v) => ("rank", v.into()),
        Field::Tags(v) => ("tags", json(serde_json::json!(v))),
        Field::Body(v) => ("body", v.into()),
        Field::Conflict(v) => ("conflict", v.into()),
    }
}

/// The columns a new item is inserted with.
#[derive(Clone, Debug, Default)]
pub struct NewItem {
    pub project: String,
    pub key: String,
    pub num: i64,
    pub title: String,
    pub turn: String,
    pub body: String,
    pub complexity: Option<String>,
    pub theme: Option<String>,
    pub group_name: Option<String>,
    pub scope: Option<String>,
    pub tags: Vec<String>,
}

/// One command's transaction: the write lock, the writes, the dump marks, the change notice, COMMIT.
pub struct Tx {
    pub conn: DatabaseTransaction,
    pub host: String,
    pub now: String,
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

    pub async fn items(&self, text: &str, values: Vec<Value>) -> Result<Vec<Item>, DbErr> {
        items(&self.conn, text, values).await
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
        let text = "INSERT INTO items (project, key, num, title, state, turn, body, complexity, theme, \
                    group_name, scope, tags, opened_at, updated_at) \
                    VALUES (?, ?, ?, ?, 'open', ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *";
        let values: Vec<Value> = vec![
            cols.project.into(),
            cols.key.into(),
            cols.num.into(),
            cols.title.into(),
            cols.turn.into(),
            cols.body.into(),
            cols.complexity.into(),
            cols.theme.into(),
            cols.group_name.into(),
            cols.scope.into(),
            json(serde_json::json!(cols.tags)),
            self.now.clone().into(),
            self.now.clone().into(),
        ];
        let row = item_row(&self.conn, text, values)
            .await?
            .ok_or_else(|| Failure::Db(DbErr::RecordNotInserted))?;
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
        let data = data
            .filter(|d| d.as_object().is_some_and(|m| !m.is_empty()))
            .map_or(Value::Json(None), |d| json(d.clone()));
        let uid = format!(
            "{}-{}-{:06x}",
            self.now,
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
                vec![pyjson::dumps(&serde_json::json!(slugs), false).into()],
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
