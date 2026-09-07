# Off-floor exchange

**Status:** Draft
**Owner:** tinyhivemind maintainers

## Problem

An aside is free but starved.

[ADR 0011](../adr/0011-an-aside-rides-alongside-a-turn.md) let a private row ride
alongside the turn that authored it, so a member no longer pays a floor turn to
ask a peer anything. That removed the whole of the cost — a 15.5-point swing on
a hidden profile — and it left the exchange bounded by the wrong thing. An aside
still *rides* on a turn, so a room that converges in eleven turns can write at
most eleven private rows, which in a room of five is about two contacts each.

The benchmark says that is nowhere near enough
([`../experiments/2026-09-07-why-asides-lose.md`](../experiments/2026-09-07-why-asides-lose.md)):

| arm | hidden profile, budget 40 |
| --- | --- |
| `hive+along` — one row per authorized turn | `+0.5 [+0.1, +1.0]` |
| `hive+share` — the same row spent continuously | `+1.9 [+0.8, +3.0]` |
| `hive+pooled` — every reading and every fact, free | `+21.2 [+19.3, +23.3]` |

Twenty-one points of peer information exist and a protocol tied to the floor
reaches two of them. The gap is not privacy, not payload, not targeting and not
accounting — every one of those was isolated and measured. It is **bandwidth**,
and bandwidth is capped because a private row may only be written by whoever the
attention market happened to authorize.

The biology says the same. A colony's contacts are not rationed by whose turn it
is to forage. Trophallaxis happens continuously and in parallel with the work,
and that is precisely why a colony's information is *pooled* rather than
sampled.

## Goals

- Let members exchange privately **without any of them taking the floor**, so
  the volume of exchange is set by what a host will pay for rather than by how
  many turns the deliberation happens to take.
- Keep the guarantee that makes this safe **provable rather than asserted**: the
  episode's decision, floor, standings, phase and budget must be bit-identical
  whether an exchange happened or not.
- Bound the spend with an explicit, finite, host-set ceiling that is knowable
  **before** the episode runs.
- Make the price **visible**. An exchange round is *n* model calls; a benchmark
  or a host that hides that is lying about what the mechanism costs.
- Keep every row auditable in the one log, on the terms
  [`private-asides.md`](private-asides.md) already sets.

## Non-goals

- **A second journal.** Spend is folded from the transcript the host already
  holds — private rows above the watermark, counted by author. Nothing is
  stored, so nothing can disagree with the log.
- **A port, or any waiting.** The library decides *whether and how many*; the
  host does the calling. `exchange` is a fold over arguments the caller already
  has, like every other decision in this crate.
- **A second floor.** An exchange round moves no standing, resolves no counting
  trace and settles nothing. To make a private finding count, a member still
  spends a desk-visible turn saying so in the open.
- **Relaxing one message, one turn.** No exchange row starts a turn. See below.
- **Deciding who talks to whom.** Targeting is a participant's judgement. The
  library authorizes and bounds; it does not choose a peer.
- **Making an exchange mandatory, or on by default.** `ExchangePolicy::DEFAULT`
  is disabled.

## Proposed behavior

### An exchange round

Between two authorized turns, or before the first one, a host may run an
**exchange round**. Never *during* a turn: a round and a turn do not interleave,
so the transcript a turn was composed from is never edited underneath it. In a round, each eligible member may append at most one private row. No
member takes the floor, no `HiveStep` is produced, and `EpisodeState` does not
advance.

The host asks the library whether a round is open:

```rust
pub fn exchange(
    policy: &ExchangePolicy,
    state: &EpisodeState,
    transcript: &[SessionMessage],
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
) -> Result<ExchangeRound>;

pub enum ExchangeRound {
    /// Each of these members may append at most one private row now.
    Open {
        members: Vec<String>,
        /// Rows this episode may still write, across every member.
        remaining: u32,
    },
    /// No round, and why.
    Closed { reason: NoExchangeReason },
}
```

`members` is in desk order and contains only members that are active on the
episode's desk and still under their own contact cap. A host runs zero or more
of them, in any order, and may run none.

### The budget, and where it lives

```rust
pub struct ExchangePolicy {
    /// Off unless the host turns it on.
    pub enabled: bool,
    /// Private rows one member may author across the whole episode.
    pub contact_cap: u32,
    /// Rounds the episode may open at all.
    pub round_cap: u32,
}
```

Two independent ceilings, both finite, both known before the episode starts. The
total number of private rows an episode can produce is at most

```text
min(members × contact_cap, round_cap × members)
```

and a host reads its worst case off the policy rather than discovering it.

**Spend is folded, never stored.** A member's contacts are the private rows
above the episode's watermark that it authored. There is no counter to keep
consistent with the log, which is the same reason `directory` and `standings`
fold rather than accumulate.

`enabled: false`, `contact_cap: 0` or `round_cap: 0` all close every round, and
a `Closed` round names which. Zero is a configuration a host may hold
deliberately, so unlike `defer_cap` it is not an error.

### What an exchange round may not do

Four prohibitions, and the first three are already properties of the algebra
rather than new promises:

1. **It may not move the episode.** A private row is dropped by `live_traces`
   before it reaches a trace, a standing, the sequence they fold at, the
   directory or the floor, and `EpisodeState::spent` counts turns rather than
   rows. `step` is therefore invariant under the *addition* of private rows.
2. **It may not start a turn.** A host does not hand a row authored in an
   exchange round to `mention_dispatch`. The exchange cannot cascade: rows do
   not beget rows, and the only thing that produces more of them is the next
   round, which the round cap bounds.
3. **It may not carry support.** [ADR 0010](../adr/0010-an-aside-carries-information-never-support.md):
   an aside carries information, never support, uniformly for every reader.
4. **It may not be written by a member the round did not name.** A row from
   anyone else is the host exceeding its authorization, and the fold that
   counts spend will see it on the next round.

### Why this does not relax one message, one turn

The charter's third rule names its own failure mode: *"a mention that could
start N turns without an approval in sight"*. Each clause fails to apply here,
and it is worth being explicit rather than gesturing at the difference.

- **N turns.** An exchange round starts none. No member gains the floor, no
  `HiveStep::Speak` is produced, and the deliberation's turn budget is untouched.
  What rides in a round is *n* rows, and a row is not a turn — which is exactly
  the distinction ADR 0011 established and two tests pin.
- **Could start.** The danger in rule 3 is unboundedness: `@everyone` mentions N
  agents, each of whom may mention N more. An exchange round cannot cascade,
  because a row is never dispatched. The worst case is a constant a host
  computes from its own policy.
- **Without an approval in sight.** The approval is the policy. It is off by
  default, it is set by the host and not by a participant, and it is finite.

What the rule protects is the **floor** — the shared, ordered record in which
one message causes one agent to run and the room's standing moves. An exchange
round touches none of that. What it does spend is model calls, which is a real
cost and a host's to authorize, and the reason the ceilings above are explicit
rather than implied.

This is also not the concurrency
[ADR 0002](../adr/0002-hive-episodes-are-sequential.md) rules out. That decision
is about the *episode*: `HiveStep::Speak` carries exactly one turn and there is
no variant carrying two. `exchange` is deliberately a separate fold rather than a
`HiveStep` variant, so the episode state machine keeps that property literally
and visibly.

### What a host does

```text
loop:
    step(...) -> Speak { turn }
    run that one turn, append it, commit turn.next_state
    exchange(...) -> Open { members, .. }
        for each member the round named:
            ask it for one private line, or nothing
            validate the audience through `aside(..)`
            append it, or drop it if the audience is not private
```

An aside whose audience the `aside` fold refuses is **dropped, not published**.
Falling back to the desk would turn a private row into a second desk-visible
contribution nobody was authorized to make.

## Invariants and constraints

- `step` returns the same `HiveStep`, including `next_state`, for a transcript
  with and without any number of private rows interleaved anywhere.
- An episode's total private rows never exceed the policy's computed worst case.
- A member never authors more than `contact_cap` private rows in one episode.
- `ExchangePolicy::DEFAULT` is disabled, and a disabled policy makes every
  projection, decision and turn identical to one taken before this spec existed.
- A `Closed` round names a reason; there is no silent no-op.
- Every row remains readable in full by the operator and by any person, and
  visible as an attributed stub to every non-member, exactly as
  [`private-asides.md`](private-asides.md) requires.

## Acceptance criteria

- A benchmark arm that runs exchange rounds reports its rows in a column of its
  own, so the cost is on the same page as the gain.
- The mechanism is allowed to lose, and a loss is published.
- The `step`-invariance property is asserted by a fuzz test over arbitrary
  transcripts, not only by a hand-written case, and fails if the audience filter
  is removed.
- Turning the policy off reproduces every published number to the decimal.

## Open questions

- **Collusion surface.** More private bandwidth is more room for covert
  coordination, which is the named mechanism in
  [Colosseum](https://arxiv.org/abs/2602.15198) and
  [Secret Collusion](https://arxiv.org/html/2402.07510v3). The mitigation here is
  total audit rather than restriction — every row is in the one log and readable
  in full by any person — and that is a weaker guarantee than prevention. A host
  running untrusted participants should hold `contact_cap` low.
- **Context pressure on non-members.** A round can add one stub per member per
  round. Consecutive stubs from one aside collapse, but stubs from *different*
  asides do not, so a heavily exchanging room costs a non-member more rows than
  a lightly exchanging one. Not addressed here beyond the existing collapse.
- **A round before the first turn is allowed, and the blind round blunts it.**
  `project_for` withholds every peer row under `Visibility::Blind` whatever its
  audience, so rows written before the room opens are not readable by their
  recipients until blindness lifts — they arrive in a batch rather than early.
  Admitting asides through that filter was measured at `+0.5` and declined
  ([ADR 0011](../adr/0011-an-aside-rides-alongside-a-turn.md)), so the
  interaction stands as a known limit rather than a defect.
