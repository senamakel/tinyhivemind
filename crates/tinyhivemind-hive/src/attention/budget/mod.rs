//! Max-min fair allocation of a character budget across context sources.

#[cfg(test)]
mod test;

mod types;

pub use types::{BudgetPolicy, BudgetRequest, BudgetShare, BudgetVerdict};

/// Allocate a character budget across competing context sources.
#[must_use]
pub fn allocate_chars(requests: &[BudgetRequest], policy: &BudgetPolicy) -> Vec<BudgetShare> {
    let share = policy.total_chars.checked_div(requests.len()).unwrap_or(0);
    requests
        .iter()
        .map(|request| {
            let granted = request.wanted.min(share);
            BudgetShare {
                source_id: request.source_id.clone(),
                granted,
                omitted: request.wanted - granted,
                verdict: if granted == request.wanted {
                    BudgetVerdict::Whole
                } else {
                    BudgetVerdict::Truncated
                },
            }
        })
        .collect()
}
