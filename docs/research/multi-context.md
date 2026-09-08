# Many inputs, many outputs, one window: how agent teams hold context

Companion to [`context-in-agent-teams.md`](context-in-agent-teams.md), which
asks what a *divergent* view costs, and [`long-context.md`](long-context.md),
which asks what one agent should do about a window smaller than its transcript.
This one asks the question the PE 1006 desk forced
([`../experiments/2026-09-09-desk-lessons.md`](../experiments/2026-09-09-desk-lessons.md)):
a hive mind has N seats each reading and writing one shared record, and every
seat is a model with one bounded window. **Where does the context live, and
who carries it between turns?**

Two answers were on the table when this was written. *Give the room an
orchestrator that holds everything.* And *make it one session, with every
input from every seat folded in.* The literature has an opinion on both, and
it is the same opinion.

## Nobody's orchestrator holds everything

Every production system that reports results has a coordinator, and none of
them gives it the full context. What it holds is small and specific.

**Anthropic's research system** ([Patterns and problems in multiagent
systems](https://www.anthropic.com/research/multiagent-systems)) runs a lead
agent over subagents with isolated windows. Each subagent may spend tens of
thousands of tokens and returns a **1–2k token summary**. The lead holds the
plan and the summaries — and the write-up's sharpest operational lesson is
that the lead **saves its plan to external memory before its own context
fills**, because past ~200k tokens it will be truncated and the plan has to
survive. The coordinator's context is the thing being defended, not the thing
doing the holding. Token spend across *independent* windows explains most of
the performance gain.

**Cognition** ([Don't Build Multi-Agents](https://cognition.com/blog/dont-build-multi-agents),
and the follow-up [Multi-Agents: What's Actually
Working](https://cognition.com/blog/multi-agents-working)) argues the
single-threaded case: one continuous thread of thought with context
compression for long tasks, and where several agents are used, **writes stay
single-threaded** while extra agents contribute reading and analysis. The
"orchestrator with full context" here is a single agent, and it survives a
long task by compressing its own history, not by holding it.

**Blackboard systems**, classic and current. HEARSAY-II's control component
was an opportunistic scheduler over a board of hypotheses, not a holder of
them. The 2026 revival — Salemi et al., [LLM-Based Multi-Agent Blackboard
System](https://arxiv.org/abs/2510.01285) — has a central agent that **posts
requests rather than assigning tasks**; helper agents watch the board and
volunteer. Responses go to a separate response board so a helper's output does
not become another helper's context by default. 13–57% relative improvement
in end-to-end task success over master-slave orchestration and over RAG. Han et
al., [Exploring Advanced LLM Multi-Agent Systems Based on Blackboard
Architecture](https://arxiv.org/abs/2507.01701), keep the strict form — all
interaction blackboard-mediated, no private histories — and it works because
the board, not any agent, is the global context.

**DeLM** — [Decentralized Multi-Agent Systems with Shared
Context](https://arxiv.org/abs/2606.10662) — removes the coordinator entirely.
Parallel agents claim subtasks from a queue, read a **shared verified
context**, reason locally, and write **compact verified updates** back. Up to
10.5 points over the strongest baseline on SWE-bench Verified at about half
the cost per task, and up to 5.7 points on LongBench-v2 multi-document QA. The
paper's framing of the problem is exactly the one raised here: a central
controller integrating results is the bottleneck, and the fix is a shared
substrate every agent reads and writes directly.

**Claude Code agent teams** ([docs](https://code.claude.com/docs/en/agent-teams))
give each teammate a fully isolated window, and coordinate through two small
shared objects: a **task list** with dependency tracking, and a **mailbox**.
The lead decomposes and synthesizes; it does not hold the teammates' contexts,
and the docs say the isolation is deliberate, so that one agent's partial or
wrong findings do not leak into another's work.

So: the orchestrator is thin in every design that reports numbers. What is
thick is a **medium** — a board, a verified context, a task list, a log — that
outlives any one window and that every seat reads through a projection sized
to its own budget. That medium *is* the "one session with all the inputs". It
is not a model's context; it is what a model's context is built from.

This workspace already has the medium. The `SessionLog` is total-ordered,
attributed, append-only and never condensed, and `project_session` builds a
per-reader window from it. [`shared-context.md`](shared-context.md) records
why that is the shape both Cognition and Anthropic are arguing for. The gap
the desk exposed is not in the medium. It is in what a *seat* carries.

## What a seat carries between turns

The desk seat is a fresh process per turn. It reads a ten-row window, the
pinned board, and whatever files it decides to open, and when it posts, its
process ends and everything it reasoned is gone. The measured cost: turn 1 of
`@solver` in run 27 spent 15 of its 19 tool calls re-reading 12 workspace
files before doing anything. Grok's `Johnny` did not pay that, because it was
one continuous session for three hours.

The obvious fix — resume the CLI session — was tried and is documented in
`main.rs` as the wrong answer: the session keeps every prior turn, the
request payload grows without bound, and the room slows turn by turn until
every call stalls. That is Anthropic's 200k cliff met from below.

The long-horizon literature converged in 2025–2026 on the same alternative,
under several names:

- **Context-Folding** — Sun et al., [Scaling Long-Horizon LLM Agent via
  Context-Folding](https://arxiv.org/abs/2510.11967). Two actions:
  `branch(description, prompt)` opens a clean sub-context for a
  token-intensive subtask; `return(message)` **collapses the whole
  sub-trajectory into one message** in the parent. The parent's context grows
  by one message per subtask, not by the subtask's transcript.
- **AgentFold** — [Long-Horizon Web Agents with Proactive Context
  Management](https://arxiv.org/abs/2510.24699) — the agent itself decides,
  each step, what to fold and at what granularity.
- **Memory as Action** — [Autonomous Context Curation for Long-Horizon Agentic
  Tasks](https://arxiv.org/abs/2510.12635) — context editing is an action the
  agent takes, trained for, not a harness heuristic.
- **Structured eviction over summarization** — [Beyond Compaction: Structured
  Context Eviction for Long-Horizon Agents](https://arxiv.org/abs/2606.11213);
  and the harness view in [Code as Agent
  Harness](https://arxiv.org/abs/2605.18747), which separates what stays in
  the active window, what is compacted, and what is **offloaded to durable
  storage with a retrievable handle**.
- **Anthropic's own rule**, from the research system: externalize the plan to
  memory *before* the window fills; do not chase a larger window.

Read together: a seat that lives across turns does not need its process to
live across turns. It needs a **fold** — a bounded, superseding record of what
it established, what it is mid-way through, and what it would tell itself next
— written by the seat at the end of each turn and read back at the start of
the next. That is `return(message)` with the desk's turn boundary as the
branch boundary. It is what `Johnny` had for free and what our seats were
hand-rolling in a 37k-character shared `NOTES.md` — the wrong place, because
that file is every seat's and so is nobody's.

The private half matters. A seat's fold is *its* working state, and Cognition's
argument for sharing full traces is about **decisions**, not about scratch: the
room needs to know what a seat concluded and what it is doing, and it learns
that from the seat's posts. The reasoning that produced the post is what the
fold carries, for the seat that will need it next turn, and for nobody else.

## Feedthrough: what the workspace already says, unheard

Gutwin & Greenberg's three sources of awareness are in
[`shared-context.md`](shared-context.md): consequential communication,
**feedthrough** — observing the effects of someone's actions on a shared
artifact — and intentional communication, the expensive fallback. The desk
runs on the third alone. A seat that writes `psi_sublinear.py` is invisible to
the room unless it says so in its post, and in run 26 it said so as three
characters of a broken fence. The result survived only because the chair's
nudge happens to say "read NOTES.md".

The event stream already carries the fact: every `write` and `edit` tool call
names its path. Turning that into one short system row per turn — *this seat
wrote these files* — is the cheapest awareness channel available, and it is the
A2A distinction between a Message and an Artifact made legible without a new
record kind: a `SessionAuthor::System { kind: "workspace" }` row is a shape
the log already admits.

## The task is a pinned row, not the newest message

Zulip's lesson, also in [`shared-context.md`](shared-context.md): conversation
scope is a mandatory field on every record. The desk's equivalent defect was
the brief re-appended on every restart, five copies deep in a ten-row window.
The brief belongs where the pins live — carried to every turn regardless of
what scrolls — and the standing brief in every seat's prompt already is that.
Appending it again was a second copy of a thing that was already unavoidable.

## What is deliberately not borrowed

- **A model in the coordinator.** Every summarizing orchestrator is the
  telephone the Anthropic write-up warns about, and it would put words in a
  seat's mouth in the shared record. The chair stays deterministic.
- **A shared mutable block.** Letta's design; refused in
  [`context-in-agent-teams.md`](context-in-agent-teams.md) for the same reason
  it is refused here: a second store that has to be invalidated.
- **The fold in the log.** A seat's fold is superseding state — the newest
  version replaces the last — and the log is append-only and never condensed.
  Putting a superseding record into an append-only log either grows it
  without bound or asks the log to forget, and the charter forbids the second.
  The fold is per-seat state the host persists, exactly as `SharingState`
  already is.
- **A bigger window.** Still the answer the literature is least kind to.

## What this workspace would have to represent

| mechanism | already held | not yet |
| --- | --- | --- |
| a medium every seat reads through its own bounded view | `SessionLog`, `project_session`, `Viewer` | — |
| a thin deterministic controller | the chair, `choose_responder`, `mention_dispatch` | — |
| a seat's own fold carried across turns | — | a bounded, superseding, private per-seat record, read at the top of every turn |
| incremental delivery to a seat that already holds context | `prepare_delta`, `SharingState` | a host that uses it, once a seat is continuous |
| feedthrough from the shared artifact | `SessionAuthor::System`, the write events in the stream | a host that emits one row per turn from them |
| a working set that arrives regardless of the window | pins, the standing brief | a restart that does not append the brief again |
| an agent deciding what to keep | `!pin` | the same for its own fold |
