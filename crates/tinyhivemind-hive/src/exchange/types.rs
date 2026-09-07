//! Inputs and outputs of the off-floor exchange fold.

use serde::{Deserialize, Serialize};

/// How much private exchange one episode may run off the floor.
///
/// Two independent ceilings, both finite and both readable before an episode
/// starts, so a host knows its worst case rather than discovering it. The
/// total rows an episode can produce is at most
/// `min(members × contact_cap, round_cap × members)`.
///
/// Off by default. An exchange round is *n* model calls, which is a real cost
/// and a host's to authorize — see
/// [ADR 0012][adr].
///
/// [adr]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0012-an-exchange-round-spends-model-calls-not-turns.md
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ExchangePolicy {
    /// Whether any round may open at all.
    pub enabled: bool,
    /// Private rows one member may author across the whole episode.
    pub contact_cap: u32,
    /// Rounds the episode may open at all.
    pub round_cap: u32,
}

impl ExchangePolicy {
    /// Off, which reproduces an episode taken before this mechanism existed.
    pub const DEFAULT: Self = Self {
        enabled: false,
        contact_cap: 0,
        round_cap: 0,
    };
}

impl Default for ExchangePolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Why no exchange round is open.
///
/// A closed round always names a reason; there is no silent no-op, for the
/// same reason [`crate::HiveStep::Idle`] is a variant rather than an empty
/// turn.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoExchangeReason {
    /// The host has not turned the mechanism on.
    Disabled,
    /// `round_cap` rounds have already opened.
    RoundsSpent,
    /// Every active member has spent its own `contact_cap`.
    ContactsSpent,
    /// The desk has nobody on it who could contact anybody else.
    TooFewMembers,
}

/// Whether members may exchange privately now, and which of them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ExchangeRound {
    /// Each of these members may append **at most one** private row now.
    ///
    /// In desk order, and containing only members that are active on the
    /// episode's desk and still under their own contact cap. A host may run
    /// any subset of them, in any order, including none.
    Open {
        /// The members this round authorizes, in desk order.
        members: Vec<String>,
        /// Private rows this episode may still write, across every member.
        remaining: u32,
    },
    /// No round, and why.
    Closed {
        /// What closed it.
        reason: NoExchangeReason,
    },
}
