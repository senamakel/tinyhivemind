//! Unit tests for caller-owned watermark sharing.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use crate::{
    Conversation, Error, LogMessage, SessionAuthor, SessionFuture, SessionPage, SourceError,
};
use std::{
    collections::{BTreeSet, VecDeque},
    io,
    sync::{Arc, Mutex},
};
use tinyhivemind_core::aside::Audience;
use tinyhivemind_core::aside::Viewer;

#[derive(Debug)]
struct FakeLog {
    pages: Mutex<VecDeque<std::result::Result<SessionPage, SourceError>>>,
    calls: Arc<Mutex<usize>>,
}

impl FakeLog {
    fn new(pages: Vec<SessionPage>) -> Self {
        Self {
            pages: Mutex::new(pages.into_iter().map(Ok).collect()),
            calls: Arc::default(),
        }
    }

    fn failing() -> Self {
        Self {
            pages: Mutex::new(VecDeque::from([Err(
                Box::new(io::Error::other("offline")) as SourceError
            )])),
            calls: Arc::default(),
        }
    }

    fn calls(&self) -> usize {
        *self.calls.lock().expect("calls lock")
    }
}

impl SessionLog for FakeLog {
    fn read_before(&self, _: Option<Sequence>, _: usize) -> SessionFuture<'_> {
        *self.calls.lock().expect("calls lock") += 1;
        Box::pin(async move {
            self.pages
                .lock()
                .expect("pages lock")
                .pop_front()
                .unwrap_or_else(|| Ok(SessionPage::default()))
        })
    }
}

fn conversation(id: &str, name: &str, root: Option<u64>) -> Conversation {
    Conversation {
        desk_id: id.into(),
        desk_name: name.into(),
        thread_root: root.map(Sequence),
    }
}

fn engineering() -> Conversation {
    conversation("engineering", "Engineering", None)
}

fn raw(sequence: u64, chat: Option<&str>, parent: Option<u64>, content: &str) -> LogMessage {
    LogMessage {
        sequence: Sequence(sequence),
        chat_id: chat.map(str::to_owned),
        parent: parent.map(Sequence),
        author: SessionAuthor::Agent {
            id: format!("agent-{sequence}"),
            label: format!("Agent {sequence}"),
        },
        content: content.into(),
        audience: Audience::Desk,
    }
}

fn page(messages: Vec<LogMessage>, next: Option<u64>) -> SessionPage {
    SessionPage {
        messages,
        next_before: next.map(Sequence),
    }
}

fn state(watermark: u64) -> SharingState {
    initialized_state(engineering(), Sequence(watermark))
}

async fn plan(log: &FakeLog, state: &SharingState, before: u64) -> Result<SharingPlan> {
    let desired = engineering();
    let current = engineering();
    prepare_delta(
        log,
        &SharingQuery {
            desired_conversation: &desired,
            current_conversation: &current,
            state,
            before: Sequence(before),
            viewer: &Viewer::Operator,
        },
    )
    .await
}

fn delta(plan: SharingPlan) -> SessionDelta {
    match plan {
        SharingPlan::Delta(delta) => delta,
        SharingPlan::Reinitialize { reason } => panic!("unexpected reinitialize: {reason:?}"),
    }
}

#[test]
fn sharing_values_pin_deterministic_wire_shapes() {
    let mut present = BTreeSet::new();
    present.insert(Sequence(12));
    present.insert(Sequence(11));
    let state = SharingState {
        conversation: engineering(),
        watermark: Sequence(10),
        present_above_watermark: present,
    };
    assert_eq!(
        serde_json::to_value(&state).expect("serializes"),
        serde_json::json!({
            "conversation":{"desk_id":"engineering","desk_name":"Engineering","thread_root":null},
            "watermark":10,
            "present_above_watermark":[11,12]
        })
    );
    assert_eq!(
        serde_json::from_value::<SharingState>(serde_json::to_value(&state).expect("serializes"))
            .expect("deserializes"),
        state
    );
    assert_eq!(
        serde_json::to_value(ReinitializeReason::GapTooLarge).expect("serializes"),
        serde_json::json!("gap_too_large")
    );
    let delta = SessionDelta {
        messages: vec![crate::SessionMessage {
            sequence: Sequence(11),
            author: SessionAuthor::Operator,
            content: "new".into(),
            audience: Audience::Desk,
            elided: None,
        }],
        next_state: state.clone(),
    };
    assert_eq!(
        serde_json::to_value(&delta).expect("serializes"),
        serde_json::json!({
            "messages":[{"sequence":11,"author":{"type":"operator"},"content":"new","audience":{"kind":"desk"},"elided":null}],
            "next_state":{
                "conversation":{"desk_id":"engineering","desk_name":"Engineering","thread_root":null},
                "watermark":10,
                "present_above_watermark":[11,12]
            }
        })
    );
    let delta_plan = SharingPlan::Delta(delta);
    assert_eq!(
        serde_json::to_value(&delta_plan).expect("serializes"),
        serde_json::json!({
            "type":"delta",
            "messages":[{"sequence":11,"author":{"type":"operator"},"content":"new","audience":{"kind":"desk"},"elided":null}],
            "next_state":{
                "conversation":{"desk_id":"engineering","desk_name":"Engineering","thread_root":null},
                "watermark":10,
                "present_above_watermark":[11,12]
            }
        })
    );
    let plan = SharingPlan::Reinitialize {
        reason: ReinitializeReason::WatermarkUnavailable,
    };
    assert_eq!(
        serde_json::to_value(&plan).expect("serializes"),
        serde_json::json!({"type":"reinitialize","reason":"watermark_unavailable"})
    );
    assert_eq!(
        serde_json::from_value::<SharingPlan>(serde_json::to_value(&plan).expect("serializes"))
            .expect("deserializes"),
        plan
    );
}

#[test]
fn initialized_state_starts_empty_at_the_accepted_trigger() {
    let state = initialized_state(engineering(), Sequence(10));
    assert_eq!(state.watermark, Sequence(10));
    assert!(state.present_above_watermark.is_empty());
}

#[test]
fn note_present_ignores_old_rows_and_is_idempotent_for_future_rows() {
    let mut state = state(10);
    note_present(&mut state, Sequence(9)).expect("old no-op");
    note_present(&mut state, Sequence(10)).expect("watermark no-op");
    note_present(&mut state, Sequence(11)).expect("future insert");
    note_present(&mut state, Sequence(11)).expect("future idempotent");
    assert_eq!(
        state.present_above_watermark,
        BTreeSet::from([Sequence(11)])
    );
    assert_eq!(state.watermark, Sequence(10));
}

#[test]
fn note_present_overflow_is_typed_and_atomic() {
    let mut state = state(10);
    for sequence in 11..=10 + PRESENT_SET_LIMIT as u64 {
        note_present(&mut state, Sequence(sequence)).expect("within bound");
    }
    let before = state.clone();
    assert!(matches!(
        note_present(&mut state, Sequence(1000)),
        Err(Error::PresentSetOverflow {
            limit: 64,
            sequence: Sequence(1000)
        })
    ));
    assert_eq!(state, before);
}

#[test]
fn note_present_accepts_a_duplicate_when_the_present_set_is_full() {
    let mut state = state(10);
    for sequence in 11..=10 + PRESENT_SET_LIMIT as u64 {
        note_present(&mut state, Sequence(sequence)).expect("within bound");
    }
    let before = state.clone();
    note_present(&mut state, Sequence(11)).expect("duplicate remains idempotent");
    assert_eq!(state, before);
}

#[test]
fn note_present_rejects_an_already_oversized_manual_state_atomically() {
    let mut state = state(10);
    state.present_above_watermark = (11..=11 + PRESENT_SET_LIMIT as u64).map(Sequence).collect();
    let before = state.clone();
    assert!(matches!(
        note_present(&mut state, Sequence(11)),
        Err(Error::PresentSetTooLarge {
            limit: 64,
            actual: 65
        })
    ));
    assert_eq!(state, before);
}

#[test]
fn sharing_state_deserialization_rejects_an_oversized_present_set() {
    let mut value = serde_json::to_value(state(10)).expect("serializes");
    value["present_above_watermark"] =
        serde_json::json!((11..=11 + PRESENT_SET_LIMIT as u64).collect::<Vec<_>>());
    let error = serde_json::from_value::<SharingState>(value).expect_err("rejects 65 entries");
    assert!(error.to_string().contains("present set has 65 entries"));
}

#[tokio::test]
async fn prepare_delta_rejects_an_oversized_manual_state_without_reading() {
    let log = FakeLog::new(Vec::new());
    let mut oversized = state(10);
    oversized.present_above_watermark =
        (11..=11 + PRESENT_SET_LIMIT as u64).map(Sequence).collect();
    assert!(matches!(
        plan(&log, &oversized, 11).await,
        Err(Error::PresentSetTooLarge {
            limit: 64,
            actual: 65
        })
    ));
    assert_eq!(log.calls(), 0);
}

#[test]
fn conversation_equivalence_accepts_general_aliases_and_exact_named_identity() {
    assert!(
        conversation("general", "General", None)
            .equivalent_to(&conversation("main", "Anything", None))
    );
    assert!(engineering().equivalent_to(&conversation("engineering", "Renamed", None)));
    assert!(!engineering().equivalent_to(&conversation("engineering-2", "Engineering 2", None)));
    assert!(!engineering().equivalent_to(&conversation("engineering-2", "Engineering", None)));
    assert!(!engineering().equivalent_to(&conversation("engineering", "Engineering", Some(1))));
}

#[tokio::test]
async fn changed_current_or_stored_conversation_reinitializes_without_reading() {
    let log = FakeLog::new(Vec::new());
    let desired = engineering();
    let current = conversation("sales", "Sales", None);
    let state = state(10);
    let result = prepare_delta(
        &log,
        &SharingQuery {
            desired_conversation: &desired,
            current_conversation: &current,
            state: &state,
            before: Sequence(11),
            viewer: &Viewer::Operator,
        },
    )
    .await
    .expect("plans");
    assert_eq!(
        result,
        SharingPlan::Reinitialize {
            reason: ReinitializeReason::ConversationChanged
        }
    );
    assert_eq!(log.calls(), 0);

    let mut mismatched = state;
    mismatched.conversation = current;
    assert_eq!(
        plan(&log, &mismatched, 11).await.expect("plans"),
        SharingPlan::Reinitialize {
            reason: ReinitializeReason::ConversationChanged
        }
    );
}

#[tokio::test]
async fn regressing_bound_is_typed_and_performs_no_read() {
    let log = FakeLog::new(Vec::new());
    assert!(matches!(
        plan(&log, &state(10), 9).await,
        Err(Error::WatermarkRegression {
            before: Sequence(9),
            watermark: Sequence(10)
        })
    ));
    assert_eq!(log.calls(), 0);
}

#[tokio::test]
async fn equal_bound_is_empty_and_keeps_state_without_reading() {
    let log = FakeLog::new(Vec::new());
    let state = state(10);
    let result = delta(plan(&log, &state, 10).await.expect("plans"));
    assert!(result.messages.is_empty());
    assert_eq!(result.next_state, state);
    assert_eq!(log.calls(), 0);
}

#[tokio::test]
async fn interleaved_local_and_peer_rows_are_attributed_and_chronological() {
    let log = FakeLog::new(vec![page(
        vec![
            raw(20, Some("engineering"), None, "local"),
            raw(19, Some("Engineering"), None, "peer"),
            raw(18, Some("engineering"), None, "watermark"),
        ],
        None,
    )]);
    let result = delta(plan(&log, &state(18), 21).await.expect("plans"));
    assert_eq!(
        result
            .messages
            .iter()
            .map(|row| row.sequence)
            .collect::<Vec<_>>(),
        vec![Sequence(19), Sequence(20)]
    );
    assert!(matches!(
        result.messages[0].author,
        SessionAuthor::Agent { ref id, .. } if id == "agent-19"
    ));
    assert_eq!(result.next_state.watermark, Sequence(21));
}

#[tokio::test]
async fn exclusive_before_and_watermark_rows_are_not_emitted() {
    let log = FakeLog::new(vec![page(
        vec![raw(10, Some("engineering"), None, "watermark")],
        None,
    )]);
    let result = delta(plan(&log, &state(10), 11).await.expect("plans"));
    assert!(result.messages.is_empty());
}

#[tokio::test]
async fn already_present_rows_are_omitted_and_later_concurrent_rows_are_retained() {
    let mut state = state(18);
    note_present(&mut state, Sequence(20)).expect("present");
    note_present(&mut state, Sequence(25)).expect("future present");
    let log = FakeLog::new(vec![page(
        vec![
            raw(20, Some("engineering"), None, "already"),
            raw(19, Some("engineering"), None, "new"),
            raw(18, Some("engineering"), None, "watermark"),
        ],
        None,
    )]);
    let result = delta(plan(&log, &state, 21).await.expect("plans"));
    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].sequence, Sequence(19));
    assert_eq!(
        result.next_state.present_above_watermark,
        BTreeSet::from([Sequence(25)])
    );
}

#[tokio::test]
async fn unrelated_desks_and_blank_rows_count_but_are_filtered() {
    let log = FakeLog::new(vec![page(
        vec![
            raw(13, Some("sales"), None, "other"),
            raw(12, Some("engineering"), None, "  "),
            raw(11, Some("engineering"), None, "kept"),
            raw(10, Some("engineering"), None, "watermark"),
        ],
        None,
    )]);
    let result = delta(plan(&log, &state(10), 14).await.expect("plans"));
    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].content, "kept");
}

#[tokio::test]
async fn general_aliases_share_one_delta() {
    let desired = conversation("general", "General", None);
    let current = conversation("main", "Anything", None);
    let state = initialized_state(conversation("", "GENERAL", None), Sequence(10));
    let log = FakeLog::new(vec![page(
        vec![
            raw(11, None, None, "general"),
            raw(10, Some("general"), None, "watermark"),
        ],
        None,
    )]);
    let result = prepare_delta(
        &log,
        &SharingQuery {
            desired_conversation: &desired,
            current_conversation: &current,
            state: &state,
            before: Sequence(12),
            viewer: &Viewer::Operator,
        },
    )
    .await
    .expect("plans");
    assert_eq!(delta(result).messages.len(), 1);
}

#[tokio::test]
async fn channels_and_exact_threads_never_mix() {
    let thread = conversation("engineering", "Engineering", Some(10));
    let state = initialized_state(thread.clone(), Sequence(10));
    let log = FakeLog::new(vec![page(
        vec![
            raw(14, Some("engineering"), Some(11), "other thread"),
            raw(13, Some("engineering"), None, "channel"),
            raw(12, Some("engineering"), Some(10), "our thread"),
            raw(10, Some("engineering"), None, "root"),
        ],
        None,
    )]);
    let result = prepare_delta(
        &log,
        &SharingQuery {
            desired_conversation: &thread,
            current_conversation: &thread,
            state: &state,
            before: Sequence(15),
            viewer: &Viewer::Operator,
        },
    )
    .await
    .expect("plans");
    let result = delta(result);
    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].sequence, Sequence(12));
}

#[tokio::test]
async fn sparse_pages_continue_until_any_raw_row_crosses_watermark() {
    let log = FakeLog::new(vec![
        page(vec![raw(20, Some("sales"), None, "other")], Some(20)),
        page(vec![raw(15, Some("sales"), None, "other")], Some(15)),
        page(
            vec![
                raw(11, Some("engineering"), None, "new"),
                raw(10, Some("sales"), None, "cross"),
            ],
            None,
        ),
    ]);
    let result = delta(plan(&log, &state(10), 21).await.expect("plans"));
    assert_eq!(result.messages.len(), 1);
    assert_eq!(log.calls(), 3);
}

#[tokio::test]
async fn exhaustion_above_watermark_requests_reinitialization() {
    let log = FakeLog::new(vec![page(
        vec![raw(11, Some("engineering"), None, "new")],
        None,
    )]);
    assert_eq!(
        plan(&log, &state(10), 12).await.expect("plans"),
        SharingPlan::Reinitialize {
            reason: ReinitializeReason::WatermarkUnavailable
        }
    );
}

#[tokio::test]
async fn scan_cap_above_watermark_requests_gap_reinitialization() {
    let rows: Vec<_> = (1001..=3048)
        .rev()
        .map(|sequence| raw(sequence, Some("sales"), None, "other"))
        .collect();
    let mut pages = Vec::new();
    for chunk in rows.chunks(PAGE_SIZE) {
        pages.push(page(chunk.to_vec(), chunk.last().map(|row| row.sequence.0)));
    }
    let log = FakeLog::new(pages);
    assert_eq!(
        plan(&log, &state(10), 3049).await.expect("plans"),
        SharingPlan::Reinitialize {
            reason: ReinitializeReason::GapTooLarge
        }
    );
}

#[tokio::test]
async fn read_errors_propagate_without_mutating_input() {
    let log = FakeLog::failing();
    let state = state(10);
    let before = state.clone();
    assert!(matches!(
        plan(&log, &state, 12).await,
        Err(Error::Read { .. })
    ));
    assert_eq!(state, before);
}

#[tokio::test]
async fn malformed_pages_reuse_p4_validation() {
    let log = FakeLog::new(vec![page(
        vec![
            raw(11, Some("engineering"), None, "a"),
            raw(12, Some("engineering"), None, "b"),
        ],
        None,
    )]);
    assert!(matches!(
        plan(&log, &state(10), 13).await,
        Err(Error::PageNotDescending { .. })
    ));
}

#[tokio::test]
async fn every_p4_page_contract_failure_propagates_from_delta_planning() {
    let stored = state(1);

    let out_of_range = FakeLog::new(vec![page(
        vec![raw(12, Some("engineering"), None, "invalid")],
        None,
    )]);
    assert!(matches!(
        plan(&out_of_range, &stored, 12).await,
        Err(Error::PageOutOfRange { .. })
    ));

    let duplicate = FakeLog::new(vec![
        page(vec![raw(11, Some("engineering"), None, "first")], Some(11)),
        page(vec![raw(11, Some("engineering"), None, "again")], None),
    ]);
    assert!(matches!(
        plan(&duplicate, &stored, 12).await,
        Err(Error::DuplicateSequence { .. })
    ));

    let empty_cursor = FakeLog::new(vec![page(Vec::new(), Some(11))]);
    assert!(matches!(
        plan(&empty_cursor, &stored, 12).await,
        Err(Error::EmptyPageCursor { .. })
    ));

    let stalled = FakeLog::new(vec![page(
        vec![raw(11, Some("engineering"), None, "row")],
        Some(12),
    )]);
    assert!(matches!(
        plan(&stalled, &stored, 12).await,
        Err(Error::CursorDidNotAdvance { .. })
    ));

    let after_oldest = FakeLog::new(vec![page(
        vec![
            raw(11, Some("engineering"), None, "newer"),
            raw(10, Some("engineering"), None, "older"),
        ],
        Some(11),
    )]);
    assert!(matches!(
        plan(&after_oldest, &stored, 12).await,
        Err(Error::CursorAfterOldest { .. })
    ));

    let too_many = (87..=599)
        .rev()
        .map(|sequence| raw(sequence, Some("engineering"), None, "row"))
        .collect();
    let oversized = FakeLog::new(vec![page(too_many, None)]);
    assert!(matches!(
        plan(&oversized, &stored, 600).await,
        Err(Error::PageTooLarge {
            requested: PAGE_SIZE,
            actual: 513
        })
    ));
}

#[tokio::test]
async fn retry_from_uncommitted_state_repeats_the_same_delta() {
    let pages = vec![page(
        vec![
            raw(11, Some("engineering"), None, "new"),
            raw(10, Some("engineering"), None, "old"),
        ],
        None,
    )];
    let state = state(10);
    let first = plan(&FakeLog::new(pages.clone()), &state, 12)
        .await
        .expect("first");
    let second = plan(&FakeLog::new(pages), &state, 12).await.expect("retry");
    assert_eq!(first, second);
    assert_eq!(state.watermark, Sequence(10));
}

#[tokio::test]
async fn simulated_compare_and_swap_commits_only_the_winning_next_state() {
    let pages = vec![page(
        vec![
            raw(11, Some("engineering"), None, "new"),
            raw(10, Some("engineering"), None, "old"),
        ],
        None,
    )];
    let stored = Arc::new(Mutex::new(state(10)));
    let snapshot = stored.lock().expect("state lock").clone();
    let proposed = delta(
        plan(&FakeLog::new(pages), &snapshot, 12)
            .await
            .expect("plans"),
    )
    .next_state;
    note_present(&mut stored.lock().expect("state lock"), Sequence(20)).expect("concurrent update");
    let mut guard = stored.lock().expect("state lock");
    let won = *guard == snapshot;
    if won {
        *guard = proposed;
    }
    assert!(!won);
    assert_eq!(guard.watermark, Sequence(10));
    assert!(guard.present_above_watermark.contains(&Sequence(20)));
}

// ---------------------------------------------------------------------------
// Private asides
// ---------------------------------------------------------------------------

fn said(sequence: u64, id: &str, content: &str) -> LogMessage {
    LogMessage {
        author: crate::SessionAuthor::Agent {
            id: id.into(),
            label: id.into(),
        },
        ..raw(sequence, Some("engineering"), None, content)
    }
}

fn aside_row(sequence: u64, id: &str, members: &[&str], content: &str) -> LogMessage {
    LogMessage {
        audience: Audience::Aside {
            members: members.iter().map(|member| (*member).to_owned()).collect(),
        },
        ..said(sequence, id, content)
    }
}

async fn plan_for(log: &FakeLog, state: &SharingState, before: u64, viewer: &Viewer) -> SharingPlan {
    let desired = engineering();
    let current = engineering();
    prepare_delta(
        log,
        &SharingQuery {
            desired_conversation: &desired,
            current_conversation: &current,
            state,
            before: Sequence(before),
            viewer,
        },
    )
    .await
    .expect("prepares")
}

fn tick_rows() -> Vec<LogMessage> {
    vec![
        said(13, "archivist", "in the open"),
        aside_row(12, "auditor", &["planner"], "and privately, in reply"),
        aside_row(11, "planner", &["auditor"], "privately"),
        // At the watermark, so the backward walk crosses it and the delta is
        // a delta rather than a request to re-seed.
        said(10, "planner", "already delivered"),
    ]
}

#[tokio::test]
async fn a_delta_applies_the_same_audience_rule_as_the_projection() {
    let log = FakeLog::new(vec![page(tick_rows(), None)]);
    let plan = plan_for(&log, &state(10), 14, &Viewer::Agent {
        id: "archivist".into(),
    })
    .await;
    let delta = delta(plan);
    // Two rows: one stub standing for the pair, and the desk row.
    assert_eq!(delta.messages.len(), 2);
    assert_eq!(delta.messages[0].readable(), None);
    assert_eq!(delta.messages[0].sequence, Sequence(11));
    assert_eq!(
        delta.messages[0]
            .elided
            .as_ref()
            .expect("a stub")
            .messages,
        2,
    );
    assert_eq!(delta.messages[1].readable(), Some("in the open"));
}

#[tokio::test]
async fn a_member_receives_its_own_aside_in_a_delta() {
    let log = FakeLog::new(vec![page(tick_rows(), None)]);
    let plan = plan_for(&log, &state(10), 14, &Viewer::Agent {
        id: "auditor".into(),
    })
    .await;
    let delta = delta(plan);
    assert_eq!(delta.messages.len(), 3);
    assert!(delta.messages.iter().all(|m| m.elided.is_none()));
}

#[tokio::test]
async fn a_reseed_never_hands_a_member_less_than_the_delta_did() {
    // The incremental path and the projection have to agree, or an agent that
    // re-seeds sees a different transcript from one that stayed incremental
    // and nothing heals the difference.
    for id in ["planner", "archivist"] {
        let viewer = Viewer::Agent { id: id.into() };

        let log = FakeLog::new(vec![page(tick_rows(), None)]);
        let incremental = delta(plan_for(&log, &state(10), 14, &viewer).await).messages;

        let log = FakeLog::new(vec![page(tick_rows(), None)]);
        let reseeded = crate::project_session(
            &log,
            &crate::SessionQuery {
                conversation: engineering(),
                before: Some(Sequence(14)),
                window: 30,
                viewer: viewer.clone(),
            },
        )
        .await
        .expect("projects");

        assert_eq!(
            incremental
                .iter()
                .map(|m| (m.sequence, m.readable().map(str::to_owned)))
                .collect::<Vec<_>>(),
            reseeded
                .iter()
                .map(|m| (m.sequence, m.readable().map(str::to_owned)))
                .collect::<Vec<_>>(),
            "{id} must read the same rows either way",
        );
    }
}

#[tokio::test]
async fn a_delta_over_rows_with_no_aside_is_the_same_for_every_viewer() {
    let rows = vec![
        said(12, "planner", "two"),
        said(11, "planner", "one"),
        said(10, "planner", "already delivered"),
    ];
    let baseline = {
        let log = FakeLog::new(vec![page(rows.clone(), None)]);
        delta(plan_for(&log, &state(10), 13, &Viewer::Operator).await).messages
    };
    for viewer in [
        Viewer::Agent {
            id: "anyone".into(),
        },
        Viewer::Person { id: "ada".into() },
    ] {
        let log = FakeLog::new(vec![page(rows.clone(), None)]);
        assert_eq!(
            delta(plan_for(&log, &state(10), 13, &viewer).await).messages,
            baseline
        );
    }
}
