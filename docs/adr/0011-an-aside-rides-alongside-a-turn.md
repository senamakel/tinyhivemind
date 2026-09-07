# 11. An aside rides alongside the turn that authored it rather than spending one

- **Status:** Proposed
- **Date:** 2026-09-07

## Context

[ADR 0010](0010-an-aside-carries-information-never-support.md) established what
an aside *is*: a row two members can read and the desk cannot, carrying
information and never support. It left open what an aside **costs**, and the
default answer — the only one the harness implemented — was that it costs a
turn, because every message in an episode is written by whoever holds the floor.

The benchmark then measured that default, and it is expensive. On a hidden
profile at a turn budget nothing exhausts, a room whose members may open one
pairwise check decides **15.4 points worse** than the same room without the
move. Three controls locate the cost precisely
([`../experiments/2026-09-07-why-asides-lose.md`](../experiments/2026-09-07-why-asides-lose.md)):

- `hive+mute` writes the identical words on the identical turns and **throws the
  answer away**. It loses `-15.7 [-17.6, -13.8]` — *more* than the arms that
  keep the answer. The transfer is not what costs the room anything.
- `hive+pooled` hands every member every peer's reading and every fact for free.
  It gains `+30.8 [+28.8, +33.0]`, in fewer turns. The information is worth more
  than anything else this benchmark measures.
- `hive+fact°` runs the same bounded exchange off the floor. It gains
  `+3.2 [+2.1, +4.1]`.

So the mechanism was never the problem. Under one message, one turn, a member
asking a peer is a member not depositing, not objecting and not refuting, while
the rest of the room goes on accumulating support for the option it stepped away
to ask about. The room reaches quorum on the decoy while its members are away
asking about it.

The biology this workspace already cites says the same thing and says it first.
Trophallaxis costs no foraging trip, a waggle dance is watched by bees already in
the nest, and quorum sensing is a concentration read off the air. A colony's
pairwise exchange is not subtracted from its capacity to act. An aside on the
floor is.

## Decision

**An aside may ride alongside the turn that authored it.** One authorized turn
produces the member's ordinary desk-visible contribution *and*, optionally, one
aside row. The aside is not a turn, does not become one, and is not charged as
one.

Three properties make this sound, and all three are already true of the algebra
rather than newly asserted by it:

1. **The episode cannot see it.** `live_traces` drops a non-desk row before it
   reaches the traces, the standings, the sequence they fold at, the directory or
   the floor, and `EpisodeState::spent` counts turns rather than rows. `step` is
   therefore invariant under the *addition* of aside rows anywhere in a
   transcript — a strictly stronger property than the idempotence under
   redelivery and reordering the fuzz suite already asserted, and now asserted
   beside it.
2. **It starts no turn.** The peer answers on its own next turn, which the
   attention market was going to give it anyway. A host must not hand an aside
   row to `mention_dispatch` inside an episode; the exchange completes over the
   participants' own turns or it does not complete.
3. **It is bounded by the floor it rides on.** At most one aside row per turn, so
   a room of *n* members writes at most *n* aside rows per round, and every one
   of them cost its author a turn it had already won. There is no arrangement of
   asides that outruns the room.

This is **not** the concurrency [ADR 0002](0002-hive-episodes-are-sequential.md)
rules out. `HiveStep::Speak` still carries exactly one turn, there is still no
variant carrying two, and no two participants ever hold the floor. What rides
alongside a turn is a *row*, not a turn. The charter's one-message-one-turn rule
is a bound on how many turns a message may start, and an aside starts none.

## Consequences

The measured effect of the change, at 2000 rooms and a paired bootstrap against
the same rooms without the move:

| configuration | on the floor | riding alongside |
| --- | --- | --- |
| hidden profile, budget 40 | `-16.9 [-18.9, -15.1]` | `+0.5 [+0.1, +0.9]` |
| uniform, budget 15 | `-2.5 [-3.0, -2.0]` | `+0.0 [+0.0, +0.1]` |
| uniform, budget 40 | `-0.1 [-0.6, +0.5]` | `+0.1 [+0.0, +0.3]` |

A 17.4-point swing on the hidden profile, bought by changing nothing about who
reads the exchange, what it says, or who it is aimed at. The move stops being a
tax and becomes free.

**A free row makes continuous exchange affordable, and that is worth more than
one question.** Once a contact costs no turn, a member can contact a peer on
every turn rather than once when it cannot separate two options, and hand over a
reading of every option rather than an answer to one. That arm gains
`+1.4 [+0.4, +2.4]` on the hidden profile — much of the way to the `+3.2` an
off-floor exchange with oracle targeting reaches, and the shape a colony's
contacts actually have.

**What remains unreached is not the accounting.** The ceiling is `+30.8` and the
best on-contract arm reaches `+1.4`, because an aside still rides on a turn: a
room converging in 11 turns can write at most 11 aside rows, which is roughly two
contacts per member in a room of five. Closing that would mean letting members
the library did **not** authorize append rows between turns. The invariant above
says `step` could not tell — but "the library cannot see it" is not "the design
endorses it", and a host running *n* participants per authorized turn is the
failure mode the charter's third rule exists to prevent. That is left as an open
question in the spec rather than taken here, and `hive+pooled` bounds what it
could be worth.

**Blindness is not the barrier, and was tested rather than assumed.** Admitting
an aside addressed to the viewer through the blind round's filter — so an
exchange can begin before positions form — is worth `+0.5`, well inside the noise
of the arms above. It would put a channel the library cannot inspect through the
one filter [ADR 0005](0005-a-blind-round-may-be-concurrent.md) measures 24 points
of value on, and it does not pay for that risk. `project_for` is unchanged.

**The host contract gains one sentence and no new type.** A host that appends
only the turn it was authorized behaves exactly as before; nothing in the
library's public surface changes, and no policy field was added. What changed is
that the spec now says the second row is allowed, and two tests say the episode
cannot tell.

## Related

- [ADR 0002](0002-hive-episodes-are-sequential.md) — one turn per step; this
  adds a row to a turn, never a turn to a step.
- [ADR 0005](0005-a-blind-round-may-be-concurrent.md) — the blind round, and why
  an aside is not let through it.
- [ADR 0010](0010-an-aside-carries-information-never-support.md) — what an aside
  carries, which is what makes it invisible to the fold and therefore free.
- [`../specs/private-asides.md`](../specs/private-asides.md) — the behavior.
- [`../experiments/2026-09-07-why-asides-lose.md`](../experiments/2026-09-07-why-asides-lose.md)
  — the measurements above.
