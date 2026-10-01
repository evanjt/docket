use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, FromQueryResult, QueryFilter,
};
use serde::Deserialize;
use serde_json::{Value, json};

use docket_core::search::{any_of, fts_query, similar_terms};

use crate::entities::{item, link, project};
use crate::reads::public::{
    Failure, Kinds, failure, internal, item_of, open_members, project_of, public, sql,
};

const STATES: [&str; 4] = ["open", "done", "dropped", "any"];

/// What narrows a full-text query besides the words.
#[derive(Default)]
pub(crate) struct Narrow {
    pub key: Option<String>,
    pub state: Option<String>,
    pub exclude: Option<i64>,
    pub theme: Option<String>,
    pub without_theme: Option<String>,
    pub n: i64,
}

/// Matches ranked by bm25 over id, title, body and files, each with a snippet of its body.
pub(crate) async fn search_rows(
    db: &DatabaseConnection,
    slug: &str,
    query: &str,
    narrow: &Narrow,
) -> Result<Vec<(item::Model, f64, String)>, Failure> {
    let mut text = "SELECT i.*, bm25(items_fts, 4.0, 8.0, 1.0, 2.0) AS score, \
                    snippet(items_fts, 2, '[', ']', ' ... ', 12) AS snip \
                    FROM items_fts JOIN items i ON i.rid = items_fts.rowid \
                    WHERE items_fts MATCH ? AND i.project=?"
        .to_string();
    let mut values: Vec<sea_orm::Value> = vec![query.into(), slug.into()];
    if let Some(key) = &narrow.key {
        text.push_str(" AND i.key=?");
        values.push(key.to_uppercase().into());
    }
    if let Some(state) = narrow.state.as_deref().filter(|s| *s != "any") {
        text.push_str(" AND i.state=?");
        values.push(state.into());
    }
    if let Some(rid) = narrow.exclude {
        text.push_str(" AND i.rid<>?");
        values.push(rid.into());
    }
    if let Some(theme) = &narrow.theme {
        text.push_str(" AND i.theme LIKE ?");
        values.push(format!("%{theme}%").into());
    }
    if let Some(theme) = &narrow.without_theme {
        text.push_str(" AND (i.theme IS NULL OR i.theme NOT LIKE ?)");
        values.push(format!("%{theme}%").into());
    }
    text.push_str(" ORDER BY score LIMIT ?");
    values.push(narrow.n.into());
    let rows = db.query_all_raw(sql(&text, values)).await.map_err(|e| {
        failure(
            StatusCode::BAD_REQUEST,
            &format!("search could not parse {query:?}: {e}"),
        )
    })?;
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
    let kinds = Kinds::of(project);
    let counts = open_members(db, &project.slug)
        .await
        .map_err(|e| internal(&e))?;
    let mut out = Vec::with_capacity(rows.len());
    for (row, score, snip) in rows {
        let open = counts.get(&row.rid).copied().unwrap_or(0);
        let mut d = public(db, kinds.kind(&row.key), row, open, None)
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
    theme: Option<String>,
    without_theme: Option<String>,
}

fn twenty() -> i64 {
    20
}

/// `docket search`: full-text search, ranked, with a snippet. `raw` passes the FTS5 query through.
///
/// # Errors
/// 400 without words or for a query FTS5 refuses, 404 for an unknown project.
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
        state: q.state,
        exclude: None,
        theme: q.theme,
        without_theme: q.without_theme,
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
    let cited: Vec<String> = link::Entity::find()
        .filter(link::Column::Rid.eq(row.rid))
        .all(&db)
        .await
        .map_err(|e| internal(&e))?
        .into_iter()
        .filter_map(|l| l.to_path)
        .collect();
    let terms = similar_terms(&row.title, &cited, &row.body);
    if terms.is_empty() {
        return Ok(Json(json!([])));
    }
    let narrow = Narrow {
        state: q.state,
        exclude: Some(row.rid),
        n: q.n,
        ..Narrow::default()
    };
    let rows = search_rows(&db, &q.project, &any_of(&terms), &narrow).await?;
    shaped(&db, &project, rows, false).await
}
