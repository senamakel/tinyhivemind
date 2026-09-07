# Why asides lose, and what peer information is actually worth

**Date:** 2026-09-07
**Status:** Recorded
**Code:** `crates/tinyhivemind-hive/examples/bench` — arms `hive+mute`,
`hive+fact`, `hive+along`, `hive+share`, `hive+fact°`, `hive+rounds`, `hive+pooled`
**Decision:** [ADR 0011](../adr/0011-an-aside-rides-alongside-a-turn.md),
[ADR 0012](../adr/0012-an-exchange-round-spends-model-calls-not-turns.md)
**Follows:** [`2026-09-07-do-asides-help.md`](2026-09-07-do-asides-help.md)

**Peer-to-peer information is worth +9 to +21 points. Spending a floor turn to
buy it costs more than it is worth. The loss was never the privacy, the
payload, or the peer — it was the scheduling.**

## What was asked

[The first note](2026-09-07-do-asides-help.md) recorded a loss and explained it
as a *pooling* effect: members of a hidden-profile room are wrong in the same
direction, so averaging a peer's reading imports shared bias. That explanation
predicts that removing the information transfer would *help*.

It does not. This note runs the controls that first note listed as missing, and
the explanation does not survive them.

## The arms this adds

| arm | what it is |
| --- | --- |
| `hive+mute` | the identical check on the identical turns, with the answer **discarded**. What it loses against `hive+` is what the turns cost. |
| `hive+fact` | the aimed check, carrying the fact that rules an option out rather than a number to be averaged. |
| `hive+along` | the aimed, fact-carrying check again, **riding alongside** the member's floor move: one turn, two rows, the second of which the episode cannot see. |
| `hive+share` | the same free row spent **continuously** — a contact on every turn, carrying a reading of every option rather than an answer to one. |
| `hive+rounds` | the same continuous exchange run **off the floor**, in rounds between turns: nobody takes the floor for it, so its volume is set by a policy rather than by how many turns the room takes. |
| `hive+fact°` | the same bounded exchange, held **off the floor** — before the episode opens, spending no turn the room could have deliberated with. |
| `hive+pooled` | the **ceiling**: every reading and every fact already in every member's hands, free. No protocol beats it. |

`--aside-cap 0` leaves all four bit-identical to `hive+`, and every number in
the published six-row table is unchanged to the decimal.

## What happened

Paired bootstrap against `hive+` over the same rooms, 2000 resamples.

| arm | uniform, b15, 5000 | uniform, b40, 2000 | hidden profile, b40, 2000 |
| --- | --- | --- | --- |
| `hive+` | **82.1** | **81.7** | **68.0** |
| `hive+aside` | 79.6 | 81.6 | 52.6 |
| `hive+ask` | 79.7 | 81.8 | 52.5 |
| `hive+aside!` | 79.6 | 81.6 | 50.9 |
| `hive+fact` | 79.6 | 81.6 | 53.0 |
| `hive+mute` | 79.6 | 81.5 | 52.3 |
| `hive+along` | 82.1 | 81.8 | 68.5 |
| `hive+share` | 82.1 | 81.7 | 69.9 |
| `hive+rounds` | 82.2 | 81.5 | 72.9 |
| `hive+fact°` | 80.9 | 81.5 | 71.0 |
| `hive+pooled` | **91.5** | **91.6** | **89.2** |

```text
hidden profile, budget 40
  hive+mute   − hive+:   -15.7 [-17.6, -13.8]     the turns alone
  hive+aside  − hive+:   -15.4 [-17.1, -13.4]
  hive+fact   − hive+:   -15.0 [-16.9, -13.1]
  hive+along  − hive+:    +0.5 [ +0.1,  +1.0]     the same exchange, riding along
  hive+share  − hive+:    +1.9 [ +0.8,  +3.0]     a contact every turn, carrying everything
  hive+rounds − hive+:    +4.9 [ +3.5,  +6.2]     the same exchange, off the floor entirely
  hive+fact°  − hive+:    +3.0 [ +2.1,  +4.0]     one contact each, off the floor, oracle-aimed
  hive+pooled − hive+:   +21.2 [+19.3, +23.3]     the ceiling

uniform, budget 15
  hive+mute   − hive+:    -2.5 [-3.0, -2.1]
  hive+aside  − hive+:    -2.5 [-3.0, -2.0]
  hive+along  − hive+:    +0.0 [+0.0, +0.1]
  hive+share  − hive+:    +0.0 [-0.1, +0.2]
  hive+fact°  − hive+:    -1.2 [-2.2, -0.2]
  hive+pooled − hive+:    +9.4 [+8.6, +10.4]
```

## What this settles

**The loss is the turns, entirely.** `hive+mute` transfers nothing and loses
*more* than the arms that transfer something (−15.7 against −15.0 for
`hive+fact`). Every informative arm sits inside a point of the mute control in
every configuration. Whatever an aside costs, it costs it before a single word
of content has changed hands.

**So the pooling explanation is wrong.** The first note argued that averaging a
peer's reading imports the room's shared bias and moves a member toward the
decoy. If that were the mechanism, muting the answer would recover the loss. It
does not; it deepens it slightly. Taking the answer in is worth about +0.7
points against not taking it in — small, and in the *helpful* direction. That
paragraph of the earlier note is retracted below.

**The information is worth a great deal.** `hive+pooled` beats `hive+` by
**+21.2** on the hidden profile and by **+9.4 to +10.0** on uniform rooms, and
does it in *fewer* turns (10.22 against 10.98). This is the largest effect
anywhere in this benchmark. A room whose members can read each other's private
evaluations is a substantially better room. The intuition that peer-to-peer
exchange should make a hive mind smarter is correct, and this is the size of it.

**The whole gap is scheduling.** `hive+fact°` runs the *same* bounded exchange,
with the same payload, the same targeting and the same number of contacts as
`hive+fact` — and differs only in that it does not spend a floor turn. It moves
from **−15.0 to +3.0**, an interval clear of zero. Eighteen points of the
difference between the two is nothing but when the exchange happens.

## Why, in one paragraph

A colony's advantage is not that ants talk to each other; it is that they talk
to each other **while foraging**. Trophallaxis costs no trip, the waggle dance
is watched by whoever is already in the nest, and quorum sensing is a
concentration read off the air. None of it is subtracted from the colony's
capacity to act.

An aside on the floor is. Under one message, one turn, a member asking a peer
is a member not depositing, not objecting, and not refuting — while the rest of
the room goes on accumulating support for the option the asker is asking about.
The room reaches quorum on the decoy while its members are away asking about it.
That is why more contacts make it monotonically worse (`--aside-cap` 1→5 takes
the hidden-profile arm from 54.2 to 43.8) even at a budget nothing exhausts: the
harm is not running out of turns, it is *conceding the floor* while the room
commits.

## Two defects found on the way

**The informed arm was not aiming.** `View::grounded_by` returned the first
`!evidence` author on the topic. Under `--blind-evidence` every lay member opens
by depositing its reading of whatever it rates highest, which under a hidden
profile is the planted decoy — so four uninformed deposits and one refutation
land on the same topic and the refutation is rarely first. Instrumented, the
"informed" check reached the one member who knows something **22%** of the time
against **25%** for drawing a peer at random. The first note's answer to Q3 —
"aiming it changes nothing" — was measuring an arm that never aimed. It now
prefers a depositor who argues *against* the topic. With the fix the arm
reaches the fact-holder reliably and gets **worse** (−17.1), for a reason worth
stating plainly: a well-aimed private question conscripts the one member whose
public turn matters into an audience of one.

**An aside could not carry a fact.** The room's public grammar has always
carried refutations in `!evidence`; an answered check could only ever hand back
a scalar. `CheckStyle::FACT` lets a member holding the fact say so, and the
asker discounts the option for itself at the same weight a desk-visible
refutation carries in `View::posterior` — a belief, not a trace, so the room
still counts nothing from an aside. It is worth +2.1 against the aimed
reading-only arm and does not come close to paying for the turn.

## The fix, and what it recovered

The mechanism was never limiting the room; the accounting was. So the accounting
changed: **an aside now rides alongside the turn that authored it.** One
authorized turn produces the member's ordinary desk-visible move *and* one
private row. It is not a second turn and cannot become one — the peer answers on
its own next turn, which the attention market was going to give it — and the
episode cannot tell the row is there, because `live_traces` drops a non-desk row
before it reaches a trace, a standing, the sequence they fold at, or the floor,
and `spent` counts turns rather than rows.

That property is now pinned twice: a readable case in `episode::test`, and an
*addition* invariant in the fuzz suite — arbitrary aside rows, carrying the same
fuzzed grammar as the desk rows, interleaved anywhere in an arbitrary
transcript, leave `step` exactly where it was. Both go red if the audience
filter is removed.

`hive+along` is the same check as `hive+fact` — same words, same targeting, same
bound — differing only in that the room is not charged for it. It moves from
`-15.0 [-16.9, -13.1]` to `+0.5 [+0.1, +1.0]`. **A 15.5-point swing, bought by
changing nothing about the exchange itself.** On uniform rooms the −2.5 becomes
±0.0: the move stops being a tax everywhere.

## What a free row makes affordable

One question when you cannot separate two options is the shape a *charged* row
forces. A colony's contacts are continuous and carry whatever the donor holds.
`hive+share` is that: a contact on every turn, to a peer not yet reached,
handing over a reading of every option and any fact its author holds. It gains
`+1.9 [+0.8, +3.0]` on the hidden profile — most of the way to the `+3.0` that an
off-floor exchange with oracle targeting reaches.

## Taking the floor out of it entirely

`hive+share` still rides on turns, so a room converging in eleven of them writes
at most eleven private rows. [ADR 0012](../adr/0012-an-exchange-round-spends-model-calls-not-turns.md)
removes that carrier: between turns, a host may run an **exchange round** in
which each member the library names appends one private row, taking no floor and
producing no turn. The volume is then set by a policy the host writes rather than
by how talkative the room happens to be.

`hive+rounds` reaches **`+4.9 [+3.5, +6.2]`** on the hidden profile — past the
`+3.0` that one oracle-aimed contact each reaches off the floor, because it is
continuous rather than a single shot.

**It is not free, and the table says so.** A round is *n* model calls, and the
`private/ep` column exists so the price sits beside the gain rather than inside
`cost/ep`, which is defined as each speaker's own cost times its turns. The curve
is the honest way to read the mechanism:

| `--exchange-cap` | correct % | private rows/ep |
| --- | --- | --- |
| 0 (off) | 68.0 | — |
| 1 | 69.8 | 5.0 |
| 2 | 69.7 | 10.0 |
| 4 (default) | **72.9** | 20.0 |
| 8 | 74.2 | 27.9 |
| 16 | 74.2 | 27.9 |

It saturates: at cap 8 a member runs out of peers to contact before it runs out
of budget, and rows stop being written at all rather than being written and
wasted. Twenty rows to buy five points is a real trade a host can now make
deliberately.

One participant-side detail is load-bearing and was measured rather than
assumed. A round during the blind phase shows a member no peer at all, so
anything written then sits unread until blindness lifts — and a member that
spends its whole cap there has none left for the turns that could use it.
Declining a round it cannot use takes the arm from 69.2 to 74.6 at cap 4 (at 500
rooms), which is most of the mechanism.

**Uniform rooms are still a null**: `+0.2 [-0.1, +0.4]` at budget 15 and
`-0.1 [-0.5, +0.3]` at budget 40. Every exchange arm is worthless there, and for
the same reason — a room whose members differ only by independent noise has no
concentrated information for a contact to move, and the fold across five members
already does the averaging.

## What is still unreached, and why it is not the accounting

The ceiling is `+21.2` and the best arm reaches `+4.9`, at twenty private rows an
episode. What is left is not the accounting and not the carrier — both of those
have now been removed — but **reach and timing**. `pooled` puts every member's
reading of every option into every other member before a word is spoken;
`hive+rounds` diffuses the same information pairwise, mid-episode, after
positions have begun to form, and saturates once each member has met each peer
once.

Two things would narrow it and neither is free. Contacting more than one peer per
round multiplies the model calls again. Starting the exchange before the room
opens runs into the blind round, which withholds peer rows whatever their
audience — admitting asides through that filter was measured at `+0.5` and
declined below.

**Blindness was tested and is not the barrier.** Half of a hidden-profile episode
is the blind round, during which `project_for` withholds every peer row, so an
exchange cannot begin until positions have already formed — an obvious suspect
for the missing points. Admitting an aside addressed to the viewer through that
filter is worth `+0.5`, inside the noise of the arms above. It would put a
channel the library cannot inspect through the one filter
[ADR 0005](../adr/0005-a-blind-round-may-be-concurrent.md) measures 24 points of
value on, and it does not pay for that risk. `project_for` is unchanged.

## What this does not settle

Everything the first note listed, still: conformity (arithmetic participants
cannot be sycophantic), the settlement pointer (no simulated participant acts on
one), and rooms whose members are wrong in *different* directions. And the gap between `+4.9` and the
`+21.2` ceiling, which is reach rather than accounting: no arm here contacts
more than one peer per round, or exchanges before the room opens.

## Retraction

The section "The hidden-profile result, and why it is worse than a wash" in
[`2026-09-07-do-asides-help.md`](2026-09-07-do-asides-help.md) attributes the
loss to importing correlated bias. `hive+mute` refutes it: with the transfer
removed entirely the loss is `-15.7 [-17.6, -13.8]`, marginally worse than with
it. The claim that pooling helps across a correlation boundary and hurts within
one may still be true; it is simply not what happened here, and this benchmark
does not evidence it. That note now carries a pointer to this one.

## Reproducing

```sh
B="cargo run --release -p tinyhivemind-hive --example bench --"
$B --episodes 5000
$B --episodes 2000 --budget 40
$B --episodes 2000 --budget 40 --hidden-profile --blind-evidence
$B --episodes 500  --budget 40 --hidden-profile --blind-evidence --aside-cap 5
$B --episodes 2000 --budget 40 --hidden-profile --blind-evidence --exchange-cap 8
$B --episodes 500  --budget 40 --hidden-profile --blind-evidence --aside-cap 0 --exchange-cap 0  # every arm identical to hive+
```
