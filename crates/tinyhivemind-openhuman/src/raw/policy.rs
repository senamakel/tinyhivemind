//! What a raw seat may do: the episode's tools, and nothing else.
//!
//! A [`ToolPolicy`] is an object a raw session takes and `AgentSpec` cannot:
//! middleware that runs before every tool executes. (What a seat remembers is
//! no longer a session object: OpenHuman memory v2 is engine-backed and off for
//! a raw seat, which [`LibraryHost`](super::LibraryHost) asserts by omitting
//! the memory context.)

use async_trait::async_trait;
use openhuman_core::agent::tool_policy::{ToolPolicy, ToolPolicyDecision, ToolPolicyRequest};

/// Allows the belt it was given and denies everything else.
///
/// The belt already holds only the episode's tools, so this is belt and
/// braces -- but it is the seam a real host puts its approval gate on, and a
/// gate that exists only as an allowlist elsewhere can be bypassed by wiring
/// one more tool. Here the refusal is a typed decision the loop sees.
#[derive(Debug)]
pub(super) struct EpisodeGate {
    admitted: Vec<String>,
}

impl EpisodeGate {
    /// A gate admitting exactly `admitted`, by tool name.
    #[must_use]
    pub(super) fn new(admitted: Vec<String>) -> Self {
        Self { admitted }
    }
}

#[async_trait]
impl ToolPolicy for EpisodeGate {
    fn name(&self) -> &'static str {
        "episode_gate"
    }

    async fn check(&self, request: &ToolPolicyRequest) -> ToolPolicyDecision {
        if self.admitted.contains(&request.tool_name) {
            ToolPolicyDecision::Allow
        } else {
            ToolPolicyDecision::deny(format!(
                "`{}` is not an episode tool; a seat holds only the room's tools",
                request.tool_name
            ))
        }
    }
}
