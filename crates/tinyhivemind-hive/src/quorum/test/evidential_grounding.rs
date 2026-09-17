//! `require_evidential`: a support only counts once its citation chain
//! reaches a stated fact, and an objection under the same gate silences
//! nobody unless its own author has deposited one.

use super::super::*;
use super::support::{evidential, fold, policy, said, standing};
use crate::trace::read;

#[test]
fn support_grounded_only_in_another_opinion_does_not_count_as_evidential() {
    // A cascade with a citation on it: every link is grounded, and the chain
    // bottoms out in an opinion rather than in a fact.
    let transcript = [
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(
            2,
            "critic",
            "!support #stage ^1 The planner is usually right.",
        ),
        said(3, "scout", "!support #stage ^2 The critic agrees."),
    ];
    assert_eq!(
        standing(&fold(&transcript, &evidential()), "stage").supporters,
        ["planner"],
    );
    // The same chain counts when only a citation is required.
    assert_eq!(
        standing(&fold(&transcript, &policy(2)), "stage").supporters,
        ["planner", "critic", "scout"],
    );
}

#[test]
fn support_whose_chain_reaches_a_fact_counts_as_evidential() {
    let transcript = [
        said(
            1,
            "auditor",
            "!evidence In-flight requests sit at 24 to 31.",
        ),
        said(2, "planner", "!propose #pool The pool caps at twenty."),
        said(3, "critic", "!support #pool ^1 The numbers line up."),
        // A chain two links long still reaches the fact at sequence 1.
        said(
            4,
            "scout",
            "!support #pool ^3 Following the critic's grounds.",
        ),
    ];
    let standings = fold(&transcript, &evidential());
    let held = standing(&standings, "pool");
    assert_eq!(held.supporters, ["planner", "critic", "scout"]);
    assert!(held.carried(&evidential()));
}

#[test]
fn a_citation_cycle_terminates_and_reads_as_social() {
    // Two supports citing each other, and nothing else. The visited set is what
    // stops the resolution recurring; the chain reaches no fact, so neither
    // support counts.
    let transcript = [
        said(1, "planner", "!propose #stage Stage it."),
        said(2, "critic", "!support #stage ^3 Circular."),
        said(3, "scout", "!support #stage ^2 Also circular."),
    ];
    assert_eq!(
        standing(&fold(&transcript, &evidential()), "stage").supporters,
        ["planner"],
    );
}

#[test]
fn a_chain_that_leaves_the_window_reads_as_social() {
    // The citation is real, and it is outside the window. Chasing it would make
    // a member's standing depend on how far back it happened to have paged.
    let transcript = [
        said(
            1,
            "auditor",
            "!evidence In-flight requests sit at 24 to 31.",
        ),
        said(40, "planner", "!propose #pool The pool caps at twenty."),
        said(41, "critic", "!support #pool ^1 The numbers line up."),
    ];
    let narrow = QuorumPolicy {
        window: 5,
        ..evidential()
    };
    let standings = standings(&read(&transcript), Sequence(41), &narrow).expect("folds");
    assert_eq!(standing(&standings, "pool").supporters, ["planner"]);
}

#[test]
fn an_objection_from_a_member_with_no_evidence_silences_nobody() {
    let transcript = [
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(2, "auditor", "!evidence The environment was retired."),
        said(3, "critic", "!support #stage ^2 Worth it anyway."),
        // The scout has put no fact on the floor, so its objection is inert
        // under `require_evidential`, and lands under the weaker policy.
        said(4, "scout", "!object >3 ^1 I disagree with the critic."),
    ];
    assert_eq!(
        standing(&fold(&transcript, &evidential()), "stage").supporters,
        ["planner", "critic"],
    );
    assert_eq!(
        standing(&fold(&transcript, &policy(2)), "stage").silenced,
        ["critic"],
    );
}

#[test]
fn requiring_evidential_grounds_implies_requiring_grounds_at_all() {
    let transcript = [
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(2, "critic", "!support #stage No citation at all."),
    ];
    let lax_but_evidential = QuorumPolicy {
        require_grounded: false,
        require_evidential: true,
        ..policy(2)
    };
    assert_eq!(
        standing(&fold(&transcript, &lax_but_evidential), "stage").supporters,
        ["planner"],
    );
}
