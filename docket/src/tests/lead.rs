use super::*;

fn held(lapsed: bool) -> LeadState {
    LeadState {
        project: "o/p".into(),
        lead: Some(Lead {
            project: "o/p".into(),
            host: "alpha".into(),
            session: "lead-1".into(),
            branch: Some("main".into()),
            since: "2026-10-02T10:00:00Z".into(),
            renewed_at: "2026-10-02T10:05:00Z".into(),
        }),
        lapsed,
        lapses_at: Some("2026-10-02T10:15:00Z".into()),
        lapse_minutes: 10,
        outcome: None,
        previous: None,
    }
}

#[test]
fn test_no_lead_says_how_to_take_it() {
    let none = LeadState {
        project: "o/p".into(),
        lapse_minutes: 10,
        ..LeadState::default()
    };
    assert_eq!(lead_text(&none), "no lead holds o/p: docket lead take\n");
}

#[test]
fn test_a_held_lead_names_its_holder_and_when_it_lapses() {
    assert_eq!(
        lead_text(&held(false)),
        "o/p is led by lead-1 on alpha, branch main, since 2026-10-02T10:00:00Z\n\
         last renewed 2026-10-02T10:05:00Z; it lapses at 2026-10-02T10:15:00Z unless renewed\n"
    );
}

#[test]
fn test_a_lapsed_lead_says_the_next_take_takes_it_over() {
    assert!(
        lead_text(&held(true)).ends_with(
            "it lapsed at 2026-10-02T10:15:00Z: the next docket lead take takes it over\n"
        )
    );
}

#[test]
fn test_an_act_names_what_it_did_first() {
    let mut took = held(false);
    took.outcome = Some("took over".into());
    took.previous = Some(Lead {
        host: "beta".into(),
        session: "lead-7".into(),
        ..Lead::default()
    });
    assert!(
        lead_text(&took).starts_with("took over the lead of o/p from lead-7 on beta\n"),
        "{}",
        lead_text(&took)
    );
    let gave = LeadState {
        project: "o/p".into(),
        outcome: Some("gave".into()),
        ..LeadState::default()
    };
    assert_eq!(
        lead_text(&gave),
        "gave back the lead of o/p\nno lead holds o/p: docket lead take\n"
    );
}
