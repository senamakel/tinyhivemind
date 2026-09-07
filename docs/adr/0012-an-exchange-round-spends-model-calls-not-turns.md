# 12. An exchange round spends model calls rather than turns

- **Status:** Proposed
- **Date:** 2026-09-07

## Context

[ADR 0011](0011-an-aside-rides-alongside-a-turn.md) made a private row free by
letting it ride alongside the turn that authored it. That removed the entire
cost of an aside — a 15.5-point swing on a hidden profile — and left it starved:
a row may only be written by whoever the attention market authorized, so a room
converging in eleven turns writes at most eleven private rows.

The benchmark bounds what is being left on the table. `hive+pooled`, which hands
every member every peer's reading and every fact for free, beats the tuned room
by `+30.8 [+28.9, +32.9]` in *fewer* turns. The best arm tied to the floor
reaches `+1.4 [+0.4, +2.4]`. Privacy, payload, targeting and accounting were each
isolated and measured, and none of them is the remaining constraint. Bandwidth
is.

Lifting it means letting members the library did **not** authorize append rows
between turns — which engages the charter's third rule, *one message, one turn*,
and deserved a decision rather than a benchmark arm.

## Decision

**A host may run an *exchange round* between turns, in which each eligible member
appends at most one private row. A round starts no turn and is bounded by an
explicit, finite, host-set policy.** The behavior is
[`../specs/off-floor-exchange.md`](../specs/off-floor-exchange.md).

The library gains one pure fold, `exchange`, in a module of its own. It answers
whether a round is open, which members it names, and how many rows the episode
may still write. It does no waiting and chooses no peer: targeting is a
participant's judgement, and the calling is the host's.

**`step` is untouched, and deliberately so.** An exchange is not a `HiveStep`
variant, because [ADR 0002](0002-hive-episodes-are-sequential.md) says
`HiveStep::Speak` carries exactly one turn and there is no variant carrying two
— and that should stay literally true of the type rather than true-with-an-
asterisk. The episode state machine is sequential exactly as before; the
exchange is a separate fold that provably cannot move it.

**Spend is folded, never stored.** A member's contacts are the private rows above
the watermark that it authored. There is no counter, so there is no second
journal to disagree with the log — the same discipline `standings` and
`directory` already follow.

## Why this is not a relaxation of one message, one turn

The rule names its own failure mode: *a mention that could start N turns without
an approval in sight*. Every clause fails to apply, and the reasoning is the
decision as much as the mechanism is.

- **N turns.** A round starts none. No member gains the floor, no `HiveStep` is
  produced, the turn budget is untouched, and `step` returns the same step —
  including the state it commits — whether the round happened or not. That is
  asserted by a fuzz invariant over arbitrary transcripts, not by argument.
- **Could start.** The danger is unboundedness through cascade: `@everyone`
  mentions N agents, each of whom may mention N more. A row authored in an
  exchange round is never handed to `mention_dispatch`, so rows do not beget
  rows. The worst case is `min(members × contact_cap, round_cap × members)`, a
  constant a host reads off its own policy before starting.
- **Without an approval in sight.** The policy *is* the approval: off by default,
  set by the host rather than by a participant, and finite.

What rule 3 protects is the floor — the ordered shared record where one message
causes one agent to run and the room's standing moves. An exchange round touches
none of it.

## Consequences

**The cost is real and is displayed rather than argued away.** A round is *n*
model calls. That is the honest price of the bandwidth, it is the reason the
ceilings are explicit, and the benchmark reports private rows in a column of
their own so a reader sees the spend beside the gain. A mechanism whose price a
table hides would be worse than one that loses.

**Writing the rows moves nothing by itself, and that was measured rather than
argued.** A private row buys no trace, standing or budget, but it does consume a
sequence number, and `salience::standing` reads recency as a raw sequence
distance that feeds the attention market. So an exchange could in principle
change who speaks next without a word of its content mattering. Two control arms
write the identical rows on the identical schedule and discard every answer:
`hive+quiet` for a round, `hive+hush` for an alongside row. Both are
`+0.0 [+0.0, +0.0]` in every configuration, including at twenty rows an episode.
Every point an exchange arm gains is information. This makes the
sequence-distance decay harmless at these volumes rather than good design — a
host writing far more rows should re-measure, and decaying over desk turns
instead of raw sequences would be its own library change and its own ADR.

**The audit guarantee carries the safety story, and it is weaker than
prevention.** More private bandwidth is more room for covert coordination — the
named mechanism in [Colosseum](https://arxiv.org/abs/2602.15198) and
[Secret Collusion](https://arxiv.org/html/2402.07510v3). Every row is in the one
log, attributed, and readable in full by the operator and by any person, and
non-members see an attributed stub. That is total auditability, not prevention. A
host running participants it does not trust should hold `contact_cap` low, and
the spec says so.

**A disabled policy is the past, exactly.** `ExchangePolicy::DEFAULT` is
disabled, and a disabled policy reproduces every published number to the decimal.
Nothing about an episode that does not exchange changes.

## Related

- [ADR 0002](0002-hive-episodes-are-sequential.md) — one turn per step, which
  this keeps literally true by not adding a variant.
- [ADR 0010](0010-an-aside-carries-information-never-support.md) — an aside
  carries information rather than support, which is what makes a row invisible
  to the fold and therefore free.
- [ADR 0011](0011-an-aside-rides-alongside-a-turn.md) — a row rides alongside a
  turn; this removes the turn as the carrier.
- [`../specs/off-floor-exchange.md`](../specs/off-floor-exchange.md) — the
  behavior.
- [`../experiments/2026-09-07-why-asides-lose.md`](../experiments/2026-09-07-why-asides-lose.md)
  — the measurements that made the case.
