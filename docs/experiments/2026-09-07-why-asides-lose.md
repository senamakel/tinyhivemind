# Why asides lose, and what peer information is actually worth

**Date:** 2026-09-07
**Status:** Recorded
**Code:** `crates/tinyhivemind-hive/examples/bench` — arms `hive+mute`,
`hive+fact`, `hive+fact°`, `hive+pooled`
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
| `hive+fact°` | 80.9 | 81.5 | 71.2 |
| `hive+pooled` | **91.5** | **91.6** | **98.8** |

```text
hidden profile, budget 40
  hive+mute   − hive+:   -15.7 [-17.6, -13.8]     the turns alone
  hive+aside  − hive+:   -15.4 [-17.1, -13.4]
  hive+fact   − hive+:   -16.9 [-18.9, -15.1]
  hive+fact°  − hive+:    +3.2 [ +2.1,  +4.2]     the same exchange, off the floor
  hive+pooled − hive+:   +30.8 [+28.7, +32.9]     the ceiling

uniform, budget 15
  hive+mute   − hive+:    -2.5 [-3.0, -2.1]
  hive+aside  − hive+:    -2.5 [-3.0, -2.0]
  hive+fact°  − hive+:    -1.2 [-2.2, -0.1]
  hive+pooled − hive+:    +9.4 [+8.5, +10.4]
```

## What this settles

**The loss is the turns, entirely.** `hive+mute` transfers nothing and loses
*more* than the matched-turn arms that transfer something (−15.7 against −15.0
for `hive+fact`; `hive+aside` and `hive+ask` land within a point of it too).
`hive+aside!` is the one exception: aimed at the fact-holder, it loses
−17.1 — *more* than muting the answer entirely, for the reason the next
section gives. Whatever a matched-turn aside costs, it costs it before a
single word of content has changed hands.

`hive+mute` (`CheckStyle::MUTE`) targets the same way `hive+aside`/`hive+ask`
do — whoever spoke first, `informed: false` — so against those two it is a
clean matched-turn control: same turns, same words, no transfer. `hive+fact`
and `hive+aside!` are `informed: true`, aimed at the fact-holder, so the
`hive+mute` vs `hive+fact` gap (−15.7 vs −15.0) bundles the value of the fact
together with a possible difference in who spends the answering turn; it is
not a pure measurement of content value with targeting held fixed.

**So the pooling explanation is wrong.** The first note argued that averaging a
peer's reading imports the room's shared bias and moves a member toward the
decoy. If that were the mechanism, muting the answer would recover the loss. It
does not; it deepens it slightly. Taking the answer in is worth about +0.7
points against not taking it in — small, and in the *helpful* direction. That
paragraph of the earlier note is retracted below.

**The information is worth a great deal.** `hive+pooled` beats `hive+` by
**+30.8** on the hidden profile and by **+9.4 to +10.0** on uniform rooms, and
does it in *fewer* turns (10.15 against 10.98). This is the largest effect
anywhere in this benchmark. A room whose members can read each other's private
evaluations is a substantially better room. The intuition that peer-to-peer
exchange should make a hive mind smarter is correct, and this is the size of it.

**Most of the gap is scheduling.** `hive+fact°` runs the same bounded exchange,
with the same payload and the same number of contacts as `hive+fact` — off the
floor rather than on it. It moves from **−15.0 to +4.9**, an interval clear of
zero. Close to twenty points of the difference between the two is when the
exchange happens.

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
still counts nothing from an aside. It is worth +2.1 against the aimed
reading-only arm and does not come close to paying for the turn.

## What follows for the design

The mechanism is not what is limiting the room; the accounting is. An aside
resolves no trace, adds no supporter and moves no standing
([ADR 0010](../adr/0010-an-aside-carries-information-never-support.md)) — it is
by construction not a floor move, and charging it a floor turn is what makes it
lose. The measured shape of the fix is: **let a private exchange run concurrently
with the deliberation rather than in place of a turn of it.** That is a real
change to how a host schedules an aside, not to the aside algebra, and it is
outside this note; recorded here as what the numbers point at rather than as
something demonstrated in the library.

The second constraint is bandwidth. One contact captures 3 of the 21 available
points on the hidden profile and none of the 10 on a uniform room, and raising
`--aside-cap` off the floor does not help, because a member stops being
uncertain once its one contact has resolved the tie. The ceiling comes from
every member reading everything, continuously — which is again the colony, and
again not one question at a time.

## What this does not settle

Everything the first note listed, still: conformity (arithmetic participants
cannot be sycophantic), the settlement pointer (no simulated participant acts on
one), and rooms whose members are wrong in *different* directions. It also does
not demonstrate the concurrent-aside scheduling it points at — `hive+fact°`
bounds what that would be worth, at +4.9, and does not implement it. And, as
noted above, `hive+fact°`'s peer selection is an oracle rather than a
transcript-matched replica of `hive+fact`'s, so +4.9 is an upper bound on the
off-floor benefit rather than an isolation of scheduling from targeting.

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
```
