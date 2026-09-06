# The context budget

How much of what a turn already holds fits in front of a member, when a
pinboard, a thread index, a digest and a set of host notes all want more room
than the shared budget has.

## Design

`allocate_chars(requests, policy)` is a fold, in the same shape as the
attention market next door: it takes the caller's `BudgetRequest`s and a
`BudgetPolicy` and returns one owned `BudgetShare` per request, in request
order. It never reads or edits the text a request describes — a source's
`wanted` count is the caller's own measurement, and the fold only decides the
numbers.

Two rules compose:

1. **Max-min fairness.** Every source is offered an equal share; a source
   wanting less than its share takes what it needs and releases the rest; the
   surplus is redistributed until it is exhausted. `fair_share_level` finds
   this by binary search — the predicate "does granting everyone `min(wanted,
   level)` fit the budget" is monotone in `level`, so the search is `O(n log
   total)` without ever running the redistribution as a loop.
2. **A floor on truncation.** Fairness alone can cut a source to a fragment
   too short to teach the reader anything. A source whose fair share falls
   below `BudgetPolicy::min_useful_chars` is dropped instead of carried at
   that length, and the drop reports every character it omitted so the caller
   can tell the reader what is missing. The floor applies only to a claim the
   budget had to *cut* — a source small enough to arrive whole is whole,
   however short it is.

When some sources are unusable at the fair level, they yield one at a time,
greediest first (ties break on `source_id`), and the level is recomputed after
each drop. Dropping one claim at a time rather than the whole unusable class
means a room of `n` equal claims degrades a claim at a time as `n` grows,
rather than losing all context the moment the budget can no longer serve
everyone.

## Determinism

Every share is a function of the *set* of requests, the budget and the floor —
never of the position a request happened to occupy in the slice. Reordering
the requests permutes the result identically and changes no number; the test
suite asserts this over rotations, a reversal, and a deterministic sweep.

Two places would have broken that if handled naively, and both are settled by
refusing a positional tie-break:

- **The remainder.** `total_chars` rarely divides evenly, so a few characters
  are usually left over. They stay unspent rather than being handed to
  whoever happens to sit first in the slice.
- **Who yields.** Equal claims tie on `wanted`, and the tie breaks on
  `source_id` rather than on position, so two requests identical in *both*
  fields are the one place this fold is not order-independent — they are
  genuinely interchangeable, and which one drops then follows request order.

All arithmetic is integer and saturating (`usize`, via `saturating_add`), so a
source asking for `usize::MAX` is cut like any other request rather than
wrapping the running sum.

## Public surface

| Item | Purpose |
| --- | --- |
| `allocate_chars` | The fold. Returns one `BudgetShare` per request, in request order. |
| `BudgetRequest` | `source_id` and `wanted`: what one context source would spend alone. |
| `BudgetPolicy` | `total_chars` and `min_useful_chars`; `DEFAULT` derives both from `BrevityPolicy`. |
| `BudgetShare` | `source_id`, `granted`, `omitted`, and `verdict`, echoed back per request. |
| `BudgetVerdict` | `Whole`, `Truncated`, or `Dropped` — what the allocation did to one source. |

`BudgetPolicy::DEFAULT` sets `total_chars` to one full window at
`BrevityPolicy::message_chars` — what a turn already expects to read — and
`min_useful_chars` to a third of one such message.

## Operational constraints

- **Pure and stateless.** No clock, no storage, no host type; every quantity
  is an argument, matching every other fold in this crate.
- **Reads nothing about the sources beyond their declared size.** The caller
  owns truncating and marking the actual text; this module only decides how
  many characters each source may spend.
- **The algorithm and its order-independence boundary are specified** in
  [`../../../../docs/specs/hive-mind.md`](../../../../docs/specs/hive-mind.md)
  under "The context budget" — read that spec before changing the tie-break
  rules above, since the fixed points there are pinned by tests over
  permutations of the request slice.
