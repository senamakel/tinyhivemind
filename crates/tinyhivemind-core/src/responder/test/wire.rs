//! Tests pinning the serde wire form of every responder payload type.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::*;
use super::support::{assert_missing_field, assert_required_fields, assert_wire};

#[test]
fn responder_payload_wire_forms_are_exact_and_round_trip() {
    let candidate = SelectorCandidate {
        id: "alice".into(),
        label: "Alice".into(),
        role: "Reviewer".into(),
        description: Some("Checks changes".into()),
    };
    let candidate_value = serde_json::json!({
        "id":"alice", "label":"Alice", "role":"Reviewer",
        "description":"Checks changes"
    });
    assert_wire(&candidate, candidate_value.clone());

    let request = ResponderRequest {
        message: "Please review".into(),
        chat: Some("eng".into()),
        mentions: Vec::new(),
        orchestrator_id: "orch".into(),
        selection_policy: SelectionPolicy::Allowed,
    };
    let request_value = serde_json::json!({
        "message":"Please review", "chat":"eng", "mentions":[],
        "orchestrator_id":"orch", "selection_policy":"allowed"
    });
    assert_wire(&request, request_value);

    let selection = SelectionRequest {
        message: "Please review".into(),
        desk_id: "eng".into(),
        candidates: vec![candidate.clone()],
    };
    let selection_value = serde_json::json!({
        "message":"Please review", "desk_id":"eng", "candidates":[candidate_value]
    });
    assert_wire(&selection, selection_value.clone());

    let decision = ResponderDecision {
        responder_id: "alice".into(),
        rung: ResponderRung::AutoSelection,
        disposition: SelectionDisposition::Selected,
    };
    let decision_value = serde_json::json!({
        "responder_id":"alice", "rung":"auto_selection", "disposition":"selected"
    });
    assert_wire(&decision, decision_value.clone());

    assert_wire(
        &ResponderPlan::Decided {
            decision: decision.clone(),
        },
        serde_json::json!({"kind":"decided", "decision":decision_value.clone()}),
    );
    assert_wire(
        &ResponderPlan::Select {
            request: selection,
            fallback: decision,
        },
        serde_json::json!({
            "kind":"select", "request":selection_value, "fallback":decision_value
        }),
    );
}

#[test]
fn responder_enum_wire_values_are_exact_and_round_trip() {
    for (value, expected) in [
        (SelectionPolicy::Allowed, "allowed"),
        (SelectionPolicy::Disabled, "disabled"),
    ] {
        assert_wire(&value, serde_json::json!(expected));
    }
    for (value, expected) in [
        (ResponderRung::ExplicitMention, "explicit_mention"),
        (ResponderRung::AutoSelection, "auto_selection"),
        (ResponderRung::DeskDefault, "desk_default"),
        (ResponderRung::DirectAgent, "direct_agent"),
        (ResponderRung::Orchestrator, "orchestrator"),
    ] {
        assert_wire(&value, serde_json::json!(expected));
    }
    for (value, expected) in [
        (SelectionDisposition::NotApplicable, "not_applicable"),
        (SelectionDisposition::Selected, "selected"),
        (SelectionDisposition::Disabled, "disabled"),
        (SelectionDisposition::Unavailable, "unavailable"),
        (SelectionDisposition::InvalidOutput, "invalid_output"),
    ] {
        assert_wire(&value, serde_json::json!(expected));
    }
}

#[test]
fn responder_option_fields_are_required_and_accept_null() {
    let candidate = serde_json::json!({
        "id":"alice", "label":"Alice", "role":"Reviewer", "description":null
    });
    assert_eq!(
        serde_json::from_value::<SelectorCandidate>(candidate.clone())
            .unwrap()
            .description,
        None
    );
    assert_missing_field::<SelectorCandidate>(candidate, "description");

    let request = serde_json::json!({
        "message":"Please review", "chat":null, "mentions":[],
        "orchestrator_id":"orch", "selection_policy":"allowed"
    });
    assert_eq!(
        serde_json::from_value::<ResponderRequest>(request.clone())
            .unwrap()
            .chat,
        None
    );
    assert_missing_field::<ResponderRequest>(request, "chat");
}

#[test]
fn every_responder_payload_wire_field_is_required() {
    let candidate = serde_json::json!({
        "id":"alice", "label":"Alice", "role":"Reviewer", "description":null
    });
    assert_required_fields::<SelectorCandidate>(
        &candidate,
        &["id", "label", "role", "description"],
    );

    let request = serde_json::json!({
        "message":"Please review", "chat":null, "mentions":[],
        "orchestrator_id":"orch", "selection_policy":"allowed"
    });
    assert_required_fields::<ResponderRequest>(
        &request,
        &[
            "message",
            "chat",
            "mentions",
            "orchestrator_id",
            "selection_policy",
        ],
    );

    let selection = serde_json::json!({
        "message":"Please review", "desk_id":"eng", "candidates":[candidate]
    });
    assert_required_fields::<SelectionRequest>(&selection, &["message", "desk_id", "candidates"]);

    let decision = serde_json::json!({
        "responder_id":"alice", "rung":"desk_default", "disposition":"unavailable"
    });
    assert_required_fields::<ResponderDecision>(
        &decision,
        &["responder_id", "rung", "disposition"],
    );
    assert_required_fields::<ResponderPlan>(
        &serde_json::json!({"kind":"decided", "decision":decision.clone()}),
        &["kind", "decision"],
    );
    assert_required_fields::<ResponderPlan>(
        &serde_json::json!({
            "kind":"select", "request":selection, "fallback":decision
        }),
        &["kind", "request", "fallback"],
    );
}
