//! Stable inputs and outcomes of one character-budget allocation.

use serde::{Deserialize, Serialize};
use tinyhivemind::BrevityPolicy;

/// One context source competing for the shared character budget.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BudgetRequest {
    /// Caller's stable name for the source, echoed onto its share.
    pub source_id: String,
    /// Characters the source would spend if it were the only one.
    pub wanted: usize,
}

impl BudgetRequest {
    /// Build a request for a named source.
    #[must_use]
    pub fn new(source_id: impl Into<String>, wanted: usize) -> Self {
        Self {
            source_id: source_id.into(),
            wanted,
        }
    }
}

/// How much of the budget one allocation may spend, and on what terms.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BudgetPolicy {
    /// Characters every source together may spend.
    pub total_chars: usize,
    /// Fewest characters that make a *truncated* source worth carrying.
    pub min_useful_chars: usize,
}

impl BudgetPolicy {
    /// The default budget, derived from the stated per-message one.
    ///
    /// `total_chars` is one full window written at
    /// [`BrevityPolicy::message_chars`] — what a turn already expects to read —
    /// and `min_useful_chars` is a third of one such message, below which a cut
    /// source is a fragment rather than context.
    pub const DEFAULT: Self = Self {
        total_chars: BrevityPolicy::DEFAULT.message_chars * BrevityPolicy::DEFAULT.window,
        min_useful_chars: BrevityPolicy::DEFAULT.message_chars / 3,
    };
}

impl Default for BudgetPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What the allocation did to one source.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetVerdict {
    /// The source got everything it asked for.
    Whole,
    /// The source was cut to a share it can still be read from.
    Truncated,
    /// The source is not carried at all, and the caller must say so.
    Dropped,
}

/// One source's settled claim on the budget.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BudgetShare {
    /// The requesting source, echoed unchanged.
    pub source_id: String,
    /// Characters this source may spend; `0` when it was dropped.
    pub granted: usize,
    /// Characters withheld from it: `wanted - granted`.
    pub omitted: usize,
    /// Whether the source is whole, cut, or gone.
    pub verdict: BudgetVerdict,
}
