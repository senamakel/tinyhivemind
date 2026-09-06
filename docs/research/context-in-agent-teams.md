# Context in agent teams: what a divergent view costs, and who pays it

Companion to [`long-context.md`](long-context.md), which asks what one agent
should do about a window smaller than its transcript. This one asks the
question P16 answers: what happens when two agents on the same desk hold
*different* transcripts, and what does a system owe them so that the difference
does not quietly become an error?

It is the reading behind private asides. The decision itself is
[ADR 0008](../adr/0008-an-aside-carries-information-never-support.md); the
behavior is [`../specs/private-asides.md`](../specs/private-asides.md).

## The two camps, and why both are right

The strongest published disagreement about agent teams is not about
architecture. It is about context, and it has two well-argued sides.

**Cognition — share everything.** Walden Yan, ["Don't Build
Multi-Agents"](https://cognition.com/blog/dont-build-multi-agents) (Cognition,
2025), states two principles:

1. Share context, and share *full agent traces*, not just individual messages.
2. Actions carry implicit decisions, and conflicting decisions carry bad
   results.

The failure it describes is concrete: parallel subagents each make unstated
choices about style, edge cases and structure; those choices conflict; the
result is fragile. Handing each subagent the same initial task description does
not fix it, because what they lack is each other's *ongoing* decisions.
Cognition's later note, ["Multi-Agents: What's Actually
Working"](https://cognition.com/blog/multi-agents-working), narrows rather than
retracts: the patterns that work are ones where several agents contribute
intelligence **while writes stay single-threaded**.

**Anthropic — isolate, and pay for it in summaries.** ["Patterns and problems in
multiagent systems"](https://www.anthropic.com/research/multiagent-systems) and
["When to use multi-agent systems (and when not
to)"](https://claude.com/blog/building-multi-agent-systems-when-and-how-to-use-them)
report a research system in which an orchestrator runs subagents with isolated
context windows that return condensed summaries of roughly one to two thousand
tokens. It outperformed the single-agent arm substantially, and the write-up is
explicit that most of the gain tracks token spend across independent windows
rather than cleverness in the protocol. The same pages carry the honest half:
domains "requiring all agents to share the same context, or involving many
dependencies between agents, are not a good fit"; early versions spawned
subagents for trivial queries and searched redundantly; and studies of swarms of
Claude agents found coordination failures, collusion and sabotage.

They are not actually in conflict, and this workspace has already recorded why:
[`shared-context.md`](shared-context.md) notes that "the only shape that
satisfies both is one durable full-fidelity log plus cheap per-reader
projections". Cognition is right that a *summary* handed between agents loses
the reasoning. Anthropic is right that repeated condensation plays telephone
with citations and that one shared window does not scale. A single log that is
never condensed, read through per-reader projections that are never authoritative,
gives each of them the thing they were defending.

The consequence for P16 is a boundary rather than a licence. An aside may
withhold **deliberation**; it may not withhold **decision**. Cognition's failure
mode requires parallel writers, and there are none here — `HiveStep::Speak`
carries exactly one turn and there is one floor.

## Divergence is cheap to create and expensive to notice

The mechanism that makes a divergent view dangerous is not the divergence. It is
that neither side can see it.

**Agents do not spontaneously model what a peer knows.** "Systematic Failures in
Collective Reasoning under Distributed Information in Multi-Agent LLMs",
[arXiv:2505.11556](https://arxiv.org/abs/2505.11556), introduces **HiddenBench**, 65 tasks built on the hidden-profile paradigm
(Stasser & Titus, already covered in [`biology.md`](biology.md)). Multi-agent
LLM groups reach **30.1%** accuracy when the deciding facts are distributed,
against **80.7%** for a single agent handed all of them. The diagnosis is the
part that matters here: agents "cannot recognize or act under latent information
asymmetry — they fail to reason about what others might know but have not yet
expressed", and converge early on the evidence everyone already shares. The
failure survives cooperative prompting, debate framing, explicit instructions
about asymmetry, deeper communication rounds, and larger groups.

Read against P16 this cuts both ways, and the second way is the important one.
It is a warning: adding privacy to a system that already fails to pool
information can make the pooling worse. It is also a specification: what the
agents lacked was a *signal* that an asymmetry existed. A design that hides the
existence of a private exchange removes the only affordance that could have
prompted the question.

**Building the environment is not the same as succeeding in it.**
"SOTOPIA-TOM", [arXiv:2605.02307](https://arxiv.org/abs/2605.02307), constructs
close to what P16 describes — 160 scenarios, three to five agents, partitioned
private knowledge, both public broadcast and private direct messages, and
channel-dependent sharing policies — and scores information management along
four axes: sharing what is useful, seeking what is missing, coordinating, and
protecting what should not travel. The strongest reasoning model evaluated
reaches **62%**. Baseline privacy violations on GPT-4o run at **9.9%**, falling
to **2.2%** under a theory-of-mind intervention. Models "struggle to strategically
seek information, and tend to exchange most knowledge in the four rounds or not
at all".

The reading for this workspace: a private channel is a capability agents use
badly by default. That argues for bounding it, for making its existence legible,
and for requiring that something come back out of it — not for leaving it out.

## Full mutual visibility is not the safe default either

It would be convenient if the conservative choice were to show everyone
everything. It is not.

"Understanding the Information Propagation Effects of Communication Topologies
in LLM-based Multi-Agent Systems",
[arXiv:2505.23352](https://arxiv.org/abs/2505.23352), EMNLP 2025, compares
topologies systematically and finds sparse ones preserving or improving
reasoning at lower cost, with dense connectivity propagating errors as
efficiently as it propagates signal. Follow-on work on generated topologies
([arXiv:2510.07799](https://arxiv.org/abs/2510.07799)) reports the same shape.

"The Cost of Consensus: Isolated Self-Correction Prevails Over Unguided
Homogeneous Multi-Agent Debate",
[arXiv:2605.00914](https://arxiv.org/html/2605.00914), names three failure modes
that peer visibility *causes*: sycophantic conformity, in which models abandon
independent reasoning to adopt the modal peer answer, measured up to **85.5%**;
contextual fragility, in which a larger prompt destabilises an otherwise correct
trajectory, up to **70.0%**; and consensus collapse, in which correct answers are
generated and then discarded during aggregation, producing oracle gaps up to
**32.3 points**.

This workspace has its own measurement of the same effect and it is the largest
number in the harness. [ADR
0005](../adr/0005-a-blind-round-may-be-concurrent.md) records a blind opening at
99.4% decided and 82.1% correct against 100.0% and 58.0% at full visibility — a
**24-point gap** bought entirely by withholding peers' positions for one round.

`Visibility::Blind` is therefore not an exotic knob. It is this repository's
existing, measured admission that showing everyone everything is a choice with a
cost, and P16 is the same admission generalised from *per-turn and time-based* to
*per-message and addressee-based*.

## What a private channel costs on the other side

Two literatures say the bill, and both are about auditability rather than
performance.

"Secret Collusion among AI Agents: Multi-Agent Deception via Steganography",
[arXiv:2402.07510](https://arxiv.org/html/2402.07510v3), and "Colosseum:
Auditing Collusion in Cooperative Multi-Agent Systems",
[arXiv:2602.15198](https://arxiv.org/abs/2602.15198), describe agents
coordinating through channels that look innocuous, and the emergence of what one
of them calls "a de facto private channel that defeats external safety rules".
Related work on covert coordination through continuous hidden states notes the
shape of the risk precisely: the public trace contains ordinary messages while
the coordination happens somewhere nobody is reading.

The design consequence is structural rather than procedural. A mechanism that
lets two agents exchange something *and leaves no row* is the failure. A
mechanism that leaves an attributed, sequenced row whose content is withheld
from peers but readable by any person is not: the exchange is bounded, counted,
addressed, and auditable, and only its text is private.

## How teams actually handle context in practice

The engineering practice has converged on four moves. Each maps onto something
this workspace either already has or gains in P16.

**Isolation is explicit, and filtering is API surface.** The [OpenAI Agents SDK
handoff documentation](https://openai.github.io/openai-agents-python/handoffs/)
specifies that a receiving agent sees the full conversation history *unless* an
`input_filter` says otherwise, with `remove_all_tools` shipped as the common
case. LangGraph's `Command`, and the equivalent hooks in the Microsoft Agent
Framework, are the same idea. The lesson is the default, not the filter: sharing
is the default and narrowing is a deliberate, named, host-supplied act.

**Condensation is where fidelity dies, so let outputs travel direct.**
Anthropic's write-up reports routing certain subagent outputs past the
coordinator entirely, describing the alternative as a game of telephone. Applied
here, this settles what an aside owes the room: the settlement is written by an
aside's own participant, in its own words, straight into the desk. A generated
gist of a private exchange would be the telephone, and would additionally put
words in an agent's mouth in the shared record.

**Compaction preserves by reference id.** The pattern that recurs across
production context-management writing is to let the model mark which references
survive and drop everything not associated with a surviving id, rather than
summarising uniformly. This workspace already has it: `!pin ^N` marks a sequence
and `read_pinboard` folds the marks (see [`../specs/recall.md`](../specs/recall.md)).

**Shared mutable state is the road not taken.** [Letta's shared memory
blocks](https://docs.letta.com/guides/agents/multi-agent-shared-memory/) attach
one block to several agents so that a write by one is immediately in every
other's context, and the docs rank the mutation primitives by concurrency safety.
It is a good design for its setting and it is the thing the charter's first rule
forbids here: a second store that has to be invalidated. `directory/mod.rs`
refuses the same shape in miniature and says why.

## The failure this note exists to name

Everything above is about a system. The four failures below are about a *reader*
— a language model with a sliding window and no reliable memory — and they are
what actually drove the shape of P16.

1. **A non-member forgets there was a hole.** A marker for an unreadable exchange
   sits in the window; the window slides; the exchange has now never happened.
   The agent's picture of the room is not incomplete, it is confidently complete.
   This is the HiddenBench failure with a mechanical cause rather than a
   cognitive one.
2. **A member keeps the conclusion and loses the grounds.** Private content ages
   out while the belief formed from it persists, so the agent argues a position
   whose support it can no longer produce — and, under the trace grammar, may
   cite a sequence no longer in front of it.
3. **A re-seed silently downgrades.** Content delivered incrementally lives in an
   agent's conversation history, not in anything re-derivable. A watermark that
   advances past filtered rows cannot redeliver them, so a forced
   re-initialisation can hand back a strictly poorer view with no signal that
   anything was lost.
4. **Markers eat the budget.** A twelve-message private exchange that costs a
   non-member twelve rows of a thirty-row window has spent the window to say
   nothing — and spent it in the middle, which
   [`long-context.md`](long-context.md) records as the worst place to spend it.

> **What this workspace would have to represent.** Three things it does not hold
> today. A **viewer**: `SessionQuery` carries a conversation and no identity, so
> no read path can differ between two members of a desk. An **audience** on the
> row, so that narrowing is a property of the message rather than of the caller
> that happened to read it. And a **settlement**: somewhere for a bounded private
> exchange to deposit what it produced, in the open, where it is citable,
> pinnable and countable. The first two are addressing. The third is the one that
> makes the mechanism a hive-mind primitive rather than a partition, and it is
> the same rule [ADR 0006](../adr/0006-a-referral-crosses-one-channel-at-a-time.md)
> already accepted for a referral: what crosses carries information, never a vote.

## What is deliberately not borrowed

- **No generated summary of a private exchange.** The telephone finding is
  strong and the failure is silent. A participant writes its own settlement or
  there is none.
- **No shared mutable block.** See above; it is a second journal.
- **No per-viewer counting.** Filtering what a turn is *shown* is safe.
  Filtering what the room *counts* would make `quorum::standings`,
  `attention::bids` and `directory` return well-formed wrong answers with no
  error path, because their order-independence is idempotent under redelivery
  and reordering and never under omission.
- **No claim that privacy improves decisions.** Nothing above measures that. The
  topology and conformity results are about visibility in general and the
  hidden-profile results point the other way. P16 bounds a cost and makes an
  exchange auditable; whether a room decides better with asides than without is
  a benchmark question, and the arm that would answer it is allowed to lose.

| mechanism | state this workspace already holds | state it does not |
| --- | --- | --- |
| narrowing what a turn sees | `Visibility::Blind` and `project_for`, per turn | any narrowing addressed to a *reader* |
| viewer identity at a read | `TeamBriefing::viewer_id`, in a type the projection never sees | a viewer on `SessionQuery` |
| addressing | `Mention`, `MentionTarget`, `referral` | an audience on a stored row |
| information crossing a boundary without a vote | `referral`, off by default | the same rule inside one desk |
| a durable working set against a sliding window | the pinboard, `!pin ^N` | a pinboard sliced by audience |
| querying instead of holding | `search_messages`, `search_threads` | a search that cannot quote what the searcher may not read |
| an honest budget | `BrevityPolicy`, stated in the briefing | a budget that states the *achieved* window rather than the nominal one |
| auditability of a private exchange | — | an attributed row that proves the exchange happened |
