//! Tests holding every [`NoReferralReason`] to the disclosure classification
//! fixed by `docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md`,
//! and pinning its settled wording.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::*;
use super::support::{OPEN, agent_mention, desk, desk_mention, input, members};
use crate::{
    dispatch::{NO_AVAILABLE_TARGET, NoDispatchReason},
    roster::Roster,
};

/// What `docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md`
/// requires of one reason's rendered sentence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Rendering {
    /// The reason turns on a named other, so it renders the shared sentence.
    Shared,
    /// The reason is about the caller's own request, so it may render its own.
    Own,
}

/// Every reason, so a rendering can be asserted over the whole enum.
const EVERY_REASON: [NoReferralReason; 11] = [
    NoReferralReason::Disabled,
    NoReferralReason::HopLimitReached,
    NoReferralReason::SourceInactive,
    NoReferralReason::NoReferralTarget,
    NoReferralReason::SelfMention,
    NoReferralReason::TargetInactive,
    NoReferralReason::SelfDesk,
    NoReferralReason::EmptyDesk,
    NoReferralReason::TargetDeskless,
    NoReferralReason::UnknownDesk,
    NoReferralReason::HopOverflow,
];

/// The classification ADR 0009 fixes for each reason.
///
/// The match is wildcard-free on purpose: a variant added later does not
/// compile until whoever adds it has classified it here, and the tests below
/// then hold its rendering to that classification.
///
/// A desk is not a named other for this purpose. `DeskSet` carries no
/// per-viewer scoping, so there is no "that desk is not yours to see" state a
/// desk refusal could be confused with — the same reasoning that keeps
/// [`crate::error::Error::UnknownDesk`] and
/// [`crate::error::Error::AmbiguousDesk`] separate. *Membership* of a desk is
/// a roster fact about other agents, so [`NoReferralReason::EmptyDesk`] is
/// withheld while [`NoReferralReason::UnknownDesk`] is not.
fn required(reason: NoReferralReason) -> Rendering {
    match reason {
        NoReferralReason::Disabled
        | NoReferralReason::HopLimitReached
        | NoReferralReason::SourceInactive
        | NoReferralReason::SelfMention
        | NoReferralReason::SelfDesk
        | NoReferralReason::UnknownDesk
        | NoReferralReason::HopOverflow => Rendering::Own,
        NoReferralReason::NoReferralTarget
        | NoReferralReason::TargetInactive
        | NoReferralReason::EmptyDesk
        | NoReferralReason::TargetDeskless => Rendering::Shared,
    }
}

#[test]
fn every_reason_that_turns_on_a_named_other_renders_the_shared_sentence() {
    let shared: Vec<NoReferralReason> = EVERY_REASON
        .into_iter()
        .filter(|reason| required(*reason) == Rendering::Shared)
        .collect();
    assert!(shared.len() > 1, "collapsing one reason collapses nothing");
    for reason in shared {
        assert_eq!(
            reason.to_string(),
            NO_AVAILABLE_TARGET,
            "{reason:?} must not be distinguishable from the other withheld reasons"
        );
    }
}

#[test]
fn a_request_local_reason_never_borrows_the_shared_sentence() {
    for reason in EVERY_REASON {
        if required(reason) == Rendering::Own {
            assert_ne!(reason.to_string(), NO_AVAILABLE_TARGET, "{reason:?}");
        }
    }
}

#[test]
fn a_spent_hop_budget_is_worded_the_same_way_on_both_edges() {
    assert_eq!(
        NoReferralReason::HopOverflow.to_string(),
        NoReferralReason::HopLimitReached.to_string()
    );
    assert_eq!(
        NoReferralReason::HopLimitReached.to_string(),
        NoDispatchReason::HopLimitReached.to_string(),
        "one budget, one vocabulary"
    );
}

#[test]
fn every_reason_is_a_lowercase_sentence_without_trailing_punctuation() {
    for reason in EVERY_REASON {
        let sentence = reason.to_string();
        assert!(!sentence.is_empty(), "{reason:?}");
        assert_eq!(sentence, sentence.to_lowercase(), "{reason:?}");
        assert!(!sentence.ends_with(['.', '!', '?']), "{reason:?}");
    }
}

#[test]
fn renders_the_settled_sentences() {
    assert_eq!(
        NoReferralReason::Disabled.to_string(),
        "referring this to another desk is turned off here"
    );
    assert_eq!(
        NoReferralReason::SourceInactive.to_string(),
        "the agent that wrote this is no longer active, so nothing was passed on"
    );
    assert_eq!(
        NoReferralReason::SelfMention.to_string(),
        "an agent cannot pass a message on to itself"
    );
    assert_eq!(
        NoReferralReason::SelfDesk.to_string(),
        "this is already on the desk it was addressed to"
    );
    assert_eq!(
        NoReferralReason::UnknownDesk.to_string(),
        "no single desk goes by that name"
    );
}

#[test]
fn an_empty_desk_and_a_deskless_target_refuse_in_the_same_words() {
    // `hedy` is active and on no desk; `solo` is a desk whose only member is
    // the asker. Both are refused, and neither sentence reports which.
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = vec![desk("payments", &["ada", "grace"]), desk("solo", &["ada"])];
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    let refuse = |mention| match referral(
        OPEN,
        &input("ada", "payments", vec![mention]),
        &roster,
        &desks,
    )
    .expect("snapshots are well formed")
    {
        ReferralDecision::None { reason } => reason,
        ReferralDecision::One { .. } => panic!("no referral was available"),
    };
    let deskless = refuse(agent_mention("hedy", 0));
    let empty = refuse(desk_mention("solo", 0));
    assert_eq!(deskless, NoReferralReason::TargetDeskless);
    assert_eq!(empty, NoReferralReason::EmptyDesk);
    assert_eq!(deskless.to_string(), empty.to_string());
}
