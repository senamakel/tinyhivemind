# Why asides lose, and what peer information is actually worth

**Date:** 2026-09-07
**Status:** Recorded
**Code:** `crates/tinyhivemind-hive/examples/bench` — arms `hive+mute`,
`hive+fact`, `hive+along`, `hive+share`, `hive+fact°`, `hive+rounds`, `hive+pooled`
**Decision:** [ADR 0011](../adr/0011-an-aside-rides-alongside-a-turn.md),
[ADR 0012](../adr/0012-an-exchange-round-spends-model-calls-not-turns.md)
**Follows:** [`2026-09-07-do-asides-help.md`](2026-09-07-do-asides-help.md)

**Peer-to-peer information is worth +9 to +31 points. Spending a floor turn to
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
| `hive+pooled` | the **ceiling** *for equal-weight pooling*: every reading and every fact already in every member's hands, free, averaged with no regard for whose reading it is. No protocol that treats every peer's reading as equally reliable beats it; a protocol that could tell a specialist's reading from a lay guess (`--specialists`, not measured here) could in principle do better by weighting instead of averaging. |

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
| `hive+fact` | 79.6 | 81.6 | 51.1 |
| `hive+mute` | 79.6 | 81.5 | 52.3 |
| `hive+along` | 82.1 | 81.8 | 68.5 |
| `hive+share` | 82.1 | 81.7 | 69.4 |
| `hive+hush` | 82.1 | 81.7 | 68.0 |
| `hive+rounds` | 82.2 | 81.5 | 71.2 |
| `hive+quiet` | 82.1 | 81.7 | 68.0 |
| `hive+fact°` | 80.9 | 81.5 | 71.2 |
| `hive+pooled` | **91.5** | **91.6** | **98.8** |

```text
hidden profile, budget 40
  hive+mute   − hive+:   -15.7 [-17.6, -13.8]     the turns alone
  hive+aside  − hive+:   -15.4 [-17.1, -13.4]
  hive+fact   − hive+:   -16.9 [-18.9, -15.1]
  hive+along  − hive+:    +0.5 [ +0.1,  +0.9]     the same exchange, riding along
  hive+share  − hive+:    +1.4 [ +0.4,  +2.4]     a contact every turn, carrying everything
  hive+hush   − hive+:    +0.0 [ +0.0,  +0.0]     alongside rows, saying nothing
  hive+rounds − hive+:    +3.2 [ +1.8,  +4.5]     the same exchange, off the floor entirely
  hive+quiet  − hive+:    +0.0 [ +0.0,  +0.0]     the same rows, saying nothing
  hive+fact°  − hive+:    +3.2 [ +2.1,  +4.1]     one contact each, off the floor, oracle-aimed
  hive+pooled − hive+:   +30.8 [+28.8, +33.0]     the ceiling

uniform, budget 15
  hive+mute   − hive+:    -2.5 [-3.0, -2.1]
  hive+aside  − hive+:    -2.5 [-3.0, -2.0]
  hive+along  − hive+:    +0.0 [+0.0, +0.1]
  hive+share  − hive+:    +0.0 [-0.1, +0.2]
  hive+fact°  − hive+:    -1.2 [-2.2, -0.2]
  hive+pooled − hive+:    +9.4 [+8.6, +10.4]
```

## What this settles

**The loss is the turns, mostly.** `hive+mute` (`CheckStyle::MUTE`) targets the
same way `hive+aside`/`hive+ask` do — whoever spoke first, `informed: false` —
and against those two it is a clean matched-turn control: same turns, same
words, no transfer, and it loses *more* than both (−15.7 against −15.4 and
−15.5). For those two, taking the answer in is worth a few tenths of a point
against not taking it in — small, and in the *helpful* direction.

`hive+fact` and `hive+aside!` are `informed: true`, aimed at the fact-holder
rather than at whoever spoke first, and *both* lose more than `hive+mute`
(−16.9 and −17.1). Aiming the question, not muting the answer, is what costs
more here: conscripting the one member whose public turn matters into an
audience of one is itself a floor cost, on top of the turns every arm already
pays. So `hive+mute` is not a uniform upper bound on every informative arm's
loss — it bounds the untargeted ones, and the targeted ones cost more still.

**So the pooling explanation is wrong.** The first note argued that averaging a
peer's reading imports the room's shared bias and moves a member toward the
decoy. If that were the mechanism, muting the answer would recover the loss. It
does not; among the untargeted arms it deepens it slightly (`hive+mute` at
−15.7 against −15.4/−15.5 for `hive+aside`/`hive+ask`). Taking the answer in is
worth a few tenths of a point against not taking it in — small, and in the
*helpful* direction. That paragraph of the earlier note is retracted below.

**The information is worth a great deal.** `hive+pooled` beats `hive+` by
**+30.8** on the hidden profile and by **+9.4 to +10.0** on uniform rooms, and
does it in *fewer* turns (10.15 against 10.98). This is the largest effect
anywhere in this benchmark. A room whose members can read each other's private
evaluations is a substantially better room. The intuition that peer-to-peer
exchange should make a hive mind smarter is correct, and this is the size of it.

**Most of the gap is scheduling.** `hive+fact°` runs the same bounded exchange,
with the same payload and the same number of contacts as `hive+fact` — off the
floor rather than on it. It moves from **−16.9 to +3.2**, an interval clear of
zero. Twenty points of the difference between the two is when the exchange
happens.

One caveat on "the same targeting": `hive+fact°` chooses its peer from private
room state — it always reaches the actual fact-holder. `hive+fact` chooses its
peer through `View::grounded_by`, reading only the transcript built so far, and
falls back to the first peer that has spoken when the holder has not deposited
yet. The two are not guaranteed to resolve to the same peer on a given episode,
so `hive+fact°` is better read as an upper bound on what the exchange is worth
off the floor with at-least-as-good targeting, not as a comparison that varies
scheduling alone. Matching the two exactly would mean deriving `hive+fact°`'s
peer from the transcript a live episode would have produced by that point,
which this benchmark does not attempt because `pre_checked` runs before any
transcript exists.

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
still counts nothing from an aside. A fact-bearing answer replaces the
reading rather than adding to it (a second defect, caught the same way: the
first cut of `CheckStyle::FACT` recorded the discount and still averaged in
the number that came with it, so `hive+fact` was measuring both channels at
once). Corrected, `hive+fact` (−16.9) and the aimed reading-only arm,
`hive+aside!` (−17.1), are within their intervals of each other — carrying
the fact instead of a number does not measurably change what the aimed check
is worth, and neither comes close to paying for the turn.

## The fix, and what it recovered

The mechanism was never limiting the room; the accounting was. So the accounting
changed: **an aside now rides alongside the turn that authored it.** One
authorized turn produces the member's ordinary desk-visible move *and* one
private row. It is not a second turn and cannot become one — the peer answers on
its own next turn, which the attention market was going to give it — and the
episode cannot vote the row, because `live_traces` drops a non-desk row before
it reaches a trace or a standing, and `spent` counts turns rather than rows.
It is not free of a sequence, though: see "What this does not settle" below.

That property is now pinned twice: a readable case in `episode::test`, and an
*addition* invariant in the fuzz suite — arbitrary aside rows, carrying the same
fuzzed grammar as the desk rows, interleaved anywhere in an arbitrary
transcript, leave `step` exactly where it was. Both go red if the audience
filter is removed.

`hive+along` is the same check as `hive+fact` — same words, same targeting, same
bound — differing only in that the room is not charged for it. It moves from
`-16.9 [-18.9, -15.1]` to `+0.5 [+0.1, +0.9]`. **A 17.4-point swing, bought by
changing nothing about the exchange itself.** On uniform rooms the −2.5 becomes
±0.0: the move stops being a tax everywhere.

## What a free row makes affordable

One question when you cannot separate two options is the shape a *charged* row
forces. A colony's contacts are continuous and carry whatever the donor holds.
`hive+share` is that: a contact on every turn, to a peer not yet reached,
handing over a reading of every option and any fact its author holds. It gains
`+1.4 [+0.4, +2.4]` on the hidden profile — much of the way to the `+3.2` that an
off-floor exchange with oracle targeting reaches.

## Taking the floor out of it entirely

`hive+share` still rides on turns, so a room converging in eleven of them writes
at most eleven private rows. [ADR 0012](../adr/0012-an-exchange-round-spends-model-calls-not-turns.md)
removes that carrier: between turns, a host may run an **exchange round** in
which each member the library names appends one private row, taking no floor and
producing no turn. The volume is then set by a policy the host writes rather than
by how talkative the room happens to be.

`hive+rounds` reaches **`+3.2 [+1.8, +4.5]`** on the hidden profile, matching
what one oracle-aimed contact each reaches off the floor (`hive+fact°`, `+3.2`)
— but reaching it from the transcript rather than from private room state, which
is the version a host could actually run.

**It is not free, and the table says so.** A round is *n* model calls, and the
`private/ep` column exists so the price sits beside the gain rather than inside
`cost/ep`, which is defined as each speaker's own cost times its turns. The curve
is the honest way to read the mechanism:

| `--exchange-cap` | correct % | private rows/ep |
| --- | --- | --- |
| 0 (off) | 68.0 | — |
| 1 | 69.5 | 5.0 |
| 2 | 69.2 | 10.0 |
| 4 (default) | **71.2** | 20.0 |
| 8 | 72.4 | 27.8 |
| 16 | 72.4 | 27.8 |

It saturates: at cap 8 a member runs out of peers to contact before it runs out
of budget, and rows stop being written at all rather than being written and
wasted. Twenty rows to buy three points is a real trade a host can now make
deliberately, and one it could not price before.

One participant-side detail is load-bearing and was measured rather than
assumed. A round during the blind phase shows a member no peer at all, so
anything written then sits unread until blindness lifts — and a member that
spends its whole cap there has none left for the turns that could use it.
Declining a round it cannot use is most of the mechanism: without it the arm
spends its whole cap writing into a blind round and lands on `hive+`.

**Uniform rooms are still a null**: `+0.2 [-0.1, +0.4]` at budget 15 and
`-0.1 [-0.5, +0.3]` at budget 40. Every exchange arm is worthless there, and for
the same reason — a room whose members differ only by independent noise has no
concentrated information for a contact to move, and the fold across five members
already does the averaging.

## Does writing a private row move the room by itself?

A private row buys no trace, no standing and no budget — that is pinned by two
tests and a fuzz invariant. But it does consume a **sequence number**, and
`salience::standing` reads recency as a raw `at - trace.sequence` distance which
feeds the attention market's choice of who speaks next. So a room that writes
private rows could in principle decide differently from one that does not,
without a word of their content mattering. Review of
[ADR 0011](../adr/0011-an-aside-rides-alongside-a-turn.md) raised this, and the
honest answer was that the invariant never covered it: it holds desk sequence
numbers fixed across the two transcripts it compares.

So it is measured. Two arms write the identical rows on the identical schedule
and **throw every answer away**:

| arm | what it controls | hidden profile, budget 40 |
| --- | --- | --- |
| `hive+hush` | `hive+share`'s rows, saying nothing | `+0.0 [+0.0, +0.0]` |
| `hive+quiet` | `hive+rounds`' rows, saying nothing | `+0.0 [+0.0, +0.0]` |

Zero, exactly, in every configuration — including `hive+quiet`, which writes
twenty rows an episode. The concern is a real mechanism and an empty one here:
**every point any exchange arm gains is information, not perturbation.**

The reason differs between the two, which is why both were run. An exchange
round appends the same number of rows after every turn, so every inter-turn gap
grows by the same constant and the *ranking* salience produces is untouched. An
alongside row lands only on turns whose author wanted one, so it stretches gaps
unevenly — the case with no such argument available, and the one that had to be
measured rather than reasoned about. It too is zero.

This does not make the sequence-distance decay a good design; it makes it
harmless at these volumes. A host writing far more private rows than this
benchmark does should re-measure rather than assume, and the real fix — decay
over desk turns rather than raw sequences — remains a library change that would
need its own ADR.

## What is still unreached, and why it is not the accounting

The ceiling is `+30.8` and the best off-floor arm reaches `+3.2`, at twenty
private rows an episode. What is left is not the accounting and not the carrier
— both of those have now been removed — but **reach and timing**. `pooled` puts
every member's reading of every option into every other member before a word is
spoken; `hive+rounds` diffuses the same information pairwise, mid-episode, after
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
one), and rooms whose members are wrong in *different* directions. And the gap between the
best off-floor arm and the `+30.8` ceiling, which is reach rather than
accounting: no arm here contacts more than one peer per round, or exchanges
before the room opens. Note also that `hive+fact°`'s peer selection is an oracle rather
than a transcript-matched replica of `hive+fact`'s, so its `+3.2` is an upper
bound on the off-floor benefit rather than an isolation of scheduling from
targeting alone — `hive+along`, which *is* transcript-matched, is the clean
comparison.

Also unsettled: `hive+along` and `hive+share` are not insulated from an aside's
*presence*, only from its content counting as a vote. Sequence numbers are
unique across the one shared journal (the runtime crate rejects a duplicate),
so each alongside row still takes the next one, and every later desk turn in
those two arms therefore lands at a higher raw sequence than the same episode
without the aside would have reached. `salience::standing` scores recency from
that raw `at - trace.sequence` distance, and salience feeds the floor-holder
choice, so a run that fires more asides reaches any given desk-turn count at a
larger sequence, which very slightly speeds decay of the room's own older
traces relative to a no-aside control. The fuzz invariant in `episode::test`
and `tests/fuzz_invariants.rs` holds desk sequence numbers fixed across the
transcripts it compares, so it correctly proves an aside cannot buy a vote —
it was never a claim about live per-episode sequence numbering, and does not
cover this. The `+0.5` and `+1.4` above are real measurements of the code as it
runs today, confound included; this note records the confound rather than
correcting for it, because the size and direction of its effect on the
reported gain have not been isolated. See the "Known limitation" note on
[ADR 0011](../adr/0011-an-aside-rides-alongside-a-turn.md).

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
