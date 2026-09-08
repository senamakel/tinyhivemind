//! Tests for how `referral` picks a forward candidate and where the child
//! turn lands: staying local, crossing to a mentioned agent's home desk,
//! resolving a desk mention, and every way a candidate is refused. Also pins
//! the compatibility statement against `mention_dispatch` for a plain policy.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::*;
use super::support::{OPEN, accepted, conversation, decide, desk, desk_mention, input, member, refused};
use crate::{
    dispatch::{
        MentionDispatchDecision, MentionDispatchInput, MentionDispatchPolicy, mention_dispatch,
    },
    roster::Roster,
};

#[test]
fn a_default_policy_refers_nothing() {
    assert_eq!(
        refused(
            ReferralPolicy::DEFAULT,
            &input("ada", "payments", vec![agent_mention("linus", 0)])
        ),
        NoReferralReason::Disabled,
    );
}

#[test]
fn an_exhausted_hop_budget_stops_the_chain() {
    let mut asked = input("ada", "payments", vec![agent_mention("linus", 0)]);
    asked.hop = 2;
    assert_eq!(refused(OPEN, &asked), NoReferralReason::HopLimitReached);
}

#[test]
fn an_inactive_author_refers_nothing() {
    assert_eq!(
        refused(
            OPEN,
            &input("nobody", "payments", vec![agent_mention("linus", 0)])
        ),
        NoReferralReason::SourceInactive,
    );
}

#[test]
fn a_mention_of_a_deskmate_stays_in_this_conversation() {
    let referral = accepted(
        OPEN,
        &input("ada", "payments", vec![agent_mention("grace", 0)]),
    );
    assert!(!referral.crosses());
    assert_eq!(referral.to, conversation("payments", None));
    // Nothing crossed, so there is nothing to carry back.
    assert_eq!(referral.origin, None);
    assert_eq!(referral.kind, ReferralKind::Forward);
    assert_eq!(referral.child_hop, 1);
}

#[test]
fn a_same_desk_referral_keeps_its_thread_root() {
    let mut asked = input("ada", "payments", vec![agent_mention("grace", 0)]);
    asked.conversation = conversation("payments", Some(12));
    assert_eq!(
        accepted(OPEN, &asked).to,
        conversation("payments", Some(12))
    );
}

#[test]
fn a_mention_of_another_desks_member_relocates_to_their_desk() {
    let referral = accepted(
        OPEN,
        &input("ada", "payments", vec![agent_mention("linus", 0)]),
    );
    assert!(referral.crosses());
    assert_eq!(referral.to, conversation("platform", None));
    assert_eq!(referral.target_id, "linus");
    assert_eq!(
        referral.origin,
        Some(ReferralOrigin {
            conversation: conversation("payments", None),
            asker_id: "ada".to_owned(),
        }),
    );
}

#[test]
fn a_crossing_referral_lands_on_the_desk_channel_not_a_thread() {
    let mut asked = input("ada", "payments", vec![agent_mention("linus", 0)]);
    asked.conversation = conversation("payments", Some(12));
    let referral = accepted(OPEN, &asked);
    assert_eq!(referral.to.thread_root, None);
    // The back edge still names the thread that asked.
    assert_eq!(
        referral
            .origin
            .map(|origin| origin.conversation.thread_root),
        Some(Some(12)),
    );
}

#[test]
fn a_desk_mention_selects_exactly_one_responder() {
    let referral = accepted(
        OPEN,
        &input("ada", "payments", vec![desk_mention("platform", 0)]),
    );
    assert_eq!(referral.target_id, "linus");
    assert_eq!(referral.to, conversation("platform", None));
}

#[test]
fn a_desk_mention_is_inert_without_the_knob() {
    let policy = ReferralPolicy {
        reach: ReferralReach::Channels,
        ..OPEN
    };
    assert_eq!(
        refused(
            policy,
            &input("ada", "payments", vec![desk_mention("platform", 0)])
        ),
        NoReferralReason::NoReferralTarget,
    );
}

#[test]
fn a_desk_mention_naming_this_desk_refers_nothing() {
    assert_eq!(
        refused(
            OPEN,
            &input("ada", "payments", vec![desk_mention("payments", 0)])
        ),
        NoReferralReason::SelfDesk,
    );
}

#[test]
fn a_desk_mention_naming_no_desk_refers_nothing() {
    assert_eq!(
        refused(
            OPEN,
            &input("ada", "payments", vec![desk_mention("legal", 0)])
        ),
        NoReferralReason::UnknownDesk,
    );
}

#[test]
fn a_desk_whose_only_member_is_the_author_refers_nothing() {
    let members = [member("ada")];
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = [desk("payments", &["grace"]), desk("platform", &["ada"])];
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    let asked = input("ada", "payments", vec![desk_mention("platform", 0)]);
    let decision = referral(OPEN, &asked, &roster, &desks).expect("well formed");
    assert_eq!(
        decision,
        ReferralDecision::None {
            reason: NoReferralReason::EmptyDesk
        },
    );
}

#[test]
fn the_lowest_offset_candidate_wins_and_a_later_one_is_never_a_fallback() {
    let asked = input(
        "ada",
        "payments",
        vec![agent_mention("ada", 3), agent_mention("linus", 40)],
    );
    assert_eq!(refused(OPEN, &asked), NoReferralReason::SelfMention);
}

#[test]
fn a_person_mention_is_skipped_rather_than_stopping_the_scan() {
    let asked = input(
        "ada",
        "payments",
        vec![
            Mention {
                target: MentionTarget::Person {
                    id: "sam".to_owned(),
                },
                text: "@sam".to_owned(),
                offset: 0,
                quiet: false,
            },
            agent_mention("linus", 10),
        ],
    );
    assert_eq!(accepted(OPEN, &asked).target_id, "linus");
}

#[test]
fn a_quiet_mention_never_refers() {
    let mut quiet = agent_mention("linus", 0);
    quiet.quiet = true;
    assert_eq!(
        refused(OPEN, &input("ada", "payments", vec![quiet])),
        NoReferralReason::NoReferralTarget,
    );
}

#[test]
fn an_inactive_target_refers_nothing() {
    assert_eq!(
        refused(
            OPEN,
            &input("ada", "payments", vec![agent_mention("nobody", 0)])
        ),
        NoReferralReason::TargetInactive,
    );
}

#[test]
fn a_target_on_no_desk_has_nowhere_to_run() {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = [desk("payments", &["ada", "grace"])];
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    let asked = input("ada", "payments", vec![agent_mention("linus", 0)]);
    assert_eq!(
        referral(OPEN, &asked, &roster, &desks).expect("well formed"),
        ReferralDecision::None {
            reason: NoReferralReason::TargetDeskless
        },
    );
}

#[test]
fn everyone_is_present_in_general() {
    let asked = input("ada", "General", vec![agent_mention("linus", 0)]);
    let referral = accepted(OPEN, &asked);
    assert!(!referral.crosses());
    assert_eq!(referral.to.desk_id, "General");
}

#[test]
fn a_malformed_roster_is_a_typed_error() {
    let members = [member(""), member("ada")];
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = desks();
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    let asked = input("ada", "payments", vec![agent_mention("linus", 0)]);
    assert!(referral(OPEN, &asked, &roster, &desks).is_err());
}

#[test]
fn a_tombstoned_target_is_refused_as_plainly_inactive() {
    // `linus` stays in the roster and on the platform desk so his old
    // messages keep their author, but a referral to him is refused with the
    // reason a retired member already gets. A reason of its own would tell
    // the asker that somebody used to hold that desk.
    let members = members();
    let tombstoned = vec!["linus".to_owned()];
    let roster = Roster::new(&members, &[], &[]).with_tombstoned(&tombstoned);
    let desk_records = desks();
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]).with_tombstoned(&tombstoned);
    let decision = referral(
        OPEN,
        &input("ada", "payments", vec![agent_mention("linus", 0)]),
        &roster,
        &desks,
    )
    .expect("snapshots are well formed");
    assert_eq!(
        decision,
        ReferralDecision::None {
            reason: NoReferralReason::TargetInactive
        }
    );
}

/// The compatibility statement from the spec, asserted rather than claimed.
#[test]
fn without_the_new_knobs_referral_decides_what_mention_dispatch_decides() {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = desks();
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    let plain = ReferralPolicy {
        enabled: true,
        max_hops: 2,
        ..ReferralPolicy::DEFAULT
    };

    for mentions in [
        vec![agent_mention("grace", 0)],
        // The interesting case: a target who is not on this desk still gets
        // pulled into this conversation, exactly as today.
        vec![agent_mention("linus", 0)],
        vec![agent_mention("ada", 0)],
        vec![agent_mention("nobody", 0)],
        vec![desk_mention("platform", 0)],
        Vec::new(),
    ] {
        let asked = input("ada", "payments", mentions.clone());
        let dispatched = mention_dispatch(
            MentionDispatchPolicy {
                enabled: true,
                max_hops: 2,
            },
            &MentionDispatchInput {
                key: asked.key,
                conversation: asked.conversation.clone(),
                author_id: asked.author_id.clone(),
                content: asked.content.clone(),
                mentions,
                hop: asked.hop,
            },
            &roster,
        )
        .expect("well formed");
        let referred = referral(plain, &asked, &roster, &desks).expect("well formed");
        match (dispatched, referred) {
            (MentionDispatchDecision::One { request }, ReferralDecision::One { referral }) => {
                assert_eq!(request.target_id, referral.target_id);
                assert_eq!(request.source_id, referral.source_id);
                assert_eq!(request.content, referral.content);
                assert_eq!(request.conversation, referral.to);
                assert_eq!(request.child_hop, referral.child_hop);
                assert!(!referral.crosses());
            }
            (MentionDispatchDecision::None { .. }, ReferralDecision::None { .. }) => {}
            (dispatched, referred) => {
                panic!("dispatch said {dispatched:?} and referral said {referred:?}")
            }
        }
    }
}
