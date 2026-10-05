use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use tower::ServiceExt;

use docket_migration::scratch::Scratch;

use crate::app;
use crate::auth::Keys;

const SEED: &str = r#"
INSERT INTO projects (slug, keys, skills, created_at, updated_at) VALUES
  ('acme/widgets', '[{"key":"T","kind":"work"},{"key":"A","kind":"audit"}]', '{"owner":"Ada Lovelace"}', 'c', 'u'),
  ('acme/gizmo', '[{"key":"B","kind":"work"}]', '{}', 'c', 'u');
INSERT INTO releases (id, project, name, position) VALUES (1, 'acme/widgets', '2.4', 0), (2, 'acme/widgets', '2.5', 1);
INSERT INTO items (rid, project, key, num, title, state, turn, tags, body, claim_branch, claim_host, claim_since, opened_at, updated_at, theme, group_name)
  VALUES (1, 'acme/widgets', 'T', 1, 'The proofing timer drifts after a restart', 'open', 'agent', '[]', '', 'b', 'delta', 's', 'o', 'u', 'Lanterns', 'harbour-lights');
INSERT INTO events (uid, project, rid, at, host, kind) VALUES ('e1', 'acme/widgets', 1, 'a', 'gamma.example.org', 'opened');
INSERT INTO machines (name, ssh, slots, runners, note, updated_at)
  VALUES ('alpha', 'user@203.0.113.7', 2, '["claude"]', NULL, 'u');
"#;

async fn get(key: &str) -> (StatusCode, Value) {
    let db = Scratch::new(2).await;
    db.seed(SEED).await;
    let app = app(
        &db.db,
        Keys::parse("alpha owner ownerkey\nbeta agent agentkey").unwrap(),
    );
    let req = Request::builder()
        .uri("/private")
        .header(AUTHORIZATION, format!("Bearer {key}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn test_the_owner_reads_every_private_name() {
    let (status, body) = get("ownerkey").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({
            "projects": ["acme/gizmo", "acme/widgets"],
            "owners": ["Ada Lovelace"],
            "machines": [["alpha", "user@203.0.113.7"]],
            "hosts": ["delta", "gamma.example.org"],
            "keys": ["A", "B", "T"],
            "themes": ["Lanterns"],
            "groups": ["harbour-lights"],
            "releases": ["2.4", "2.5"],
            "titles": ["The proofing timer drifts after a restart"],
        })
    );
}

#[tokio::test]
async fn test_an_agent_key_is_refused_the_private_names() {
    assert_eq!(get("agentkey").await.0, StatusCode::FORBIDDEN);
}
