# Off-floor exchange

An aside that rides alongside the turn that authored it is free but rationed:
only the member the attention market authorized may write one, so a room
converging in eleven turns writes at most eleven private rows. This module lifts
that ration. Between turns — or before the first one, never *during* one — a host
may run an **exchange round** in which each member the library names appends at
most one private row, taking no floor and producing no turn.

The behavior is [`docs/specs/off-floor-exchange.md`][spec]; the decision and its
reasoning are [ADR 0012][adr].

## The one property everything rests on

A private row is dropped by `live_traces` before it can reach a trace, a
standing, the sequence they fold at, the directory or the floor, and
`EpisodeState::spent` counts turns rather than rows. So `step` returns the same
`HiveStep` — *including the state it commits* — whether an exchange happened or
not.

That is asserted rather than argued, and in two places, because it is the whole
safety case:

- `episode::test::an_aside_riding_along_with_a_turn_costs_the_room_nothing` — one
  readable case.
- `tests/fuzz_invariants.rs::asides_interleaved_into_an_arbitrary_transcript_do_not_move_the_episode`
  — arbitrary private rows, carrying the same fuzzed grammar as the desk rows so
  private `!propose`/`!support`/`!commit` are in the corpus, interleaved anywhere
  in an arbitrary transcript.

Both go red if the audience filter is removed. Note what the second one is *not*:
it holds desk sequence numbers fixed across the two transcripts it compares, so
it says nothing about live per-episode sequence numbering. See "What it does
spend", below.

## The public surface

```rust
pub fn exchange(
    policy: &ExchangePolicy,
    state: &EpisodeState,
    transcript: &[SessionMessage],
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
) -> Result<ExchangeRound>;
```

A fold over what the caller already holds. It answers whether a round is open,
which members it names, and how many rows the episode may still write. It does
no waiting and **chooses no peer**: who a member wants to contact is that
member's judgement, and the audience it writes is validated by
`tinyhivemind::aside::aside` exactly as an aside riding alongside a turn is.

`ExchangeRound::Open::members` is in desk order and contains only members that
are active on the episode's desk and still under their own contact cap. A host
may run any subset, in any order, including none.

## Spend is folded, never stored

A member's contacts are the private rows above the episode's watermark that it
authored. There is no counter, so there is nothing that could disagree with the
log — the discipline `standings` and `directory` already follow, and what keeps
this inside the charter's "no second journal" rule.

Rounds are counted by the **busiest** member, since a round gives each member at
most one row. That under-counts rounds in which a member declined to speak, which
is the safe direction: it can only leave a host with budget it did not use.

## Bounds, and why they are two

```rust
pub struct ExchangePolicy {
    pub enabled: bool,
    pub contact_cap: u32,  // private rows one member may author, per episode
    pub round_cap: u32,    // rounds the episode may open at all
}
```

Both finite, both readable before the episode starts. The total an episode can
produce is at most `min(members × contact_cap, round_cap × members)`, so a host
reads its worst case off its own policy rather than discovering it.

`ExchangeRound::Open::remaining` is clamped **per member and then summed**, not
summed and then clamped. A round gives each member at most one row, so no member
can write more than `rounds_left` however much of its cap is left. The two forms
diverge exactly when spend is uneven — one member spent out and another
untouched — which is when a host sizing a batch off the field would overallocate.

`enabled: false`, `contact_cap: 0` and `round_cap: 0` all close every round and a
`Closed` round names which. Zero is a configuration a host may hold on purpose,
so unlike `defer_cap` it is not an error.

## Operational constraints

**A host must not dispatch an exchange row.** A row authored in a round is never
handed to `mention_dispatch`. That is what stops rows begetting rows: the
exchange cannot cascade, and its worst case stays the constant above. The peer
answers on its own next turn, which the attention market was going to give it.

**A refused audience is dropped, not published.** If the `aside` fold declines to
make a row private, the row is not written. Falling back to the desk would put a
second desk-visible contribution on one turn.

**A round does not interleave with a turn.** Run rounds between turns, so the
transcript a turn was composed from is never edited underneath it.

**The price is model calls, and it is the host's to authorize.** A round is *n*
of them. That is why the policy is off by default and its ceilings explicit, and
why the benchmark reports private rows in a column of their own rather than
folding them into a cost figure that means something else.

## What it does spend

A sequence number. Rows are unique in the one shared journal, so a private row
takes the next sequence, and `salience::standing` reads recency as a raw
`at - trace.sequence` distance that feeds the attention market's choice of who
speaks next. A room writing private rows can therefore in principle decide
differently from one that does not, without a word of their content mattering.

Measured, it does not: the benchmark's `hive+quiet` and `hive+hush` arms write
the identical rows on the identical schedule and discard every answer, and both
are `+0.0 [+0.0, +0.0]` at up to twenty rows an episode. That makes the
sequence-distance decay harmless at these volumes rather than good design. A host
writing far more rows than that should measure rather than assume; decaying over
desk turns instead of raw sequences would be a library change with its own ADR.

[spec]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/specs/off-floor-exchange.md
[adr]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0012-an-exchange-round-spends-model-calls-not-turns.md
