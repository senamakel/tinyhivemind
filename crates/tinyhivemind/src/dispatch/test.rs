//! Unit tests for the one-call atomic mention-turn queue boundary.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use std::{
    collections::HashSet,
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tinyhivemind_core::{
    dispatch::{
        DispatchConversation, DispatchKey, MentionDispatchInput, MentionDispatchPolicy,
        MentionTurnRequest,
    },
    mention::{Mention, MentionTarget},
    roster::{Roster, RosterMember},
};
use tokio::sync::Barrier;

#[derive(Default)]
struct AtomicQueue {
    calls: AtomicUsize,
    fail_before_commit: AtomicBool,
    before_commit: Option<Arc<Barrier>>,
    durable: Mutex<DurableQueueState>,
}

#[derive(Default)]
struct DurableQueueState {
    keys: HashSet<(DispatchConversation, DispatchKey)>,
    children: Vec<MentionTurnRequest>,
}

impl AtomicQueue {
    fn failing_once_before_commit() -> Self {
        Self {
            fail_before_commit: AtomicBool::new(true),
            ..Self::default()
        }
    }

    fn with_forced_concurrent_commit_attempts() -> Self {
        Self {
            before_commit: Some(Arc::new(Barrier::new(2))),
            ..Self::default()
        }
    }

    fn durable_counts(&self) -> (usize, usize) {
        let durable = self.durable.lock().unwrap();
        (durable.keys.len(), durable.children.len())
    }
}

impl MentionTurnQueue for AtomicQueue {
    fn enqueue_once(&self, request: MentionTurnRequest) -> MentionTurnFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if let Some(barrier) = &self.before_commit {
                barrier.wait().await;
            }
            if self.fail_before_commit.swap(false, Ordering::SeqCst) {
                return Err(Box::new(io::Error::other("failed before commit")) as BoxError);
            }
            let key = (request.conversation.clone(), request.key);
            let mut durable = self.durable.lock().unwrap();
            if durable.keys.contains(&key) {
                return Ok(EnqueueOutcome::Already);
            }
            durable.keys.insert(key);
            durable.children.push(request);
            Ok(EnqueueOutcome::Enqueued)
        })
    }
}

struct FixedQueue {
    calls: AtomicUsize,
    outcome: std::result::Result<EnqueueOutcome, ()>,
}

impl MentionTurnQueue for FixedQueue {
    fn enqueue_once(&self, _: MentionTurnRequest) -> MentionTurnFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            self.outcome
                .map_err(|()| Box::new(io::Error::other("queue failed")) as BoxError)
        })
    }
}

fn fixture() -> (Vec<RosterMember>, MentionDispatchInput) {
    (
        vec![
            RosterMember {
                id: "alice".into(),
                name: None,
            },
            RosterMember {
                id: "bob".into(),
                name: None,
            },
        ],
        MentionDispatchInput {
            key: DispatchKey {
                trigger_sequence: 9,
            },
            conversation: DispatchConversation {
                desk_id: "eng".into(),
                thread_root: Some(4),
            },
            author_id: "alice".into(),
            content: "please take this @bob".into(),
            mentions: vec![Mention {
                target: MentionTarget::Agent { id: "bob".into() },
                text: "@bob".into(),
                offset: 17,
                quiet: false,
            }],
            hop: 0,
        },
    )
}

fn policy() -> MentionDispatchPolicy {
    MentionDispatchPolicy {
        enabled: true,
        max_hops: 2,
    }
}

#[tokio::test]
async fn no_decision_calls_the_queue_zero_times() {
    let (members, input) = fixture();
    let roster = Roster::new(&members, &[], &[]);
    let queue = AtomicQueue::default();
    let outcome = dispatch_mention(
        &queue,
        MentionDispatchPolicy {
            enabled: false,
            max_hops: 2,
        },
        &input,
        &roster,
    )
    .await
    .unwrap();
    assert_eq!(
        outcome,
        MentionDispatchOutcome::NotDispatched {
            reason: tinyhivemind_core::dispatch::NoDispatchReason::Disabled
        }
    );
    assert_eq!(queue.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_decision_calls_once_and_maps_every_expected_outcome() {
    for (queued, expected) in [
        (EnqueueOutcome::Enqueued, MentionDispatchOutcome::Enqueued),
        (EnqueueOutcome::Already, MentionDispatchOutcome::Already),
        (
            EnqueueOutcome::Refused {
                reason: EnqueueRefusal::Unauthorized,
            },
            MentionDispatchOutcome::Refused {
                reason: EnqueueRefusal::Unauthorized,
            },
        ),
        (
            EnqueueOutcome::Refused {
                reason: EnqueueRefusal::TargetUnavailable,
            },
            MentionDispatchOutcome::Refused {
                reason: EnqueueRefusal::TargetUnavailable,
            },
        ),
        (
            EnqueueOutcome::Refused {
                reason: EnqueueRefusal::FeatureDisabled,
            },
            MentionDispatchOutcome::Refused {
                reason: EnqueueRefusal::FeatureDisabled,
            },
        ),
    ] {
        let (members, input) = fixture();
        let roster = Roster::new(&members, &[], &[]);
        let queue = FixedQueue {
            calls: AtomicUsize::new(0),
            outcome: Ok(queued),
        };
        assert_eq!(
            dispatch_mention(&queue, policy(), &input, &roster)
                .await
                .unwrap(),
            expected
        );
        assert_eq!(queue.calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn an_unexpected_queue_failure_is_typed_preserves_source_and_never_retries() {
    let (members, input) = fixture();
    let roster = Roster::new(&members, &[], &[]);
    let queue = FixedQueue {
        calls: AtomicUsize::new(0),
        outcome: Err(()),
    };
    let error = dispatch_mention(&queue, policy(), &input, &roster)
        .await
        .unwrap_err();
    assert!(matches!(error, crate::Error::Enqueue { .. }));
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(queue.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn concurrent_duplicate_requests_enqueue_exactly_once() {
    let (members, input) = fixture();
    let roster = Roster::new(&members, &[], &[]);
    let queue = AtomicQueue::with_forced_concurrent_commit_attempts();
    let (left, right) = tokio::join!(
        dispatch_mention(&queue, policy(), &input, &roster),
        dispatch_mention(&queue, policy(), &input, &roster),
    );
    let mut outcomes = [left.unwrap(), right.unwrap()];
    outcomes.sort_by_key(|outcome| matches!(outcome, MentionDispatchOutcome::Already));
    assert_eq!(
        outcomes,
        [
            MentionDispatchOutcome::Enqueued,
            MentionDispatchOutcome::Already
        ]
    );
    assert_eq!(queue.calls.load(Ordering::SeqCst), 2);
    assert_eq!(queue.durable_counts(), (1, 1));
}

#[tokio::test]
async fn retry_after_failure_before_atomic_commit_creates_one_key_and_one_child() {
    let (members, input) = fixture();
    let roster = Roster::new(&members, &[], &[]);
    let queue = AtomicQueue::failing_once_before_commit();

    assert!(
        dispatch_mention(&queue, policy(), &input, &roster)
            .await
            .is_err()
    );
    assert_eq!(queue.durable_counts(), (0, 0));
    assert_eq!(
        dispatch_mention(&queue, policy(), &input, &roster)
            .await
            .unwrap(),
        MentionDispatchOutcome::Enqueued
    );
    assert_eq!(
        dispatch_mention(&queue, policy(), &input, &roster)
            .await
            .unwrap(),
        MentionDispatchOutcome::Already
    );
    assert_eq!(queue.durable_counts(), (1, 1));
}

#[tokio::test]
async fn idempotency_key_is_bound_to_conversation_scope() {
    let (members, input) = fixture();
    let roster = Roster::new(&members, &[], &[]);
    let queue = AtomicQueue::default();
    let mut other = input.clone();
    other.conversation.thread_root = Some(5);
    assert_eq!(
        dispatch_mention(&queue, policy(), &input, &roster)
            .await
            .unwrap(),
        MentionDispatchOutcome::Enqueued
    );
    assert_eq!(
        dispatch_mention(&queue, policy(), &other, &roster)
            .await
            .unwrap(),
        MentionDispatchOutcome::Enqueued
    );
    assert_eq!(
        dispatch_mention(&queue, policy(), &input, &roster)
            .await
            .unwrap(),
        MentionDispatchOutcome::Already
    );
}

#[test]
fn pins_runtime_outcome_wire_forms() {
    fn assert_wire<T>(value: &T, expected: serde_json::Value)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Eq + std::fmt::Debug,
    {
        assert_eq!(serde_json::to_value(value).unwrap(), expected);
        assert_eq!(serde_json::from_value::<T>(expected).unwrap(), *value);
    }

    fn assert_requires_every_field<T>(wire: &serde_json::Value)
    where
        T: serde::de::DeserializeOwned,
    {
        let keys = wire
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            let mut missing = wire.clone();
            missing.as_object_mut().unwrap().remove(&key);
            assert!(
                serde_json::from_value::<T>(missing).is_err(),
                "{key} must be required"
            );
        }
    }

    for (refusal, wire) in [
        (EnqueueRefusal::Unauthorized, "unauthorized"),
        (EnqueueRefusal::TargetUnavailable, "target_unavailable"),
        (EnqueueRefusal::FeatureDisabled, "feature_disabled"),
    ] {
        assert_wire(&refusal, serde_json::json!(wire));
        let enqueue_wire = serde_json::json!({"status": "refused", "reason": wire});
        assert_wire(
            &EnqueueOutcome::Refused { reason: refusal },
            enqueue_wire.clone(),
        );
        assert_requires_every_field::<EnqueueOutcome>(&enqueue_wire);
        let dispatch_wire = serde_json::json!({"status": "refused", "reason": wire});
        assert_wire(
            &MentionDispatchOutcome::Refused { reason: refusal },
            dispatch_wire.clone(),
        );
        assert_requires_every_field::<MentionDispatchOutcome>(&dispatch_wire);
    }
    for (outcome, wire) in [
        (
            EnqueueOutcome::Enqueued,
            serde_json::json!({"status": "enqueued"}),
        ),
        (
            EnqueueOutcome::Already,
            serde_json::json!({"status": "already"}),
        ),
    ] {
        assert_wire(&outcome, wire.clone());
        assert_requires_every_field::<EnqueueOutcome>(&wire);
    }
    for (outcome, wire) in [
        (
            MentionDispatchOutcome::Enqueued,
            serde_json::json!({"status": "enqueued"}),
        ),
        (
            MentionDispatchOutcome::Already,
            serde_json::json!({"status": "already"}),
        ),
    ] {
        assert_wire(&outcome, wire.clone());
        assert_requires_every_field::<MentionDispatchOutcome>(&wire);
    }
    let not_dispatched_wire = serde_json::json!({"status": "not_dispatched", "reason": "disabled"});
    assert_wire(
        &MentionDispatchOutcome::NotDispatched {
            reason: tinyhivemind_core::dispatch::NoDispatchReason::Disabled,
        },
        not_dispatched_wire.clone(),
    );
    assert_requires_every_field::<MentionDispatchOutcome>(&not_dispatched_wire);
    assert!(
        serde_json::from_value::<EnqueueOutcome>(serde_json::json!({"status": "refused"})).is_err()
    );
    assert!(
        serde_json::from_value::<MentionDispatchOutcome>(serde_json::json!({
            "status": "not_dispatched"
        }))
        .is_err()
    );
}

/// What [ADR 0009] requires of one refusal's rendered sentence.
///
/// [ADR 0009]: ../../../../docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Rendering {
    /// The refusal turns on a named other, so it renders the shared sentence.
    Shared,
    /// The refusal is about the caller's own request, so it may render its own.
    Own,
}

/// Every refusal, so a rendering can be asserted over the whole enum.
const EVERY_REFUSAL: [EnqueueRefusal; 3] = [
    EnqueueRefusal::Unauthorized,
    EnqueueRefusal::TargetUnavailable,
    EnqueueRefusal::FeatureDisabled,
];

/// The classification ADR 0009 fixes for each refusal.
///
/// The match is wildcard-free on purpose: a variant added later does not
/// compile until whoever adds it has classified it here.
fn required(refusal: EnqueueRefusal) -> Rendering {
    match refusal {
        EnqueueRefusal::Unauthorized | EnqueueRefusal::TargetUnavailable => Rendering::Shared,
        EnqueueRefusal::FeatureDisabled => Rendering::Own,
    }
}

#[test]
fn every_refusal_that_turns_on_a_named_other_renders_the_shared_sentence() {
    let shared: Vec<EnqueueRefusal> = EVERY_REFUSAL
        .into_iter()
        .filter(|refusal| required(*refusal) == Rendering::Shared)
        .collect();
    assert!(shared.len() > 1, "collapsing one refusal collapses nothing");
    for refusal in shared {
        assert_eq!(
            refusal.to_string(),
            NO_AVAILABLE_TARGET,
            "{refusal:?} must not be distinguishable from a target that was never there"
        );
    }
}

#[test]
fn a_feature_turned_off_late_is_worded_as_a_feature_turned_off() {
    // Every queue refusal is reached only after the core decision resolved a
    // target, so wording only reachable here would report that the mentioned
    // agent exists. The one refusal that is not about the target borrows the
    // sentence of the reason a caller could already have got before any
    // mention was read.
    assert_eq!(
        EnqueueRefusal::FeatureDisabled.to_string(),
        NoDispatchReason::Disabled.to_string()
    );
}

#[test]
fn every_refusal_is_a_lowercase_sentence_without_trailing_punctuation() {
    for refusal in EVERY_REFUSAL {
        let sentence = refusal.to_string();
        assert!(!sentence.is_empty(), "{refusal:?}");
        assert_eq!(sentence, sentence.to_lowercase(), "{refusal:?}");
        assert!(!sentence.ends_with(['.', '!', '?']), "{refusal:?}");
    }
}

#[tokio::test]
async fn a_refused_enqueue_and_an_unaddressed_reply_decline_in_the_same_words() {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let policy = MentionDispatchPolicy {
        enabled: true,
        max_hops: 2,
    };
    let refusing = RefusingQueue {
        reason: EnqueueRefusal::TargetUnavailable,
    };
    let refused = dispatch_mention(&refusing, policy, &input(vec![mention("bob", 0)]), &roster)
        .await
        .unwrap();
    let unaddressed = dispatch_mention(&refusing, policy, &input(Vec::new()), &roster)
        .await
        .unwrap();
    let sentence = |outcome| match outcome {
        MentionDispatchOutcome::NotDispatched { reason } => reason.to_string(),
        MentionDispatchOutcome::Refused { reason } => reason.to_string(),
        other => panic!("expected a decline, got {other:?}"),
    };
    assert_ne!(refused, unaddressed, "the operator's log still separates them");
    assert_eq!(sentence(refused), sentence(unaddressed));
}
