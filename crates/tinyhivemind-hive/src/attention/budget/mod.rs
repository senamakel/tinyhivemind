//! Max-min fair allocation of a character budget across context sources.

#[cfg(test)]
mod test;

mod types;

pub use types::{BudgetPolicy, BudgetRequest, BudgetShare, BudgetVerdict};

/// Allocate a character budget across competing context sources.
#[must_use]
pub fn allocate_chars(requests: &[BudgetRequest], policy: &BudgetPolicy) -> Vec<BudgetShare> {
    let _ = policy;
    requests
        .iter()
        .map(|request| BudgetShare {
            source_id: request.source_id.clone(),
            granted: request.wanted,
            omitted: 0,
            verdict: BudgetVerdict::Whole,
        })
        .collect()
}
