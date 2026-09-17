//! Tests pinning the serde wire form of a referral, its policy, and a
//! refusal decision.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::*;
use super::support::{OPEN, accepted, agent_mention, input};

#[test]
fn a_referral_pins_its_wire_form() {
    let referral = accepted(
        OPEN,
        &input("ada", "payments", vec![agent_mention("linus", 0)]),
    );
    let json = serde_json::to_value(&referral).expect("serializes");
    assert_eq!(
        json,
        serde_json::json!({
            "key": { "trigger_sequence": 7 },
            "kind": "forward",
            "source_id": "ada",
            "target_id": "linus",
            "content": "body",
            "from": { "desk_id": "payments", "thread_root": null },
            "to": { "desk_id": "platform", "thread_root": null },
            "origin": {
                "conversation": { "desk_id": "payments", "thread_root": null },
                "asker_id": "ada",
            },
            "child_hop": 1,
        }),
    );
    let round_tripped: Referral = serde_json::from_value(json).expect("deserializes");
    assert_eq!(round_tripped, referral);
}

#[test]
fn a_policy_and_a_refusal_pin_their_wire_forms() {
    assert_eq!(
        serde_json::to_value(ReferralPolicy::DEFAULT).expect("serializes"),
        serde_json::json!({
            "enabled": false,
            "max_hops": 0,
            "reach": "local",
            "returns": false,
        }),
    );
    assert_eq!(
        serde_json::to_value(ReferralDecision::None {
            reason: NoReferralReason::HopLimitReached
        })
        .expect("serializes"),
        serde_json::json!({ "kind": "none", "reason": "hop_limit_reached" }),
    );
    assert_eq!(ReferralPolicy::default(), ReferralPolicy::DEFAULT);
}
