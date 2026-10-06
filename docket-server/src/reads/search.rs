use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use sea_orm::sea_query::NullOrdering;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, FromQueryResult, Order,
    QueryFilter, QueryOrder,
};
use serde::Deserialize;
use serde_json::{Value, json};

use docket_core::search::{any_of, fts_query, similar_terms};

use crate::entities::{item, link, project};
use crate::reads::public::{
    Failure, failure, internal, item_of, member_counts, project_of, public, sql,
};
use crate::reads::queue::under_cond;
use crate::verbs::labels::{carried_cond, exactly};
use docket_core::word::kind_of_type;

const STATES: [&str; 4] = ["open", "done", "dropped", "any"];

/// What narrows a full-text query besides the words.
#[derive(Default)]
pub(crate) struct Narrow {
    pub key: Option<String>,
    /// Only items with a non-empty decision whose key is one of these; unused when empty.
    pub decided: Vec<String>,
    pub state: Option<String>,
    pub exclude: Option<i64>,
    /// Only the items carrying this label.
    pub label: Option<String>,
    /// Only the items not carrying this label.
    pub without_label: Option<String>,
    /// Only items under this plan, story or concept, by the rule `next` applies.
    pub under: Option<String>,
    pub n: i64,
}

/// The weights of the body, files, id and title, as `ts_rank` takes them (D, C, B, A).
const WEIGHTS: &str = "'{0.125, 0.25, 0.5, 1}'";

/// How `ts_headline` cuts a body: one fragment of up to twelve words, matches in brackets.
const SNIPPET: &str =
    "'StartSel=[, StopSel=], MaxWords=12, MinWords=4, MaxFragments=1, FragmentDelimiter=\" ... \"'";

/// The items of a project matching a `websearch_to_tsquery` text, best first, as `i.*` with the
/// matched body, the query and the score. The query's separators are read as the index reads them.
/// The score is the rank negated: lower is closer.
pub(crate) fn ranked(
    slug: &str,
    query: &str,
    narrow: &Narrow,
    under: (String, Vec<sea_orm::Value>),
) -> (String, Vec<sea_orm::Value>) {
    let mut text = format!(
        "SELECT i.*, s.body AS matched, q, (-ts_rank({WEIGHTS}, s.doc, q))::float8 AS score \
         FROM search s JOIN items i ON i.rid = s.rid, websearch_to_tsquery('simple', translate(?, '/:<>@?=&#%~+', '            ')) q \
         WHERE s.doc @@ q AND i.project=?"
    );
    let mut values: Vec<sea_orm::Value> = vec![query.into(), slug.into()];
    if let Some(key) = &narrow.key {
        text.push_str(" AND i.key=?");
        values.push(key.to_uppercase().into());
    }
    if !narrow.decided.is_empty() {
        text.push_str(" AND i.key IN (");
        text.push_str(&crate::reads::public::marks(narrow.decided.len()));
        text.push_str(") AND i.decision IS NOT NULL AND i.decision<>''");
        values.extend(narrow.decided.iter().map(|k| k.as_str().into()));
    }
    if let Some(state) = narrow.state.as_deref().filter(|s| *s != "any") {
        text.push_str(" AND i.state=?");
        values.push(state.into());
    }
    if let Some(rid) = narrow.exclude {
        text.push_str(" AND i.rid<>?");
        values.push(rid.into());
    }
    for (name, not) in [(&narrow.label, false), (&narrow.without_label, true)] {
        if let Some(name) = name {
            text.push_str(&carried_cond("i.rid", not));
            values.push(slug.into());
            values.push(exactly(name).into());
        }
    }
    text.push_str(&under.0);
    values.extend(under.1);
    text.push_str(" ORDER BY score, i.rid LIMIT ?");
    values.push(narrow.n.into());
    (text, values)
}

/// Matches ranked over title, id, files and body, each with a snippet of its body.
pub(crate) async fn search_rows(
    db: &DatabaseConnection,
    slug: &str,
    query: &str,
    narrow: &Narrow,
) -> Result<Vec<(item::Model, f64, String)>, Failure> {
    let under = under_cond(db, slug, narrow.under.as_deref(), "i.rid").await?;
    let (inner, values) = ranked(slug, query, narrow, under);
    let text = format!(
        "SELECT m.*, ts_headline('simple', m.matched, m.q, {SNIPPET}) AS snip FROM ({inner}) m \
         ORDER BY m.score, m.rid"
    );
    let rows = db
        .query_all_raw(sql(&text, values))
        .await
        .map_err(|e| internal(&e))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let model = item::Model::from_query_result(&row, "").map_err(|e| internal(&e))?;
        let score: f64 = row.try_get("", "score").map_err(|e| internal(&e))?;
        let snip: String = row.try_get("", "snip").map_err(|e| internal(&e))?;
        out.push((model, score, snip));
    }
    Ok(out)
}

/// The rows shaped as `--json` prints them, each carrying its score and snippet.
async fn shaped(
    db: &DatabaseConnection,
    project: &project::Model,
    rows: Vec<(item::Model, f64, String)>,
    snippet: bool,
) -> Result<Json<Value>, Failure> {
    let counts = member_counts(db, &project.slug)
        .await
        .map_err(|e| internal(&e))?;
    let mut out = Vec::with_capacity(rows.len());
    for (row, score, snip) in rows {
        let open = counts.get(&row.rid).copied().unwrap_or_default();
        let mut d = public(db, kind_of_type(&row.item_type), row, open, None)
            .await
            .map_err(|e| internal(&e))?;
        d.insert("score".into(), json!(score));
        d.insert("snip".into(), json!(snip));
        if snippet {
            d.insert("snippet".into(), json!(snip));
        }
        out.push(Value::Object(d));
    }
    Ok(Json(Value::Array(out)))
}

#[derive(Deserialize)]
pub struct SearchQuery {
    project: String,
    #[serde(default)]
    q: String,
    key: Option<String>,
    state: Option<String>,
    #[serde(default = "twenty")]
    n: i64,
    #[serde(default)]
    raw: bool,
    /// `theme` is the name it had.
    #[serde(alias = "theme")]
    label: Option<String>,
    #[serde(alias = "without_theme")]
    without_label: Option<String>,
    under: Option<String>,
}

fn twenty() -> i64 {
    20
}

/// `docket search`: full-text search, ranked, with a snippet. `raw` passes the query through as
/// `websearch_to_tsquery` reads it: quoted phrases, `or`, and `-` before a word to leave it out.
///
/// # Errors
/// 400 without words, 404 for an unknown project.
pub async fn search(
    State(db): State<DatabaseConnection>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Value>, Failure> {
    crate::reads::public::one_of("state", q.state.as_deref(), &STATES)?;
    let project = project_of(&db, &q.project).await?;
    let query = if q.raw {
        q.q.clone()
    } else {
        fts_query(&q.q).ok_or_else(|| failure(StatusCode::BAD_REQUEST, "search needs words."))?
    };
    let narrow = Narrow {
        key: q.key,
        decided: Vec::new(),
        state: q.state,
        exclude: None,
        label: q.label,
        without_label: q.without_label,
        under: q.under,
        n: q.n,
    };
    let rows = search_rows(&db, &q.project, &query, &narrow).await?;
    shaped(&db, &project, rows, true).await
}

#[derive(Deserialize)]
pub struct SimilarQuery {
    project: String,
    #[serde(default = "ten")]
    n: i64,
    state: Option<String>,
}

fn ten() -> i64 {
    10
}

/// `docket similar`: items close to one by its title's words, the files it cites and its symbols.
///
/// # Errors
/// 400 for a state outside its options, 404 for an unknown project or item.
pub async fn similar(
    State(db): State<DatabaseConnection>,
    Path(id): Path<String>,
    Query(q): Query<SimilarQuery>,
) -> Result<Json<Value>, Failure> {
    crate::reads::public::one_of("state", q.state.as_deref(), &STATES)?;
    let project = project_of(&db, &q.project).await?;
    let row = item_of(&db, &q.project, &id).await?;
    let narrow = Narrow {
        state: q.state,
        exclude: Some(row.rid),
        n: q.n,
        ..Narrow::default()
    };
    let rows = similar_rows(&db, &q.project, &row, &narrow).await?;
    shaped(&db, &project, rows, false).await
}

/// The items close to `row` by its title's words, the files it cites and its symbols.
pub(crate) async fn similar_rows(
    db: &DatabaseConnection,
    slug: &str,
    row: &item::Model,
    narrow: &Narrow,
) -> Result<Vec<(item::Model, f64, String)>, Failure> {
    let cited: Vec<String> = link::Entity::find()
        .filter(link::Column::Rid.eq(row.rid))
        .order_by_asc(link::Column::Kind)
        .order_by_with_nulls(link::Column::ToRid, Order::Asc, NullOrdering::First)
        .order_by_with_nulls(link::Column::ToPath, Order::Asc, NullOrdering::First)
        .order_by_with_nulls(link::Column::ToLine, Order::Asc, NullOrdering::First)
        .order_by_asc(link::Column::Id)
        .all(db)
        .await
        .map_err(|e| internal(&e))?
        .into_iter()
        .filter_map(|l| l.to_path)
        .collect();
    let terms = similar_terms(&row.title, &cited, &row.body);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    search_rows(db, slug, &any_of(&terms), narrow).await
}
