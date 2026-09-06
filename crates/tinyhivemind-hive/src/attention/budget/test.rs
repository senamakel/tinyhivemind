//! Unit tests for max-min fair character allocation.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn tight(total: usize, min_useful: usize) -> BudgetPolicy {
    BudgetPolicy {
        total_chars: total,
        min_useful_chars: min_useful,
    }
}

fn granted(shares: &[BudgetShare]) -> Vec<(&str, usize)> {
    shares
        .iter()
        .map(|share| (share.source_id.as_str(), share.granted))
        .collect()
}

#[test]
fn grants_every_source_in_full_when_the_budget_covers_them() {
    let requests = [
        BudgetRequest::new("pins", 300),
        BudgetRequest::new("threads", 200),
    ];
    let shares = allocate_chars(&requests, &tight(1_000, 100));
    assert_eq!(granted(&shares), [("pins", 300), ("threads", 200)]);
    assert!(shares.iter().all(|share| share.omitted == 0));
    assert!(
        shares
            .iter()
            .all(|share| share.verdict == BudgetVerdict::Whole)
    );
}

#[test]
fn splits_an_oversubscribed_budget_equally_between_equal_claims() {
    let requests = [
        BudgetRequest::new("pins", 1_000),
        BudgetRequest::new("threads", 1_000),
    ];
    let shares = allocate_chars(&requests, &tight(500, 200));
    assert_eq!(granted(&shares), [("pins", 250), ("threads", 250)]);
    assert!(shares.iter().all(|share| share.omitted == 750));
    assert!(
        shares
            .iter()
            .all(|share| share.verdict == BudgetVerdict::Truncated)
    );
}

#[test]
fn redistributes_what_a_small_source_does_not_need() {
    let requests = [
        BudgetRequest::new("pins", 50),
        BudgetRequest::new("threads", 1_000),
        BudgetRequest::new("board", 1_000),
    ];
    let shares = allocate_chars(&requests, &tight(1_000, 100));
    assert_eq!(
        granted(&shares),
        [("pins", 50), ("threads", 475), ("board", 475)]
    );
    assert_eq!(shares[0].verdict, BudgetVerdict::Whole);
    assert_eq!(shares[1].omitted, 525);
}

#[test]
fn drops_a_source_cut_below_the_useful_floor_and_marks_what_it_omitted() {
    let requests = [
        BudgetRequest::new("threads", 1_000),
        BudgetRequest::new("pins", 100),
    ];
    let shares = allocate_chars(&requests, &tight(250, 200));
    assert_eq!(granted(&shares), [("threads", 0), ("pins", 100)]);
    assert_eq!(shares[0].verdict, BudgetVerdict::Dropped);
    assert_eq!(shares[0].omitted, 1_000);
    assert_eq!(shares[1].verdict, BudgetVerdict::Whole);
}
