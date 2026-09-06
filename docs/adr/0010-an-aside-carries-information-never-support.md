# 10. An aside carries information rather than support, and a redaction is a row rather than an absence

- **Status:** Proposed
- **Date:** 2026-09-07

## Context

Nothing here can be said to fewer than everybody. `SessionMessage` has an author
and no audience, `SessionQuery` has a conversation and no viewer, and two agents
on one desk therefore receive byte-identical projections. Addressing a peer with
`@auditor` routes a turn; the message it rides in is read by the whole desk.

A room where everyone reads everything converges on what everyone already
shares, and this workspace has measured the cost of that inside its own harness.
[ADR 0005](0005-a-blind-round-may-be-concurrent.md): a blind opening decides
99.4% of rooms at 82.1% correct against 100.0% and 58.0% at full visibility, a
24-point gap bought entirely by withholding peers' positions for one round.
`Visibility` buys that once, for the whole room, at the start of an episode.
There is no way to buy it for two members, about one question, in the middle.

The published evidence points the same way and further. Sparse communication
topologies preserve or improve reasoning at lower cost than dense ones
([arXiv:2505.23352](https://arxiv.org/abs/2505.23352), EMNLP 2025). Peer
visibility induces sycophantic conformity to the modal answer measured up to
85.5%, and consensus collapse that generates correct answers and discards them,
with oracle gaps up to 32.3 points
([arXiv:2605.00914](https://arxiv.org/html/2605.00914)).

Three objections had to be answered, and each had a tempting wrong answer.

**Cognition says share everything.** ["Don't Build
Multi-Agents"](https://cognition.com/blog/dont-build-multi-agents) gives two
rules: share full agent traces rather than individual messages, and remember
that actions carry implicit decisions whose conflicts produce bad results. Taken
at face value this forbids an aside outright. It does not, and the reason is in
Cognition's own follow-up: the patterns that work are ones where several agents
contribute intelligence **while writes stay single-threaded**. That is already
the charter here — `HiveStep::Speak` carries exactly one turn, and there is one
floor. The failure mode Cognition describes needs parallel writers, and there are
none. So an aside may withhold deliberation; it may not withhold decision.

**A private channel is the named mechanism of covert coordination.** ["Secret
Collusion among AI Agents"](https://arxiv.org/html/2402.07510v3) and
["Colosseum"](https://arxiv.org/abs/2602.15198) describe agents coordinating
where nobody is reading, one of them naming "a de facto private channel that
defeats external safety rules". The tempting answer is a policy — log it, review
it later. The failure is structural and wants a structural answer.

**Privacy can make information pooling worse.** HiddenBench
([arXiv:2505.11556](https://arxiv.org/abs/2505.11556)) puts multi-agent LLMs at
30.1% under distributed information against 80.7% for a single agent holding all
of it, and names the cause: agents cannot reason about what peers know but have
not said. Partitioning knowledge further, with nothing forcing it back out, is
the obvious way to make that worse. The tempting answer is to hide an aside
completely, which removes the only signal that could have prompted the question.

The reading is
[`../research/context-in-agent-teams.md`](../research/context-in-agent-teams.md).

## Decision

`Audience` is a field on a stored row and `Viewer` is a field on a query, both in
`tinyhivemind-core` alongside the rest of the addressing algebra, gated by an
`AsidePolicy` that is off by default.

**A redaction is a row, not an absence.** A row a viewer is not admitted to keeps
its sequence, its author and its audience, and loses only its content. Two agents
cannot exchange anything without leaving an attributed, sequenced, counted
trace, which answers the covert-channel objection by construction rather than by
policy. It also preserves citations — the trace grammar addresses messages by
sequence, and removing rows would leave `^N` naming nothing — and it is the
signal HiddenBench found agents lacked: you cannot ask about an exchange you
cannot tell happened.

**Consecutive elided rows collapse into one, carrying a settlement pointer.** The
reader is a model with a sliding window, and a run of stubs spends that window to
say nothing, in the middle, where attention is worst. One row per aside, naming
its range, its members, its size and where it settled, turns a hole in a reader's
context into a resolvable pointer into the shared record.

**An aside carries information, never support.** A trace deposited in a row whose
audience is not `Desk` contributes nothing: no supporter, no silenced advocate,
no directory credit — identically for every reader. To make an aside count, a
member spends a desk-visible turn saying so in the open. This is the rule
[ADR 0006](0006-a-referral-crosses-one-channel-at-a-time.md) accepted at a
channel boundary, applied inside one desk.

**One transcript is counted, and only presentation narrows.** `step` folds the
whole transcript and `project_for` remains the only thing that filters — the
asymmetry `Visibility` established in
[ADR 0002](0002-hive-episodes-are-sequential.md). Folding per-viewer transcripts
would give `quorum::standings`, `attention::bids` and `directory` well-formed
wrong answers with no error path, because their order-independence is idempotent
under redelivery and reordering and never under omission.

**A person reads everything.** `Operator` and `Person` viewers are admitted to
every row in full. This is audit access and not membership: reading an aside does
not join it and does not change what its agents are told. The privacy here is
between agents. It is a deliberation device, not a security boundary, and the
spec says so where a host will read it.

**A participant writes its own settlement.** There is no generated summary of an
aside. Condensation between agents loses the reasoning Cognition is right to
defend, and a generated gist would additionally put words in an author's mouth in
the shared record.

## Consequences

- Hosts get a wire migration. `audience` is a required, non-null tagged object
  on every stored row, and `viewer` is a required, non-null tagged object on the
  query rather than on each row — following the convention `refutation_cap`
  established: a payload from before the field fails to decode rather than
  silently acquiring a permissive default. The cost is one field per row for
  `audience`, and it is the deliberate outcome for a policy-bearing field.
- An audience is immutable once appended. Widening one could never be redelivered
  incrementally, because a sharing watermark advances past filtered rows
  unconditionally, so a widened row would be below the watermark forever.
- Every read path that returns content now needs a viewer, and the two built to
  defeat the window bound — pins and search — defeat an audience bound for free
  unless explicitly stopped. A pin excerpt reaches the system prompt of every
  desk member, and an unscoped search reads every desk in the log. Those are
  tests, not review items.
- Recall inside an aside is a requirement rather than a nicety. A member that
  cannot search or pin its own aside loses it to the window and is worse off than
  if it had spoken in the open.
- Bound erosion is real and unfixed. Every scan budget counts raw rows, so a
  viewer admitted to little of a busy desk receives proportionally less for the
  same cost, and re-seeds more often. The briefing states the achieved window
  rather than the nominal one, which is honest rather than a fix.
- The room can close with an aside unsettled. `must_surface` is enforced when the
  next aside is requested, not at the last turn, and making convergence refuse an
  unsettled aside was rejected as too blunt for a first cut.
- Nothing here claims a room decides better with asides. The topology and
  conformity results are about visibility in general, and the hidden-profile
  results point the other way. The mechanism ships off by default, and the arm
  that would settle it is allowed to lose — the precedent is
  [`../experiments/2026-09-01-refutation-and-grounds.md`](../experiments/2026-09-01-refutation-and-grounds.md).

## Related

- [ADR 0002](0002-hive-episodes-are-sequential.md) — visibility is the fan-out
  knob; this generalises it from per-turn to per-message.
- [ADR 0005](0005-a-blind-round-may-be-concurrent.md) — the measured cost of full
  visibility, and the existing decision about divergent per-turn views.
- [ADR 0006](0006-a-referral-crosses-one-channel-at-a-time.md) — information
  crosses a boundary, a vote does not.
- [`../specs/private-asides.md`](../specs/private-asides.md) — the behavior.
- [`../research/context-in-agent-teams.md`](../research/context-in-agent-teams.md)
  — the reading.
