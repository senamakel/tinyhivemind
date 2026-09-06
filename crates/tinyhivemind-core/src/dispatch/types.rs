//! Stable payloads for one bounded mention-dispatch decision.

use crate::mention::Mention;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// The one sentence every refusal that turns on a named other renders as.
///
/// A refusal an agent can tell apart is a refusal an agent can probe. If
/// "nobody was addressed" and "the agent you addressed is not available" came
/// back in different words, an agent could learn which ids a roster holds by
/// reading which sentence it got, one name at a time. So they come back in
/// these words, and so does every later reason that depends on the same
/// lookup — including the ones a host's queue returns after it revalidates.
///
/// The distinction is not lost; it changes audience. The structured reason is
/// still there for the operator's log, and `{:?}` still prints it. This is
/// what `{}` prints, because the rendering that is safe to hand to a model has
/// to be the one that costs nothing to reach for.
///
/// See [ADR 0009] and [`docs/specs/mention-dispatch.md`].
///
/// [ADR 0009]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md
/// [`docs/specs/mention-dispatch.md`]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/specs/mention-dispatch.md
pub const NO_AVAILABLE_TARGET: &str = "there is no available agent to pass this to";

/// The sentence a reason renders when the hop budget stopped the decision.
///
/// [`NoDispatchReason::HopOverflow`] shares it with
/// [`NoDispatchReason::HopLimitReached`]: the overflow is only reachable once
/// the target has been resolved, so wording of its own would report that the
/// mentioned agent exists.
const HOP_BUDGET_SPENT: &str = "this has already been passed along as far as it may go";

/// Explicit host policy for agent-to-agent mention dispatch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MentionDispatchPolicy {
    /// Whether mention dispatch is enabled for this committed reply.
    pub enabled: bool,
    /// Maximum permitted chain depth, supplied by the host without a library cap.
    pub max_hops: u32,
}

/// The committed trigger that makes an enqueue idempotent within its scope.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchKey {
    /// Host-owned sequence of the committed agent reply.
    pub trigger_sequence: u64,
}

/// Pure conversation identity bound into a mention turn request.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchConversation {
    /// Canonical case-sensitive desk id.
    pub desk_id: String,
    /// Root sequence for a thread, or explicit `null` for the desk channel.
    #[serde(deserialize_with = "deserialize_required_option")]
    pub thread_root: Option<u64>,
}

/// Inputs captured from one committed agent reply.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MentionDispatchInput {
    /// Idempotency key for the committed reply.
    pub key: DispatchKey,
    /// Conversation to which any child turn remains bound.
    pub conversation: DispatchConversation,
    /// Exact active agent id that authored the reply.
    pub author_id: String,
    /// Exact committed reply content passed to the child turn.
    pub content: String,
    /// Revalidated resolved mentions from the committed content.
    pub mentions: Vec<Mention>,
    /// Current chain depth of the committed reply.
    pub hop: u32,
}

/// Why no child turn was selected.
///
/// The variant is the operator's record and `{:?}` prints it. The `Display`
/// rendering is the acting agent's, and is deliberately coarser: every reason
/// that turns on a named other renders as [`NO_AVAILABLE_TARGET`]. See
/// [ADR 0009].
///
/// [ADR 0009]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoDispatchReason {
    /// Host policy explicitly disabled dispatch.
    Disabled,
    /// The current reply has exhausted the host's hop budget.
    HopLimitReached,
    /// The committed author is not an active agent.
    SourceInactive,
    /// No nonquiet direct agent mention was present.
    NoDirectAgentMention,
    /// The first direct mention addresses the author.
    SelfMention,
    /// The first direct mention addresses an inactive agent.
    TargetInactive,
    /// The child hop could not be represented.
    HopOverflow,
}

impl fmt::Display for NoDispatchReason {
    /// Render the sentence an acting agent may repeat to a person.
    ///
    /// A reason renders wording of its own only when what it reveals is
    /// something the caller already holds: the policy it supplied, the hop it
    /// supplied, its own identity. Anything that turns on whether a *named
    /// other* exists, is active, or is reachable renders as
    /// [`NO_AVAILABLE_TARGET`], because those are the refusals a caller could
    /// otherwise probe one name at a time.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Disabled => "passing this on to another agent is turned off here",
            Self::HopLimitReached | Self::HopOverflow => HOP_BUDGET_SPENT,
            Self::SourceInactive => {
                "the agent that wrote this is no longer active, so nothing was passed on"
            }
            Self::SelfMention => "an agent cannot pass a message on to itself",
            Self::NoDirectAgentMention | Self::TargetInactive => NO_AVAILABLE_TARGET,
        })
    }
}

/// One canonical child-turn enqueue request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MentionTurnRequest {
    /// Idempotency key derived from the committed reply.
    pub key: DispatchKey,
    /// Exact active author id.
    pub source_id: String,
    /// Exact active target id.
    pub target_id: String,
    /// Exact committed reply content.
    pub content: String,
    /// Bound conversation scope.
    pub conversation: DispatchConversation,
    /// Checked child depth.
    pub child_hop: u32,
}

/// Pure result of considering one committed reply.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MentionDispatchDecision {
    /// No enqueue may be attempted.
    None {
        /// Deterministic reason for stopping.
        reason: NoDispatchReason,
    },
    /// Exactly one canonical enqueue may be attempted.
    One {
        /// Request to pass unchanged to the host queue.
        request: MentionTurnRequest,
    },
}

fn deserialize_required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
