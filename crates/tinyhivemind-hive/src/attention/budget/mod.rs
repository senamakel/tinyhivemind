//! Max-min fair allocation of a character budget across context sources.

#[cfg(test)]
mod test;

mod types;

pub use types::{BudgetPolicy, BudgetRequest, BudgetShare, BudgetVerdict};

/// Allocate a character budget across competing context sources.
#[must_use]
pub fn allocate_chars(requests: &[BudgetRequest], policy: &BudgetPolicy) -> Vec<BudgetShare> {
    let mut carried: Vec<bool> = vec![true; requests.len()];
    let mut level;
    loop {
        let wants: Vec<usize> = carried_wants(requests, &carried);
        level = fair_share_level(&wants, policy.total_chars);
        let Some(greediest) = requests
            .iter()
            .zip(&carried)
            .filter(|(request, carried)| **carried && unusable(request, level, policy))
            .map(|(request, _)| request.wanted)
            .max()
        else {
            break;
        };
        for (index, request) in requests.iter().enumerate() {
            if carried[index] && request.wanted == greediest && unusable(request, level, policy) {
                carried[index] = false;
            }
        }
    }

    requests
        .iter()
        .zip(&carried)
        .map(|(request, carried)| {
            let granted = if *carried { request.wanted.min(level) } else { 0 };
            BudgetShare {
                source_id: request.source_id.clone(),
                granted,
                omitted: request.wanted - granted,
                verdict: verdict(granted, request.wanted),
            }
        })
        .collect()
}

fn carried_wants(requests: &[BudgetRequest], carried: &[bool]) -> Vec<usize> {
    requests
        .iter()
        .zip(carried)
        .filter(|(_, carried)| **carried)
        .map(|(request, _)| request.wanted)
        .collect()
}

/// Whether a source's fair share would be a fragment rather than context.
///
/// A source small enough to arrive whole is never a fragment, however small it
/// is, so the floor applies only to a claim the budget had to cut.
fn unusable(request: &BudgetRequest, level: usize, policy: &BudgetPolicy) -> bool {
    let granted = request.wanted.min(level);
    granted < request.wanted && granted < policy.min_useful_chars
}

/// The largest equal share every carried source may draw without overspending.
///
/// This is max-min fairness stated directly: the level `L` maximising
/// `sum(min(wanted, L))` subject to that sum fitting the budget. Every source
/// wanting less than `L` is satisfied outright and the rest divide what is
/// left, which is what "the surplus is redistributed until it is exhausted"
/// means once the redistribution has run to its fixed point.
///
/// The predicate is monotone in `L`, so a binary search over `0..=total` finds
/// it in `O(n log total)` without iterating the redistribution.
fn fair_share_level(wants: &[usize], total: usize) -> usize {
    let (mut low, mut high) = (0, total);
    while low < high {
        // Round up, so `low` always names a level already known to fit.
        let mid = low + (high - low).div_ceil(2);
        if fits(wants, mid, total) {
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
fn fits(wants: &[usize], level: usize, total: usize) -> bool {
    let mut spent = 0_usize;
    for wanted in wants {
        spent = spent.saturating_add((*wanted).min(level));
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
