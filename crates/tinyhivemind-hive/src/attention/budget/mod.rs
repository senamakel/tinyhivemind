//! Max-min fair allocation of a character budget across context sources.
//!
//! The market next door decides *who speaks*. This decides *how much of what a
//! turn already holds fits in front of them*: a pinboard, a thread index, a
//! digest and a set of host notes all want room in one bounded prompt, and the
//! budget they share is smaller than the sum of what they would each spend.
//!
//! The rule is **max-min fairness**, the standard allocation: every source is
//! offered an equal share; a source wanting less than its share takes what it
//! needs and releases the rest; the surplus is redistributed until it is
//! exhausted. Equivalently — and this is how [`allocate_chars`] computes it —
//! there is one level `L` such that each source is granted `min(wanted, L)`
//! and `L` is the largest level that fits the budget. Small sources are never
//! squeezed by a large one, and a large one cannot take more than an equal
//! share of what the small ones leave.
//!
//! Fairness alone still produces rubbish, which is the second rule. A source
//! cut to fifty characters is not context, it is a fragment that spends budget
//! and teaches the reader nothing. A source whose share falls below
//! [`BudgetPolicy::min_useful_chars`] is therefore **dropped and marked**
//! ([`BudgetVerdict::Dropped`], carrying every character it omitted) rather
//! than carried in a state nobody can read. The floor applies only to a claim
//! the budget had to *cut*: a source small enough to arrive whole is whole,
//! however short it is.
//!
//! # Determinism
//!
//! Every share is a function of the *set* of requests, the budget and the
//! floor — never of the position a request happened to occupy. Reordering the
//! requests permutes the result identically and changes no number, which the
//! tests assert over rotations, a reversal and a deterministic sweep.
//!
//! Two places would have broken that, and both are settled by refusing a
//! positional tie-break:
//!
//! - **The remainder.** `total_chars` rarely divides evenly, so a few
//!   characters are usually left over. They stay unspent. Handing them to
//!   somebody would mean choosing whom, and the only thing distinguishing
//!   equal claimants is where they sat in the slice.
//! - **Who yields.** When the budget cannot usefully serve everyone, sources
//!   are dropped one at a time, greediest first, and the level recomputed after
//!   each — so a room of `n` equal claims loses claims one by one instead of
//!   losing all context at the moment `n` grows too large. Equal claims tie on
//!   `wanted`, and the tie breaks on `source_id` rather than on position. Two
//!   requests identical in *both* are genuinely interchangeable; which of them
//!   is dropped then follows request order, and that is the one place this fold
//!   is not order-independent.
//!
//! All arithmetic is integer and saturating, so a source asking for
//! `usize::MAX` is cut like any other rather than wrapping the sum.

#[cfg(test)]
mod test;

mod types;

pub use types::{BudgetPolicy, BudgetRequest, BudgetShare, BudgetVerdict};

/// Allocate a character budget across competing context sources.
///
/// Returns one [`BudgetShare`] per request, in request order, each carrying
/// what its source may spend, what was withheld, and whether it survived. The
/// caller does the cutting and the marking; this fold only decides the
/// numbers, and it never reads or edits the text behind a request.
///
/// See the [module documentation][self] for the algorithm and for exactly
/// where the result is and is not order-independent.
///
/// # Example
///
/// ```
/// use tinyhivemind_hive::attention::{
///     BudgetPolicy, BudgetRequest, BudgetVerdict, allocate_chars,
/// };
///
/// // A tight budget, four sources, and one of them enormous.
/// let policy = BudgetPolicy { total_chars: 700, min_useful_chars: 250 };
/// let requests = [
///     BudgetRequest::new("pins", 50),
///     BudgetRequest::new("digest", 4_000),
///     BudgetRequest::new("threads", 600),
///     BudgetRequest::new("notes", 300),
/// ];
///
/// let shares = allocate_chars(&requests, &policy);
///
/// // The pinboard wanted little and got all of it; its surplus went to the
/// // others rather than being wasted on an equal quarter share.
/// assert_eq!((shares[0].granted, shares[0].verdict), (50, BudgetVerdict::Whole));
///
/// // The greediest claim is the one that yields, and it says how much it took
/// // with it so the caller can tell the reader.
/// assert_eq!(shares[1].verdict, BudgetVerdict::Dropped);
/// assert_eq!(shares[1].omitted, 4_000);
///
/// // What it freed goes to the survivors: the thread index is cut to a length
/// // still worth reading, and the notes now fit whole.
/// assert_eq!((shares[2].granted, shares[2].verdict), (350, BudgetVerdict::Truncated));
/// assert_eq!((shares[3].granted, shares[3].verdict), (300, BudgetVerdict::Whole));
/// assert_eq!(shares.iter().map(|share| share.granted).sum::<usize>(), 700);
/// ```
#[must_use]
pub fn allocate_chars(requests: &[BudgetRequest], policy: &BudgetPolicy) -> Vec<BudgetShare> {
    let mut carried: Vec<bool> = vec![true; requests.len()];
    let mut level;
    loop {
        let wants: Vec<usize> = carried_wants(requests, &carried);
        level = fair_share_level(&wants, policy.total_chars);
        // One at a time, greediest first, rather than every unusable source at
        // once: dropping the whole class starves a room of `n` equal claims of
        // *all* context the moment `n` passes what the budget can usefully
        // serve, where dropping one at a time degrades a claim at a time.
        let Some((index, _)) = requests
            .iter()
            .enumerate()
            .filter(|(index, request)| carried[*index] && unusable(request, level, policy))
            .max_by(|(_, held), (_, next)| {
                held.wanted
                    .cmp(&next.wanted)
                    .then_with(|| held.source_id.cmp(&next.source_id))
            })
        else {
            break;
        };
        carried[index] = false;
    }

    requests
        .iter()
        .zip(&carried)
        .map(|(request, carried)| {
            let granted = if *carried {
                request.wanted.min(level)
            } else {
                0
            };
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
