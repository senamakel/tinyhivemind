//! Tests for carrying one answer back to the conversation that asked: the
//! `returns` knob, precedence against a fresh forward, and every way a
//! return is refused.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::*;
use super::support::{OPEN, accepted, agent_mention, conversation, input, refused};

#[test]
fn a_reply_under_a_crossing_referral_carries_one_answer_back() {
    let mut answered = input("linus", "platform", Vec::new());
    answered.conversation = conversation("platform", None);
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", Some(12)),
        asker_id: "ada".to_owned(),
    });
    let referral = accepted(OPEN, &answered);
    assert_eq!(referral.kind, ReferralKind::Return);
    assert_eq!(referral.target_id, "ada");
    assert_eq!(referral.to, conversation("payments", Some(12)));
    assert_eq!(referral.child_hop, 2);
    // A return carries no origin, so the round trip cannot ring.
    assert_eq!(referral.origin, None);
}

#[test]
fn a_forward_of_its_own_takes_precedence_over_the_answer() {
    let mut answered = input("linus", "platform", vec![agent_mention("hedy", 0)]);
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", None),
        asker_id: "ada".to_owned(),
    });
    let referral = accepted(OPEN, &answered);
    assert_eq!(referral.kind, ReferralKind::Forward);
    assert_eq!(referral.target_id, "hedy");
}

#[test]
fn a_return_needs_the_knob() {
    let policy = ReferralPolicy {
        returns: false,
        ..OPEN
    };
    let mut answered = input("linus", "platform", Vec::new());
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", None),
        asker_id: "ada".to_owned(),
    });
    assert_eq!(
        refused(policy, &answered),
        NoReferralReason::NoReferralTarget
    );
}

#[test]
fn an_answer_committed_where_it_was_asked_carries_nothing_back() {
    let mut answered = input("grace", "payments", Vec::new());
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", None),
        asker_id: "ada".to_owned(),
    });
    assert_eq!(refused(OPEN, &answered), NoReferralReason::NoReferralTarget);
}

#[test]
fn an_answer_never_returns_to_its_own_author() {
    let mut answered = input("ada", "platform", Vec::new());
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", None),
        asker_id: "ada".to_owned(),
    });
    assert_eq!(refused(OPEN, &answered), NoReferralReason::SelfMention);
}

#[test]
fn an_answer_to_a_departed_asker_is_dropped() {
    let mut answered = input("linus", "platform", Vec::new());
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", None),
        asker_id: "departed".to_owned(),
    });
    assert_eq!(refused(OPEN, &answered), NoReferralReason::TargetInactive);
}
