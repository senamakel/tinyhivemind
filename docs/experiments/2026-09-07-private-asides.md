# Private asides, live

**Date:** 2026-09-07
**Status:** Recorded
**Code:** `cargo run -p tinyhivemind --example crosstalk -- --api-base <url> --model flash --aside`
**Spec:** [`../specs/private-asides.md`](../specs/private-asides.md)
**Decision:** [ADR 0010](../adr/0010-an-aside-carries-information-never-support.md)

Three agents on one desk, `deepseek-v4-flash` behind a local ladder router, with
`AsidePolicy { enabled: true, max_members: 1, max_messages: 4, must_surface:
true, require_thread: false }`.

This records what happened, not that the mechanism is a good idea. Whether a
room *decides better* with asides is unmeasured, and the benchmark arm that
would settle it is not in this change.

## What the mechanism did

Every claim the harness makes passed:

```text
[2] hop 0 @planner (channel) → aside with @auditor
     said: !aside @auditor Should we cut over the payments table this week,
           and what risks or evidence should determine that call?
[3] hop 1 @auditor (channel) → aside with @planner
     said: !aside @planner What rollback plan, migration rehearsal, and
           reconciliation evidence do we have before deciding?
[4] hop 2 @planner (channel)
     said: Defer this week's cutover unless a rehearsed rollback, migration
           rehearsal, and clean reconciliation results are documented today.

  @archivist
      [1] Ada (human): ...
      [2-3] @planner → @auditor · aside, 2 message(s), settled at [4]
      [4] @planner: Defer this week's cutover unless a rehearsed rollback...
```

Three readings of the same four rows. `@planner` and `@auditor` read all of it.
`@archivist` is handed one stub for the pair — collapsed, attributed, and
pointing at `[4]`, a row it can read. `Ada`, a person, reads every row in full.

Note the two aside rows name *different* audiences — `planner → auditor` and
`auditor → planner` — and collapse into one stub anyway, because they name the
same participants. An exchange is one exchange from the outside.

## The finding that matters: it had to be asked for

**The first two runs produced no aside at all.**

Same policy, same grammar, same seats. Given the default instruction — *"work
out how, between you, and tell me the plan and its worst failure mode"* —
`flash` was told it could address one peer privately and did not. It used the
plain `@peer` hand-off both times and answered in the open.

The run above came from an instruction that invited it:

> We must decide today whether to cut over the payments table this week. Before
> anyone commits to a position in front of the desk, check your doubts privately
> with one peer first, then tell me what you concluded.

Two attempts at fixing this from the library side did not work. Teaching the
grammar was not enough. Adding a concrete trigger to the rules — *"use it when
you are not yet ready to say something in front of the whole desk"* — was not
enough either. What worked was the operator asking for it.

That is consistent with the published picture rather than surprising against it.
[HiddenBench](https://arxiv.org/abs/2505.11556) finds agents "cannot recognize
or act under latent information asymmetry", and reports that the failure
survives cooperative prompting, debate framing and explicit instructions about
asymmetry. [SOTOPIA-TOM](https://arxiv.org/abs/2605.02307) builds close to this
environment and finds models "struggle to strategically seek information, and
tend to exchange most knowledge in the four rounds or not at all", with the
strongest model reaching 62% on its information-management score.

The honest reading: **`tinyhivemind` supplies a mechanism and a grammar, and
neither is sufficient.** Whether a room reaches for an aside is a property of
the models and of the host's framing. A host that turns `AsidePolicy` on and
expects agents to discover the affordance will get the transcript it would have
had anyway. This belongs in the host-integration documentation, not in a tuning
pass on the prompt.

## What was not measured

- **Whether asides improve a decision.** Nothing here compares outcomes. The
  simulated benchmark has no hidden profile between *members of one desk*,
  which is the structure an aside would have to help with, and building that arm
  is separate work. The mechanism ships off by default and is allowed to lose,
  on the precedent of
  [`2026-09-01-refutation-and-grounds.md`](2026-09-01-refutation-and-grounds.md).
- **Whether the settlement pointer changes behaviour.** `@archivist` was handed
  `settled at [4]` and the episode ended before it took a turn, so nothing here
  shows an agent *acting* on a pointer. That is the mechanism's central claim to
  being more than a partition and it remains unevidenced.
- **Cost under a narrow audience.** Every scan bound counts raw rows, so a
  viewer admitted to little of a busy desk receives proportionally less for the
  same budget and re-seeds sooner. The briefing states the achieved window
  rather than the nominal one, which is honest but is not a fix.
