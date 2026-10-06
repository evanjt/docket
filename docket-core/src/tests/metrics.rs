use super::*;
use crate::fact::{Prices, prices_of};
use std::collections::HashSet;

const NOW: i64 = 100 * DAY + 100;

fn day(n: i64) -> i64 {
    n * DAY + 3600
}

fn subject(
    id: &str,
    word: &str,
    opened: i64,
    started: Option<i64>,
    closed: Option<i64>,
) -> Subject {
    Subject {
        id: id.into(),
        word: word.into(),
        release: Some(0),
        opened_at: day(opened),
        closed_at: closed.map(day),
        started_at: started.map(day),
        claimed: false,
        area: None,
        holder: false,
    }
}

fn fixed() -> Vec<Subject> {
    vec![
        subject("T1", "done", 90, Some(91), Some(95)),
        subject("T2", "done", 92, Some(93), Some(99)),
        subject("T3", "ready", 98, None, None),
        subject("T4", "dropped", 97, None, Some(98)),
        subject("T5", "blocked", 99, None, None),
    ]
}

fn span(item: &str, kind: Kind, runner: Option<&str>, start: i64, end: Option<i64>) -> Span {
    Span {
        item: item.into(),
        kind,
        runner: runner.map(str::to_string),
        start,
        end,
        outcome: None,
        settled: None,
        tokens_in: None,
        tokens_out: None,
        cost_reported: None,
        model: None,
    }
}

#[test]
fn test_progress_is_done_of_everything_and_counts_open_by_word() {
    let p = progress(&fixed());
    assert_eq!((p.done, p.dropped, p.total), (2, 1, 5));
    assert_eq!(
        p.open,
        BTreeMap::from([("blocked".into(), 1), ("ready".into(), 1)])
    );
}

#[test]
fn test_throughput_reads_done_per_calendar_day_with_empty_days_as_zero() {
    let got: Vec<(String, u64)> = throughput(&fixed(), NOW, 3)
        .into_iter()
        .map(|d| (d.date, d.done))
        .collect();
    assert_eq!(
        got,
        [
            ("1970-04-09".to_string(), 0),
            ("1970-04-10".to_string(), 1),
            ("1970-04-11".to_string(), 0)
        ]
    );
}

#[test]
fn test_cycle_and_lead_times_match_hand_counts() {
    let lead = lead_time(&fixed()).unwrap();
    assert_eq!((lead.n, lead.median, lead.mean), (2, 6 * DAY, 6 * DAY));
    let cycle = cycle_time(&fixed()).unwrap();
    assert_eq!((cycle.n, cycle.median), (2, 5 * DAY));
    assert_eq!(lead_time(&[subject("T9", "ready", 1, None, None)]), None);
}

#[test]
fn test_time_spent_splits_agent_person_owner_and_blocked() {
    let mut blocked = span("T1", Kind::Claim, Some("claude"), 0, Some(600));
    blocked.outcome = Some(Outcome::Blocked);
    let spans = [
        blocked,
        span("T1", Kind::Claim, Some("claude"), 1000, Some(1300)),
        span("T2", Kind::Claim, None, 0, Some(90)),
        span("T2", Kind::Ask, None, 100, Some(160)),
    ];
    assert_eq!(
        time_spent(&spans, 5000),
        TimeSpent {
            agent: 900,
            person: 90,
            waiting_owner: 60,
            blocked: 400
        }
    );
}

fn run(item: &str, runner: Option<&str>, secs: i64, tokens: (i64, i64), cost: Option<f64>) -> Span {
    let mut s = span(item, Kind::Claim, runner, 0, Some(secs));
    s.tokens_in = Some(tokens.0);
    s.tokens_out = Some(tokens.1);
    s.cost_reported = cost;
    s
}

#[test]
fn test_cost_sums_agent_attempts_and_leaves_the_owners_out() {
    let spans = [
        run("T1", Some("claude"), 60, (10, 2), Some(0.25)),
        run("T1", Some("claude"), 30, (4, 5), Some(0.5)),
        run("T2", Some("codex"), 120, (7, 3), None),
        run("T2", None, 600, (0, 0), None),
    ];
    let got = cost(&spans, NOW, &Prices::default());
    assert_eq!((got.tokens_in, got.tokens_out), (21, 10));
    assert_eq!(got.agent_seconds, 210);
    assert_eq!((got.attempts, got.attempts_reported), (3, 2));
    assert!((got.cost_reported.unwrap() - 0.75).abs() < f64::EPSILON);
    assert_eq!(
        cost(&spans[2..], NOW, &Prices::default()).cost_reported,
        None
    );
}

#[test]
fn test_cost_counts_an_open_attempt_up_to_now() {
    let open = span("T1", Kind::Claim, Some("claude"), NOW - 40, None);
    assert_eq!(cost(&[open], NOW, &Prices::default()).agent_seconds, 40);
}

/// Ten open items against four closes and two opens a day over three days.
fn shrinking() -> Vec<Subject> {
    let mut items: Vec<Subject> = (0..10)
        .map(|n| subject(&format!("T{n}"), "ready", 10, None, None))
        .collect();
    for (n, d) in [98, 98, 98, 98, 99, 99, 99, 99, 100, 100, 100, 100]
        .iter()
        .enumerate()
    {
        items.push(subject(&format!("D{n}"), "done", 10, None, Some(*d)));
    }
    for (n, d) in [98, 98, 99, 99, 100, 100].iter().enumerate() {
        items.push(subject(&format!("N{n}"), "ready", *d, None, None));
    }
    items
}

#[test]
fn test_forecast_of_a_release_with_no_burn_is_not_converging() {
    let items: Vec<Subject> = (0..4)
        .map(|n| subject(&format!("T{n}"), "ready", 10, None, None))
        .collect();
    let f = forecast(&items, 0, NOW, 3, 7, Some("1970-06-01"));
    assert!(!f.converging);
    assert_eq!((f.p50, f.p85, f.late), (None, None, Some(true)));
}

#[test]
fn test_forecast_gives_the_same_dates_for_the_same_seed() {
    let items = shrinking();
    let a = forecast(&items, 0, NOW, 3, 42, None);
    let b = forecast(&items, 0, NOW, 3, 42, None);
    assert_eq!(a, b);
    assert!(a.converging);
    assert_eq!(a.open, 16);
    assert!((a.burn - 2.0).abs() < 1e-9);
    // Every day burns two net, so sixteen open items clear on day eight, 1970-04-19.
    assert_eq!(a.p50.as_deref(), Some("1970-04-19"));
    assert_eq!(a.p85.as_deref(), Some("1970-04-19"));
}

#[test]
fn test_forecast_marks_a_target_before_the_p85_date_late() {
    let items = shrinking();
    assert_eq!(
        forecast(&items, 0, NOW, 3, 1, Some("1970-04-18")).late,
        Some(true)
    );
    assert_eq!(
        forecast(&items, 0, NOW, 3, 1, Some("1970-04-19")).late,
        Some(false)
    );
}

#[test]
fn test_forecast_counts_only_items_up_to_the_release() {
    let mut items = shrinking();
    items[0].release = Some(2);
    assert_eq!(forecast(&items, 0, NOW, 3, 1, None).open, 15);
    assert_eq!(forecast(&items, 2, NOW, 3, 1, None).open, 16);
}

#[test]
fn test_forecast_line_names_the_p50_date_and_a_late_target() {
    let f = Forecast {
        open: 16,
        burn: 2.0,
        converging: true,
        p50: Some("2026-10-20".into()),
        p85: Some("2026-11-02".into()),
        target: Some("2026-10-30".into()),
        late: Some(true),
    };
    assert_eq!(
        f.line(),
        "16 open, clear by P50 2026-10-20, P85 2026-11-02; late for the target 2026-10-30"
    );
    let on_time = Forecast {
        target: Some("2026-11-05".into()),
        late: Some(false),
        ..f
    };
    assert_eq!(
        on_time.line(),
        "16 open, clear by P50 2026-10-20, P85 2026-11-02; on course for the target 2026-11-05"
    );
}

#[test]
fn test_forecast_line_says_not_converging_for_a_zero_burn() {
    let f = forecast(
        &(0..4)
            .map(|n| subject(&format!("T{n}"), "ready", 10, None, None))
            .collect::<Vec<_>>(),
        0,
        NOW,
        3,
        7,
        None,
    );
    assert_eq!(
        f.line(),
        "4 open, not converging: closes do not outrun opens"
    );
}

#[test]
fn test_forecast_line_of_a_cleared_release_is_empty() {
    let f = forecast(&[], 0, NOW, 3, 7, None);
    assert_eq!(f.line(), "nothing open");
}

fn modelled(model: Option<&str>, tokens: (i64, i64)) -> Span {
    let mut s = run("T1", Some("claude"), 10, tokens, None);
    s.model = model.map(str::to_string);
    s
}

#[test]
fn test_cost_prices_tokens_by_model_per_million_and_leaves_unpriced_models_as_tokens() {
    let prices = prices_of("alpha=3:15").unwrap();
    let spans = [
        modelled(Some("alpha"), (1_000_000, 0)),
        modelled(Some("alpha"), (0, 500_000)),
        modelled(Some("beta"), (1_000_000, 1_000_000)),
        modelled(None, (5, 5)),
    ];
    let got = cost(&spans, NOW, &prices);
    assert!((got.money.unwrap() - 10.5).abs() < 1e-9);
    assert_eq!((got.attempts, got.attempts_priced), (4, 2));
    assert_eq!(got.tokens_in, 2_000_005);
    let none = cost(&spans[2..], NOW, &prices);
    assert_eq!((none.money, none.attempts_priced), (None, 0));
}

#[test]
fn test_prices_refuse_a_malformed_entry() {
    assert!(prices_of("alpha=3").is_err());
    assert!(prices_of("alpha=x:1").is_err());
    assert!(prices_of("alpha=-1:1").is_err());
    assert!(prices_of("").unwrap().is_empty());
}

fn in_release(mut s: Subject, release: usize) -> Subject {
    s.release = Some(release);
    s
}

fn slots() -> Vec<Unshipped> {
    vec![
        Unshipped {
            position: 0,
            name: "1.0".into(),
            target: Some("2030-01-01".into()),
        },
        Unshipped {
            position: 1,
            name: "1.1".into(),
            target: None,
        },
    ]
}

#[test]
fn test_release_rows_count_each_word_apart_with_held_later_and_pace() {
    let items = vec![
        subject("T1", "done", 90, Some(91), Some(95)),
        subject("T2", "ready", 98, None, None),
        subject("T3", "blocked", 99, None, None),
        subject("T4", "in progress", 99, Some(99), None),
        subject("T5", "parked", 99, None, None),
        subject("T6", "dropped", 97, None, Some(98)),
        subject("A1", "under way", 99, None, None),
        in_release(subject("T7", "ready", 99, None, None), 1),
    ];
    let held = HashSet::from(["T2".to_string()]);
    let rows = release_rows(&items, &slots(), &held, NOW, WINDOW, 7);
    assert_eq!(rows.len(), 2);
    let first = &rows[0];
    assert_eq!(first.name, "1.0");
    assert_eq!(
        (
            first.closed,
            first.open,
            first.ready,
            first.in_progress,
            first.under_way,
            first.waiting_owner,
            first.blocked,
            first.held_later
        ),
        (1, 5, 1, 1, 1, 1, 1, 1)
    );
    assert!((first.forecast.burn - first.pace).abs() < f64::EPSILON);
    assert_eq!(first.forecast.target.as_deref(), Some("2030-01-01"));
    let second = &rows[1];
    assert_eq!((second.closed, second.open, second.held_later), (0, 1, 0));
}

#[test]
fn test_a_plan_with_one_done_and_one_dropped_member_reads_both_apart() {
    let members = [
        Subject::member("T1", "done", false),
        Subject::member("T2", "dropped", false),
        Subject::member("T3", "in progress", true),
    ];
    let got = progress(&members[..2]);
    assert_eq!(
        (got.done, got.dropped, got.total, got.counted),
        (1, 1, 2, 1)
    );
    assert!(got.settled());
    let all = progress(&members);
    assert_eq!(
        (all.live, all.remaining, all.open["in progress"]),
        (1, 1, 1)
    );
    assert!(!all.settled());
}

#[test]
fn test_area_progress_leaves_holders_and_dropped_items_out_in_position_order() {
    use crate::area::{Area, Listed};
    let area = |name: &str, position: i64| Area {
        name: name.into(),
        description: None,
        position,
        priority: None,
        history: false,
    };
    let areas = Listed {
        rows: vec![(1, area("lanterns", 2)), (2, area("kites", 1))],
    };
    let mut items = vec![
        Subject::member("T1", "done", false),
        Subject::member("T2", "ready", true),
        Subject::member("T3", "dropped", false),
        Subject::member("A1", "under way", false),
        Subject::member("T4", "ready", false),
    ];
    for (i, a) in items.iter_mut().zip([1, 1, 1, 1, 2]) {
        i.area = Some(a);
    }
    items[3].holder = true;
    let got: Vec<(String, u64, u64, u64)> = area_progress(&areas, &items)
        .into_iter()
        .map(|a| (a.name, a.open, a.done, a.live))
        .collect();
    assert_eq!(
        got,
        [("kites".into(), 1, 0, 0), ("lanterns".into(), 1, 1, 1)]
    );
}

#[test]
fn test_daily_counts_opened_done_and_dropped_per_day_with_zeros() {
    let items = [
        subject("T1", "done", 98, None, Some(99)),
        subject("T2", "dropped", 99, None, Some(99)),
        subject("T3", "ready", 99, None, None),
    ];
    let got: Vec<(u64, u64, u64)> = daily(&items, NOW, 3)
        .into_iter()
        .map(|d| (d.opened, d.closed, d.dropped))
        .collect();
    assert_eq!(got, [(1, 0, 0), (2, 1, 1), (0, 0, 0)]);
}

/// Every source file under `dir` with the extension, as a path and its text.
fn sources(dir: &std::path::Path, ext: &str, out: &mut Vec<(std::path::PathBuf, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n != "node_modules" && n != "target")
            {
                sources(&path, ext, out);
            }
        } else if path.extension().is_some_and(|e| e == ext) {
            let text = std::fs::read_to_string(&path).unwrap();
            out.push((path, text));
        }
    }
}

#[test]
fn test_progress_and_throughput_are_worked_out_only_in_the_metrics_module() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    for krate in std::fs::read_dir(&root).unwrap() {
        let src = krate.unwrap().path().join("src");
        if src.is_dir() {
            sources(&src, "rs", &mut found);
        }
    }
    let mut owners: Vec<String> = Vec::new();
    for (path, text) in &found {
        if path.ends_with("docket-core/src/metrics.rs") {
            continue;
        }
        for line in text.lines().map(str::trim_start) {
            let name = line
                .trim_start_matches("pub(crate) ")
                .trim_start_matches("pub ")
                .strip_prefix("fn ");
            if let Some(name) = name.and_then(|n| n.split(['(', '<']).next())
                && matches!(name, "daily" | "progress" | "area_progress")
            {
                owners.push(format!("{} defines fn {name}", path.display()));
            }
        }
    }
    let mut web = Vec::new();
    sources(&root.join("docket-web/src/lib"), "ts", &mut web);
    for (path, text) in &web {
        for line in text.lines().map(str::trim_start) {
            let name = line
                .trim_start_matches("export ")
                .strip_prefix("function ")
                .and_then(|n| n.split(['(', '<']).next());
            if matches!(
                name,
                Some("daily" | "progress" | "areaCounts" | "areaProgress")
            ) {
                owners.push(format!("{} defines {line}", path.display()));
            }
        }
    }
    assert!(owners.is_empty(), "{owners:#?}");
}
