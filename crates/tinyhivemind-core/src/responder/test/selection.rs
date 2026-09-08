//! Tests for `accept_selection`'s tolerant parsing of a model's raw output.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::*;

#[test]
fn accepts_canonical_id_case_wrappers_and_one_period() {
    let candidates = [SelectorCandidate {
        id: "Agent_A".into(),
        label: "A".into(),
        role: "R".into(),
        description: None,
    }];
    for output in [
        "agent_a",
        " AGENT_A. ",
        "'agent_a'",
        "\"agent_a\".",
        "`agent_a`",
    ] {
        assert_eq!(
            accept_selection(output, &candidates),
            Some("Agent_A".into())
        );
    }
}

#[test]
fn rejects_empty_prose_multiple_out_of_set_and_extra_punctuation() {
    let candidates = [
        SelectorCandidate {
            id: "alice".into(),
            label: "A".into(),
            role: "R".into(),
            description: None,
        },
        SelectorCandidate {
            id: "bob".into(),
            label: "B".into(),
            role: "R".into(),
            description: None,
        },
    ];
    for output in [
        "",
        "alice because",
        "alice bob",
        "cara",
        "alice..",
        "'alice\"",
        "\"alice\" extra",
    ] {
        assert_eq!(accept_selection(output, &candidates), None, "{output}");
    }
}
