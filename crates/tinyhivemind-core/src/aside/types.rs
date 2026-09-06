//! Stable payloads for one bounded private aside.

use crate::{dispatch::DispatchConversation, mention::Mention};
use serde::{Deserialize, Serialize};

/// Who a stored row is addressed to, when that is narrower than its
/// conversation.
///
/// [`Audience::Desk`] is what every row written before this mechanism means,
/// and what a host writes when it means nothing special. [`Audience::Aside`]
/// restricts a row to its author plus the named agent ids — and to every
/// person, who reads it in full as audit access rather than as membership.
///
/// An audience is fixed when the row is appended. Widening one afterwards
/// could never be redelivered incrementally, because a sharing watermark
/// advances past filtered rows unconditionally, so a widened row would sit
/// below the watermark forever.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Audience {
    /// Every reader of the conversation.
    #[default]
    Desk,
    /// The author plus these agent ids, and nobody else.
    Aside {
        /// Addressed agent ids, in the order they were written, without the
        /// author and without repeats.
        members: Vec<String>,
    },
}

impl Audience {
    /// Whether this row is visible to everyone who reads the conversation.
    #[must_use]
    pub const fn is_desk(&self) -> bool {
        matches!(self, Self::Desk)
    }

    /// The addressed agent ids, or an empty slice for a desk-visible row.
    #[must_use]
    pub fn members(&self) -> &[String] {
        match self {
            Self::Desk => &[],
            Self::Aside { members } => members,
        }
    }

    /// Whether `viewer` may read this row's content.
    ///
    /// `author_id` is the row author's agent id, or `None` when a person, the
    /// operator, or a system source wrote it. An author is always admitted to
    /// its own row.
    ///
    /// A person and the operator are admitted to everything. That is the
    /// property that keeps the mechanism auditable: privacy here is between
    /// agents and is a deliberation device, never a security boundary.
    #[must_use]
    pub fn admits(&self, viewer: &Viewer, author_id: Option<&str>) -> bool {
        let Self::Aside { members } = self else {
            return true;
        };
        match viewer {
            Viewer::Operator | Viewer::Person { .. } => true,
            Viewer::Agent { id } => {
                Some(id.as_str()) == author_id || members.iter().any(|member| member == id)
            }
        }
    }
}

/// Who a projection is being assembled for.
///
/// A projection that cannot name its reader cannot narrow for it, which is why
/// this rides on the query rather than being inferred. The operator and every
/// person read every row in full.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Viewer {
    /// The local operator, reading everything.
    Operator,
    /// A human participant, reading everything — audit access, not membership.
    Person {
        /// The exact person id.
        id: String,
    },
    /// An agent, reading a row only when its audience admits it.
    Agent {
        /// The exact agent id.
        id: String,
    },
}

impl Viewer {
    /// The agent id this viewer reads as, or `None` for a person or operator.
    #[must_use]
    pub fn agent_id(&self) -> Option<&str> {
        match self {
            Self::Agent { id } => Some(id.as_str()),
            Self::Operator | Self::Person { .. } => None,
        }
    }

    /// Whether this viewer reads every row regardless of audience.
    #[must_use]
    pub const fn reads_everything(&self) -> bool {
        matches!(self, Self::Operator | Self::Person { .. })
    }
}

/// Explicit host policy for private asides.
///
/// [`Self::DEFAULT`] permits nothing, so a host that does not opt in gets no
/// asides and every row it writes stays [`Audience::Desk`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AsidePolicy {
    /// Whether an aside may be opened at all.
    pub enabled: bool,
    /// Largest addressed audience, excluding the author. Two is a direct
    /// message; more is a caucus.
    pub max_members: usize,
    /// How many rows one aside may carry before it must settle.
    pub max_messages: usize,
    /// Whether an aside must deposit one desk-visible message before its
    /// members may open another.
    pub must_surface: bool,
    /// Whether an aside must be a thread rather than a run of channel rows.
    pub require_thread: bool,
}

impl AsidePolicy {
    /// Asides fully disabled: the conservative default a host must opt out of.
    pub const DEFAULT: Self = Self {
        enabled: false,
        max_members: 0,
        max_messages: 0,
        must_surface: false,
        require_thread: false,
    };
}

impl Default for AsidePolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Inputs captured from one authored message that asks for an aside.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AsideInput {
    /// Conversation the message is being written on.
    pub conversation: DispatchConversation,
    /// Exact active agent id asking for the aside.
    pub author_id: String,
    /// Revalidated resolved mentions naming the addressed peers.
    pub mentions: Vec<Mention>,
    /// Rows this aside has already spent, folded by the caller.
    pub spent: usize,
    /// Whether a prior aside among the same members has not yet surfaced,
    /// folded by the caller.
    pub unsettled: bool,
}

/// Why no aside was authorized.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoAsideReason {
    /// Host policy explicitly disabled asides.
    Disabled,
    /// The asking author is not an active agent.
    SourceInactive,
    /// The asking author is not an effective member of this desk.
    AuthorNotOnDesk,
    /// No nonquiet direct agent mention addressed anybody.
    NoAudience,
    /// The addressed audience named only the author.
    SelfOnly,
    /// The addressed audience is larger than the policy permits.
    AudienceTooLarge,
    /// An addressed agent is not an active roster member.
    TargetInactive,
    /// An addressed agent is not an effective member of this desk.
    TargetNotOnDesk,
    /// Policy requires an aside to be a thread, and this is the desk channel.
    ThreadRequired,
    /// This aside has spent its whole message budget.
    BudgetSpent,
    /// A prior aside among these members has not surfaced yet.
    UnsettledAside,
}

/// Pure result of considering one request for an aside.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AsideDecision {
    /// No aside is authorized, and the row stays desk-visible.
    None {
        /// Deterministic reason for refusing.
        reason: NoAsideReason,
    },
    /// The audience to stamp on this row.
    One {
        /// The resolved audience, always [`Audience::Aside`].
        audience: Audience,
    },
}
