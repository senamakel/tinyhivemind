//! Max-min fair allocation of a character budget across context sources.

#[cfg(test)]
mod test;

mod types;

pub use types::{BudgetPolicy, BudgetRequest, BudgetShare, BudgetVerdict};

/// Allocate a character budget across competing context sources.
#[must_use]
pub fn allocate_chars(requests: &[BudgetRequest], policy: &BudgetPolicy) -> Vec<BudgetShare> {
    let level = fair_share_level(requests, policy.total_chars);
    requests
        .iter()
        .map(|request| {
            let mut granted = request.wanted.min(level);
            if granted < request.wanted && granted < policy.min_useful_chars {
                granted = 0;
            }
            BudgetShare {
                source_id: request.source_id.clone(),
                granted,
                omitted: request.wanted - granted,
                verdict: verdict(granted, request.wanted),
            }
        })
        .collect()
}

/// The largest equal share every source may draw without overspending.
///
/// This is max-min fairness stated directly: the level `L` maximising
/// `sum(min(wanted, L))` subject to that sum fitting the budget. Every source
/// wanting less than `L` is satisfied outright and the rest divide what is
/// left, which is what "the surplus is redistributed until it is exhausted"
/// means once the redistribution has run to its fixed point.
///
/// The predicate is monotone in `L`, so a binary search over `0..=total` finds
/// it in `O(n log total)` without iterating the redistribution.
fn fair_share_level(requests: &[BudgetRequest], total: usize) -> usize {
    let (mut low, mut high) = (0, total);
    while low < high {
        // Round up, so `low` always names a level known to fit.
        let mid = low + (high - low).div_ceil(2);
        if fits(requests, mid, total) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

/// Whether granting every source `min(wanted, level)` fits inside the budget.
///
/// The running sum saturates and stops the moment it passes the budget, so a
/// source asking for `usize::MAX` cannot wrap the arithmetic.
fn fits(requests: &[BudgetRequest], level: usize, total: usize) -> bool {
    let mut spent = 0_usize;
    for request in requests {
        spent = spent.saturating_add(request.wanted.min(level));
        if spent > total {
            return false;
        }
    }
    true
}

/// Read one settled pair of numbers as an outcome the caller can report.
///
/// Zero characters is [`BudgetVerdict::Dropped`] rather than a truncation to
/// nothing: a source that contributes no text is not carried, and the caller
/// owes the reader a note saying so.
fn verdict(granted: usize, wanted: usize) -> BudgetVerdict {
    if granted >= wanted {
        BudgetVerdict::Whole
    } else if granted == 0 {
        BudgetVerdict::Dropped
    } else {
        BudgetVerdict::Truncated
    }
}
