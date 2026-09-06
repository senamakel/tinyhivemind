//! Runtime outcomes for the atomic mention-turn enqueue boundary.

use serde::{Deserialize, Serialize};
use std::fmt;
use tinyhivemind_core::dispatch::{NO_AVAILABLE_TARGET, NoDispatchReason};

/// Result returned by a host's atomic enqueue transaction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EnqueueOutcome {
    /// This request durably created its one child turn.
    Enqueued,
    /// The bound conversation-and-trigger key was already enqueued.
    Already,
    /// The transaction refused the request after live revalidation.
    Refused {
        /// Final, expected reason for refusing the enqueue.
        reason: EnqueueRefusal,
    },
}

/// An expected host refusal after atomic live revalidation.
///
/// The variant is the operator's record and `{:?}` prints it. The `Display`
/// rendering is the acting agent's, and every one of these is reached only
/// after the pure decision resolved a target — so a sentence unique to this
/// boundary would report that the mentioned agent exists.
/// [`Self::Unauthorized`] and [`Self::TargetUnavailable`] therefore render as
/// [`NO_AVAILABLE_TARGET`], and [`Self::FeatureDisabled`] borrows the wording
/// of [`NoDispatchReason::Disabled`], which a caller could already have got
/// before any mention was read. See [ADR 0009].
///
/// [ADR 0009]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnqueueRefusal {
    /// Current authorization no longer permits this source-target edge.
    Unauthorized,
    /// The target is no longer available for a turn.
    TargetUnavailable,
    /// Current host policy no longer enables the feature.
    FeatureDisabled,
}

impl fmt::Display for EnqueueRefusal {
    /// Render the sentence an acting agent may repeat to a person.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized | Self::TargetUnavailable => f.write_str(NO_AVAILABLE_TARGET),
            Self::FeatureDisabled => NoDispatchReason::Disabled.fmt(f),
        }
    }
}

/// Final result of one library dispatch attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MentionDispatchOutcome {
    /// Pure policy or routing selected no target, so the queue was not called.
    NotDispatched {
        /// Deterministic reason for stopping.
        reason: NoDispatchReason,
    },
    /// The host durably created the child turn.
    Enqueued,
    /// The exact scoped trigger had already created its child turn.
    Already,
    /// The host atomically refused the request after revalidation.
    Refused {
        /// Expected refusal returned by the host.
        reason: EnqueueRefusal,
    },
}
