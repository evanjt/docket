//! Scenario: one `/graph` over a project of 2,000 items, each citing 10 files and related to 2 others.
//! Expected behaviour: the memory the request holds at its peak is under 3 times the response body,
//! so the answer is serialized from the rows rather than built as a second tree beside them.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use tower::ServiceExt;

use docket_migration::scratch::Scratch;
use docket_server::auth::Keys;

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(by: usize) {
    let now = LIVE.fetch_add(by, Ordering::Relaxed) + by;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        grew(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        grew(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        if new >= layout.size() {
            grew(new - layout.size());
        } else {
            LIVE.fetch_sub(layout.size() - new, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

const SEED: &str = r"
INSERT INTO projects (slug, created_at, updated_at) VALUES ('o/p', 'c', 'u');
INSERT INTO items (rid, project, key, num, title, state, body, opened_at, updated_at)
  SELECT n, 'o/p', 'T', n, 'Item number ' || n, 'open', repeat('body ', 400), 'o', 'u'
  FROM generate_series(1, 2000) AS n;
INSERT INTO links (rid, kind, to_path, to_line)
  SELECT n, 'cites_file', 'src/module_' || n || '/file_' || f || '.rs', f
  FROM generate_series(1, 2000) AS n, generate_series(1, 10) AS f;
INSERT INTO links (rid, kind, to_rid)
  SELECT n, 'related', (n + d - 1) % 2000 + 1
  FROM generate_series(1, 2000) AS n, generate_series(1, 2) AS d;
";

#[tokio::test]
async fn test_graph_peak_memory_is_a_few_times_its_response() {
    let s = Scratch::new(2).await;
    s.seed(SEED).await;
    let app = docket_server::app(&s.db, Keys::parse("devbox agent secret").unwrap());
    let request = Request::builder()
        .uri("/graph?project=o/p")
        .header(AUTHORIZATION, "Bearer secret")
        .body(Body::empty())
        .unwrap();

    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let peak = PEAK.load(Ordering::Relaxed) - base;

    assert!(
        peak < 3 * body.len(),
        "the request peaked {peak} bytes above its start for a {} byte answer",
        body.len()
    );
}
