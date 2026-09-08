//! Tests for `responder_plan`'s ladder routing: explicit mentions, desk
//! defaults, direct-agent chats, the orchestrator fallback, and how selector
//! candidate detail is folded in or rejected along the way.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::*;
use super::support::{decision, desk, member, mention, request};
use crate::{
    desk::{DeskOrder, DeskSet, ResponderMode},
    error::Error,
    mention::{Mention, MentionTarget},
    roster::Roster,
};

#[test]
fn first_reading_order_direct_mention_is_the_only_decision() {
    let members = [
        member("orch", None),
        member("alice", None),
        member("bob", None),
    ];
    let desks_data = [desk("eng", "Engineering", &["bob"], ResponderMode::Lead)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&desks_data, &[], &[], &[], &[]);
    let mut input = request(Some("eng"));
    input.mentions = vec![
        mention("bob", 20, false),
        Mention {
            target: MentionTarget::Everyone,
            text: "@everyone".into(),
            offset: 0,
            quiet: false,
        },
        mention("alice", 10, false),
    ];
    assert_eq!(
        decision(responder_plan(&input, &roster, &desks, &[]).unwrap()),
        ResponderDecision {
            responder_id: "alice".into(),
            rung: ResponderRung::ExplicitMention,
            disposition: SelectionDisposition::NotApplicable,
        }
    );
}

#[test]
fn quiet_inactive_and_non_agent_mentions_do_not_select() {
    let members = [
        member("orch", None),
        member("alice", None),
        member("bob", None),
    ];
    let retired = ["alice".to_owned()];
    let roster = Roster::new(&members, &[], &retired);
    let desks = DeskSet::new(&[], &[], &[], &[], &retired);
    let mut input = request(None);
    input.mentions = vec![
        mention("bob", 0, true),
        mention("alice", 2, false),
        Mention {
            target: MentionTarget::Desk { id: "eng".into() },
            text: "@#eng".into(),
            offset: 4,
            quiet: false,
        },
        Mention {
            target: MentionTarget::Person {
                id: "person".into(),
            },
            text: "@person".into(),
            offset: 6,
            quiet: false,
        },
    ];
    assert_eq!(
        decision(responder_plan(&input, &roster, &desks, &[]).unwrap()).responder_id,
        "orch"
    );
}

#[test]
fn direct_mention_does_not_require_an_active_orchestrator() {
    let members = [member("alice", None)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    let mut input = request(None);
    input.mentions = vec![mention("alice", 0, false)];
    assert_eq!(
        decision(responder_plan(&input, &roster, &desks, &[]).unwrap()).responder_id,
        "alice"
    );
}

#[test]
fn general_unaddressed_and_unresolved_chats_use_orchestrator() {
    let members = [member("orch", None)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    for chat in [None, Some("main"), Some("General"), Some("unknown")] {
        let selected = decision(responder_plan(&request(chat), &roster, &desks, &[]).unwrap());
        assert_eq!(selected.rung, ResponderRung::Orchestrator);
    }
}

#[test]
fn reached_fallback_requires_an_active_orchestrator() {
    let members = [member("alice", None)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    assert_eq!(
        responder_plan(&request(None), &roster, &desks, &[]),
        Err(Error::NoActiveResponder {
            agent_id: "orch".into()
        })
    );
}

#[test]
fn lead_mode_uses_first_effective_active_member() {
    let members = [
        member("orch", None),
        member("alice", None),
        member("bob", None),
    ];
    let retired = ["alice".to_owned()];
    let records = [desk(
        "eng",
        "Engineering",
        &["missing", "alice", "bob"],
        ResponderMode::Lead,
    )];
    let roster = Roster::new(&members, &[], &retired);
    let desks = DeskSet::new(&records, &[], &[], &[], &retired);
    let selected = decision(responder_plan(&request(Some("eng")), &roster, &desks, &[]).unwrap());
    assert_eq!(selected.responder_id, "bob");
    assert_eq!(selected.rung, ResponderRung::DeskDefault);
}

#[test]
fn complete_desk_order_controls_the_default() {
    let members = [
        member("orch", None),
        member("alice", None),
        member("bob", None),
    ];
    let records = [desk(
        "eng",
        "Engineering",
        &["alice", "bob"],
        ResponderMode::Lead,
    )];
    let orders = [DeskOrder {
        desk_id: "eng".into(),
        ordered: vec!["bob".into(), "alice".into()],
    }];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&records, &[], &[], &orders, &[]);
    assert_eq!(
        decision(responder_plan(&request(Some("eng")), &roster, &desks, &[]).unwrap()).responder_id,
        "bob"
    );
}

#[test]
fn auto_with_no_effective_members_uses_orchestrator() {
    let members = [member("orch", None)];
    let records = [desk(
        "eng",
        "Engineering",
        &["missing"],
        ResponderMode::Auto,
    )];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&records, &[], &[], &[], &[]);
    assert_eq!(
        decision(responder_plan(&request(Some("eng")), &roster, &desks, &[]).unwrap()).rung,
        ResponderRung::Orchestrator
    );
}

#[test]
fn auto_with_one_effective_member_uses_desk_default() {
    let members = [member("orch", None), member("alice", None)];
    let records = [desk("eng", "Engineering", &["alice"], ResponderMode::Auto)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&records, &[], &[], &[], &[]);
    assert_eq!(
        decision(responder_plan(&request(Some("eng")), &roster, &desks, &[]).unwrap()).rung,
        ResponderRung::DeskDefault
    );
}

#[test]
fn auto_builds_ordered_clamped_candidates_and_synthesizes_missing_detail() {
    let members = [
        member("orch", None),
        member("alice", None),
        member("bob", None),
    ];
    let records = [desk(
        "eng",
        "Engineering",
        &["alice", "bob"],
        ResponderMode::Auto,
    )];
    let detail = [
        SelectorCandidate {
            id: "bob".into(),
            label: "Bob".into(),
            role: "Reviewer".into(),
            description: Some("Checks".into()),
        },
        SelectorCandidate {
            id: "extra".into(),
            label: "Extra".into(),
            role: "Nope".into(),
            description: None,
        },
    ];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&records, &[], &[], &[], &[]);
    let ResponderPlan::Select { request, fallback } =
        responder_plan(&request(Some("Engineering")), &roster, &desks, &detail).unwrap()
    else {
        panic!("expected selection")
    };
    assert_eq!(request.desk_id, "eng");
    assert_eq!(
        request
            .candidates
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["alice", "bob"]
    );
    assert_eq!(request.candidates[0].label, "alice");
    assert_eq!(request.candidates[0].role, "Teammate");
    assert_eq!(fallback.rung, ResponderRung::DeskDefault);
    assert_eq!(fallback.disposition, SelectionDisposition::Unavailable);
}

#[test]
fn disabled_auto_selection_uses_first_desk_member() {
    let members = [
        member("orch", None),
        member("alice", None),
        member("bob", None),
    ];
    let records = [desk(
        "eng",
        "Engineering",
        &["alice", "bob"],
        ResponderMode::Auto,
    )];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&records, &[], &[], &[], &[]);
    let mut input = request(Some("eng"));
    input.selection_policy = SelectionPolicy::Disabled;
    let selected = decision(responder_plan(&input, &roster, &desks, &[]).unwrap());
    assert_eq!(selected.responder_id, "alice");
    assert_eq!(selected.rung, ResponderRung::DeskDefault);
    assert_eq!(selected.disposition, SelectionDisposition::Disabled);
}

#[test]
fn bare_id_name_and_dm_identity_select_a_direct_agent() {
    let members = [member("orch", None), member("alice", Some("Alice A"))];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    for chat in ["alice", "Alice A", "dm:alice", "dm:Alice A"] {
        let selected =
            decision(responder_plan(&request(Some(chat)), &roster, &desks, &[]).unwrap());
        assert_eq!(selected.responder_id, "alice");
        assert_eq!(selected.rung, ResponderRung::DirectAgent);
    }
}

#[test]
fn desk_identity_outranks_a_direct_agent_collision() {
    let members = [
        member("orch", None),
        member("eng", None),
        member("alice", None),
    ];
    let records = [desk("eng", "Engineering", &["alice"], ResponderMode::Lead)];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&records, &[], &[], &[], &[]);
    assert_eq!(
        decision(responder_plan(&request(Some("eng")), &roster, &desks, &[]).unwrap()).responder_id,
        "alice"
    );
}

#[test]
fn ambiguous_agent_name_fails_closed_to_orchestrator() {
    let members = [
        member("orch", None),
        member("alice", Some("Sam")),
        member("bob", Some("Sam")),
    ];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    assert_eq!(
        decision(responder_plan(&request(Some("Sam")), &roster, &desks, &[]).unwrap()).responder_id,
        "orch"
    );
}

#[test]
fn ambiguous_desk_name_fails_closed_to_orchestrator() {
    let members = [
        member("orch", None),
        member("alice", None),
        member("bob", None),
    ];
    let records = [
        desk("eng", "Shared", &["alice"], ResponderMode::Lead),
        desk("ops", "Shared", &["bob"], ResponderMode::Lead),
    ];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&records, &[], &[], &[], &[]);
    assert_eq!(
        decision(responder_plan(&request(Some("Shared")), &roster, &desks, &[]).unwrap())
            .responder_id,
        "orch"
    );
}

#[test]
fn duplicate_extra_candidate_detail_is_ignored() {
    let members = [
        member("orch", None),
        member("alice", None),
        member("bob", None),
    ];
    let roster = Roster::new(&members, &[], &[]);
    let records = [desk(
        "eng",
        "Engineering",
        &["alice", "bob"],
        ResponderMode::Auto,
    )];
    let desks = DeskSet::new(&records, &[], &[], &[], &[]);
    let detail = SelectorCandidate {
        id: "extra".into(),
        label: "Extra".into(),
        role: "Teammate".into(),
        description: None,
    };
    assert!(matches!(
        responder_plan(
            &request(Some("eng")),
            &roster,
            &desks,
            &[detail.clone(), detail]
        ),
        Ok(ResponderPlan::Select { .. })
    ));
}

#[test]
fn duplicate_relevant_candidate_detail_errors_when_auto_enrichment_is_reached() {
    let members = [
        member("orch", None),
        member("alice", None),
        member("bob", None),
    ];
    let records = [desk(
        "eng",
        "Engineering",
        &["alice", "bob"],
        ResponderMode::Auto,
    )];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&records, &[], &[], &[], &[]);
    let detail = SelectorCandidate {
        id: "alice".into(),
        label: "Alice".into(),
        role: "Teammate".into(),
        description: None,
    };
    assert_eq!(
        responder_plan(
            &request(Some("eng")),
            &roster,
            &desks,
            &[detail.clone(), detail]
        ),
        Err(Error::DuplicateSelectorCandidate {
            agent_id: "alice".into()
        })
    );
}

#[test]
fn irrelevant_duplicate_details_do_not_affect_non_auto_routes() {
    let members = [
        member("orch", None),
        member("alice", None),
        member("bob", None),
    ];
    let lead_records = [desk("lead", "Lead", &["alice", "bob"], ResponderMode::Lead)];
    let one_records = [desk("solo", "Solo", &["alice"], ResponderMode::Auto)];
    let roster = Roster::new(&members, &[], &[]);
    let lead_desks = DeskSet::new(&lead_records, &[], &[], &[], &[]);
    let one_desks = DeskSet::new(&one_records, &[], &[], &[], &[]);
    let no_desks = DeskSet::new(&[], &[], &[], &[], &[]);
    let detail = SelectorCandidate {
        id: "alice".into(),
        label: "Alice".into(),
        role: "Teammate".into(),
        description: None,
    };
    let duplicates = [detail.clone(), detail];

    let mut mentioned = request(None);
    mentioned.mentions = vec![mention("alice", 0, false)];
    let cases = [
        (&mentioned, &no_desks, ResponderRung::ExplicitMention),
        (
            &request(Some("lead")),
            &lead_desks,
            ResponderRung::DeskDefault,
        ),
        (
            &request(Some("solo")),
            &one_desks,
            ResponderRung::DeskDefault,
        ),
        (&request(Some("bob")), &no_desks, ResponderRung::DirectAgent),
        (
            &request(Some("unknown")),
            &no_desks,
            ResponderRung::Orchestrator,
        ),
        (
            &request(Some("General")),
            &no_desks,
            ResponderRung::Orchestrator,
        ),
        (&request(None), &no_desks, ResponderRung::Orchestrator),
    ];
    for (input, desks, expected_rung) in cases {
        assert_eq!(
            decision(responder_plan(input, &roster, desks, &duplicates).unwrap()).rung,
            expected_rung
        );
    }

    let mut disabled = request(Some("eng"));
    disabled.selection_policy = SelectionPolicy::Disabled;
    let auto_records = [desk(
        "eng",
        "Engineering",
        &["alice", "bob"],
        ResponderMode::Auto,
    )];
    let auto_desks = DeskSet::new(&auto_records, &[], &[], &[], &[]);
    assert_eq!(
        decision(responder_plan(&disabled, &roster, &auto_desks, &duplicates).unwrap()).disposition,
        SelectionDisposition::Disabled
    );
}
