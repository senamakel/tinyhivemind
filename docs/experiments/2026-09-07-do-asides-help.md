# Do private asides help a room decide?

**Date:** 2026-09-07
**Status:** Recorded
**Code:** `crates/tinyhivemind-hive/examples/bench` — `--aside-cap`, arms
`hive+aside`, `hive+ask`, `hive+aside!`
**Spec:** [`../specs/private-asides.md`](../specs/private-asides.md)
**Decision:** [ADR 0008](../adr/0008-an-aside-carries-information-never-support.md)

**No. A pairwise check is pure cost, and privacy is never the variable.**

The spec's acceptance criteria say the mechanism must be allowed to lose and
that the loss must be published. It lost.

## What was asked

P16 shipped a mechanism and the live harness showed it *works* — a private
exchange happens, a non-member reads a stub, a person reads everything. It did
not show the room was any better for it. Three questions, written before the
numbers existed:

**Q1 — does a pairwise check improve the decision?** A member that cannot
separate its two best options spends a turn asking one peer for its reading,
the peer spends a turn answering, and the asker averages the two. Against
`hive+`, the same room without the move.

**Q2 — is any of it about *privacy*?** `hive+ask` writes the identical words,
costs the identical turns, and differs in one thing: the row is desk-visible,
so every member reads the answer rather than one. If `hive+aside` and
`hive+ask` agree, privacy buys nothing and the whole question is about asking.

**Q3 — was the question aimed at the wrong peer?** `hive+aside!` sends the
check to whoever the transcript shows has grounded that option, rather than to
whoever spoke first. It exists to close the obvious objection to a negative
result.

## The arms

| arm | what it is |
| --- | --- |
| `hive+` | the tuned policy, unchanged. The control. |
| `hive+aside` | a member that cannot separate its two best options asks one peer, privately. The peer answers. The asker averages. |
| `hive+ask` | the identical exchange in the open. Same turns, same words, every member reads it. |
| `hive+aside!` | private again, aimed at whoever the room has heard ground the option. |

An `!aside` line parses to no trace whatever its audience, so in every arm the
exchange adds no supporter and moves no standing — which is the library's own
rule and not something the harness arranges. The arms therefore differ in who
reads the content and in nothing else.

`--aside-cap 0` makes all three bit-identical to `hive+`, and every published
number in the six-row table above them is unchanged to the decimal.

## What happened

Paired bootstrap over the same rooms, 2000 resamples.

| configuration | `hive+` | `hive+aside` | `hive+ask` | `hive+aside!` |
| --- | --- | --- | --- | --- |
| uniform, budget 15, 5000 rooms | **82.1** | 79.6 | 79.7 | 79.6 |
| uniform, budget 40, 2000 rooms | **81.7** | 81.6 | 81.8 | 81.6 |
| hidden profile, budget 40, 2000 rooms | **68.0** | 52.7 | 52.0 | 52.7 |

```text
uniform, budget 15
  hive+aside  − hive+:     -2.5 [-3.0, -2.0]
  hive+ask    − hive+:     -2.4 [-2.9, -1.9]
  hive+aside! − hive+:     -2.5 [-3.0, -2.0]
  hive+aside  − hive+ask:  -0.1 [-0.4, +0.2]

uniform, budget 40
  hive+aside  − hive+:     -0.1 [-0.6, +0.5]
  hive+aside  − hive+ask:  -0.2 [-0.5, +0.2]

hidden profile, budget 40
  hive+aside  − hive+:    -15.3 [-17.0, -13.4]
  hive+ask    − hive+:    -16.0 [-17.8, -14.0]
  hive+aside! − hive+:    -15.3 [-17.2, -13.5]
  hive+aside  − hive+ask:  +0.7 [-0.8, +2.1]
```

**A1 — no, and at a tight budget it costs.** At the tuned budget the check
spends 1.4 turns per episode (6.75 → 8.14) and loses 2.5 points, with the
interval clear of zero. Give the room a budget it cannot exhaust and the loss
goes to −0.1 with the interval spanning zero: the move is then free, and
worthless. The 2.5 points were the turns, not the mechanism.

**A2 — no. Privacy is never the variable.** `hive+aside − hive+ask` spans zero
in every configuration, and the interval is tight enough at the tuned budget
(±0.3) to call it a null rather than an absence of power. Whatever the check is
worth, it is worth the same whether one member reads the answer or five do.

**A3 — no, and aiming it changes nothing.** `hive+aside!` is identical to
`hive+aside` to the decimal in every configuration. The peer it reaches is not
what is wrong.

## The hidden-profile result, and why it is worse than a wash

Fifteen points, with the budget unconstrained and every arm deciding 100% of
its rooms, is not a cost-of-turns effect. It is the mechanism doing harm.

The reason is the one this workspace already writes down about desks, arriving
one level lower. Under `--hidden-profile` one decoy is planted `HIDDEN_LIFT`
above every member's reading except the one member holding the fact that rules
it out. Members are therefore **wrong in the same direction**. Averaging a
reading with a peer removes independent noise and imports shared bias, so a
member that checks with a peer usually moves *toward* the decoy — and it does so
before the grounded deposit has had a chance to discount it. `fact %` stays at
about 97% in all four arms, so the fact still reaches the floor; the room is
simply more committed against it by the time it arrives.

That is the same claim [ADR 0006](../adr/0006-a-referral-crosses-one-channel-at-a-time.md)
makes for channels — "averaging correlated error does not remove it" — and it
turns out to hold for a pair inside one desk as sharply as for a desk inside a
company. **Pooling helps across a correlation boundary and hurts within one.**
An aside is inside one by construction.

## What this does and does not settle

It settles the question P16 left open, in the direction that costs the
mechanism: a host that turns `AsidePolicy` on expecting better decisions will
get worse ones on the task shape where pooling matters most, and will get
nothing at all on the task shape where it does not. `AsidePolicy::DEFAULT`
being off is the right default, and this is the evidence for it rather than the
caution behind it.

It does **not** settle whether asides are worth having. Nothing measured here
touches the reasons the spec actually gives for them — auditability of a
private exchange, a bounded independence device, a place to form a view before
the room anchors. What it does establish is that *decision quality on this
benchmark* is not among those reasons, and the spec should not claim it.

Four things it does not measure:

- **A room whose members are not correlated in the way this one is.** The
  benchmark's members differ only by independent noise, or by a bias every
  member shares. A room where two members are wrong in *different* directions
  is the case an aside should suit, and this harness cannot make one.
- **Conformity.** The published argument for withholding peers' positions is
  that visibility induces it; the simulated participants here are arithmetic
  and cannot be sycophantic, so the harness is blind to the effect an aside is
  most plausibly for. The live rooms are where that would show, and the live
  run in [`2026-09-07-private-asides.md`](2026-09-07-private-asides.md) was too
  small to say anything.
- **The settlement pointer.** No simulated participant ever acts on one.
- **A free ceiling.** `swarm.rs` has `pooled`, which hands every desk every
  other desk's readings at zero turn cost and bounds what crossing could ever
  be worth. There is no analogue here, so the loss at budget 15 is attributed
  to turn cost by the budget sweep rather than by a ceiling.
