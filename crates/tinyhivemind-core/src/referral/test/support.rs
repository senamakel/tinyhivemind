//! Shared referral fixtures: a payments/platform roster and desk snapshot,
//! the fully open policy the swarm harness runs at, and helpers to build
//! inputs and unwrap a decision.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::*;
use crate::{
    desk::{Desk, ResponderMode},
    dispatch::DispatchKey,
    roster::RosterMember,
};

/// Build an active roster member whose display name equals its id.
pub(super) fn member(id: &str) -> RosterMember {
    RosterMember {
        id: id.to_owned(),
        name: Some(id.to_owned()),
    }
}

/// Build a desk record with the given members, in `Lead` responder mode.
pub(super) fn desk(id: &str, members: &[&str]) -> Desk {
    Desk {
        id: id.to_owned(),
        name: id.to_owned(),
        description: None,
        members: members.iter().map(|id| (*id).to_owned()).collect(),
        responder_mode: ResponderMode::Lead,
    }
}

/// The fixture roster: four active agents split across two desks.
pub(super) fn members() -> Vec<RosterMember> {
    ["ada", "grace", "linus", "hedy"]
        .iter()
        .copied()
        .map(member)
        .collect()
}

/// The fixture desks: `payments` (ada, grace) and `platform` (linus, hedy).
pub(super) fn desks() -> Vec<Desk> {
    vec![
        desk("payments", &["ada", "grace"]),
        desk("platform", &["linus", "hedy"]),
    ]
}

/// Build a nonquiet agent mention at the given offset.
pub(super) fn agent_mention(id: &str, offset: usize) -> Mention {
    Mention {
        target: MentionTarget::Agent { id: id.to_owned() },
        text: format!("@{id}"),
        offset,
        quiet: false,
    }
}

/// Build a nonquiet desk mention at the given offset.
pub(super) fn desk_mention(id: &str, offset: usize) -> Mention {
    Mention {
        target: MentionTarget::Desk { id: id.to_owned() },
        text: format!("@#{id}"),
        offset,
        quiet: false,
    }
}

/// Build a conversation on the given desk, with an optional thread root.
pub(super) fn conversation(desk_id: &str, thread_root: Option<u64>) -> DispatchConversation {
    DispatchConversation {
        desk_id: desk_id.to_owned(),
        thread_root,
    }
}

/// Build a referral input authored on `desk_id` with the given mentions.
pub(super) fn input(author: &str, desk_id: &str, mentions: Vec<Mention>) -> ReferralInput {
    ReferralInput {
        key: DispatchKey {
            trigger_sequence: 7,
        },
        conversation: conversation(desk_id, None),
        author_id: author.to_owned(),
        content: "body".to_owned(),
        mentions,
        hop: 0,
        origin: None,
    }
}

/// Everything on, two hops, which is what the swarm harness runs at.
pub(super) const OPEN: ReferralPolicy = ReferralPolicy {
    enabled: true,
    max_hops: 2,
    reach: ReferralReach::Desks,
    returns: true,
};

/// Run `referral` over the fixture roster and desks, panicking on a structural error.
pub(super) fn decide(policy: ReferralPolicy, input: &ReferralInput) -> ReferralDecision {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = desks();
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    referral(policy, input, &roster, &desks).expect("snapshots are well formed")
}

/// Decide, and unwrap the reason, panicking if a referral was selected instead.
pub(super) fn refused(policy: ReferralPolicy, input: &ReferralInput) -> NoReferralReason {
    match decide(policy, input) {
        ReferralDecision::None { reason } => reason,
        ReferralDecision::One { referral } => {
            panic!("expected no referral, got one to {}", referral.target_id)
        }
    }
}

/// Decide, and unwrap the referral, panicking if none was selected.
pub(super) fn accepted(policy: ReferralPolicy, input: &ReferralInput) -> Referral {
    match decide(policy, input) {
        ReferralDecision::One { referral } => *referral,
        ReferralDecision::None { reason } => panic!("expected a referral, got {reason:?}"),
    }
}
