# hypergrok-trading-desk

- **URL:** https://github.com/galleonlabs/hypergrok-trading-desk
- **SHA:** `a80d82ccebb42876cf9f7e33484c5ff6b68b59dc` (shallow clone, HEAD)
- **Licence:** MIT (`LICENSE`)
- **Size:** ~1.6 MB, mostly Markdown; a handful of Python validators under `scripts/`
- **What it is:** not code that runs a desk — a *plugin* (Grok Bot / Grok Build / Cursor / Claude Code) shipping Markdown "Bot" prompts, "skills" (more Markdown, loaded on demand), and a JSON template, that a host chat product turns into six-to-seven persistent chat participants plus one off-floor reviewer. There is no orchestration engine in this repo at all — the "runtime" is Grok Bot's group-chat product and a human relaying `@mentions`.

## Architecture

Seven roles, six of them in one Grok Bot group chat ("Trading Floor"), one working by DM off the floor:

```text
you
 |
 v
Trading Floor (group chat, 6 Bots)                 DM
  Desk Lead ------------------------------------> Trade Reviewer
   |     |     |     |     |                       journal, reviews
   |     |     |     |     +-- Execution Trader ---> Hyperliquid /exchange   (the one writer)
   |     |     +-------------- Risk Manager -------> Hyperliquid /info       (your account, live)
   |     +-------------------- Strategist ---------> Hyperliquid /info       (history, backtests)
   +-------------------------- Research Analyst ---> browser, public data
                                Market Analyst -----> Hyperliquid /info, /ws
```
(`docs/ARCHITECTURE.md:7-22`)

The whole system is a fixed seven-stage pipeline for one artifact — a trade — persisted as a Markdown file per trade id (`proposals/HG-*.md`), never as anything the "agents" hold in memory: `idea -> evidence -> risk sign-off -> your approval by ticket id -> one send -> reconciliation -> review` (`docs/ARCHITECTURE.md:31`, restated verbatim in `skills/desk-trade-lifecycle/SKILL.md:14-16`).

## Roles: how declared, what fields

A role is a Markdown file with YAML frontmatter plus prose sections, one file per Bot under `agents/*.md`. Frontmatter fields, e.g. Risk Manager (`agents/risk-manager.md:1-14`):

```yaml
name: risk-manager
title: Risk Manager
description: ...
seat: floor            # or "off-floor" for trade-reviewer.md:5
skills:                 # list of Markdown skill-file names loaded into context
  - desk-risk-limits
  - desk-trade-lifecycle
  - desk-monitoring
  - hyperliquid-account
  - hyperliquid-market-data
  - hyperliquid-api-reference
writes_to_exchange: false   # exactly one role has `true` (execution-trader.md:16)
```

That's the entire schema: `name`, `title`, `description`, `seat` (`floor`/`off-floor`), `skills` (a list, not a capability grant enforced by any code — it is instructions telling the LLM which Markdown to also read), and `writes_to_exchange` (a boolean *documentation* flag, not an enforced permission — nothing in the repo checks it; the actual restriction is prose in the "Boundaries" section of each agent file, e.g. `agents/desk-lead.md:62`: "Never place, modify or cancel an order... you do not use the `hyperliquid-orders`... write paths").

There is no expertise weight, no threshold, no numeric budget field anywhere in a role record. "Expertise" is entirely the free-text job description and skill list; nothing computes or updates it.

## Routing

Fixed pipeline, not a market or ad-hoc mention resolution. The routing rule is written down twice, redundantly, as prose: the stage table in `docs/ARCHITECTURE.md:28-36` and the authoritative version in `skills/desk-trade-lifecycle/SKILL.md:11-16`, which names an explicit owner per stage:

```text
idea -> evidence -> risk sign-off -> user approval -> execution -> reconciliation -> review
 DL       MA/RA        RM              user            ET            ET               TR
```

Within that pipeline, the Desk Lead is the sole router — "Read each request, decide the smallest set of specialists it needs, and @mention them" (`agents/desk-lead.md:43`) — using literal `@mention` text in a Grok Bot group chat. This is exactly `tinyhivemind-core`'s mention grammar target (a bare `@name` resolved to a roster member) except: (a) there is no mention *grammar* spec — no `@desk`, `@everyone`, `@this` semantics, just `@Bot Name`; (b) resolution is done by an LLM reading a table in its own prompt, not a pure function over a roster; (c) there is no host port or hop bound — nothing stops the Desk Lead from mentioning three Bots in one message, though the prose asks it not to ("Do not do a specialist's job yourself when the specialist is available"). "Routing" here is a single LLM's judgement call, re-derived from its system prompt on every turn, not a computed decision a caller could audit or replay.

## Termination

Not an "episode" concept at all — each trade proposal terminates independently when its lifecycle file reaches a terminal `status`: `live` (stage 5 shows a resting/filled order), `closed` (flat + stage 6 written), or `void` (expired or rejected without retry) (`skills/desk-trade-lifecycle/SKILL.md:104-107`). There is no quorum, no vote, no deadlock detection, no budget on turns. The two real termination gates are:

1. **A hard human gate** at stage 3: the user must type the literal phrase `"approve HG-YYYYMMDD-NN"` after seeing the ticket in chat; "yes"/"go"/a thumbs-up is explicitly rejected and the Desk Lead must re-ask (`skills/desk-trade-lifecycle/SKILL.md:60-64`). A ticket that isn't approved before `expiresAfter` (default 30 minutes, `agents/execution-trader.md:41`) becomes void and must restart from risk sign-off.
2. **A hard machine gate** at stage 2: the Risk Manager can REJECT, which "ends the lifecycle for that proposal" unless the user supplies new evidence and it restarts at stage 1 (`skills/desk-trade-lifecycle/SKILL.md:59`).

Nothing here resembles `HiveStep::{Converged, Deadlocked, Exhausted, Idle}` — there is no multi-party consensus to converge or deadlock on. It's a single-threaded workflow with two veto points, not a deliberation.

## Adversarial / review roles

There is exactly one role with veto power, the Risk Manager, and it is a **unilateral, unappealable gate**, not a vote or a cross-inhibition mechanism: "You are the one Bot whose 'no' ends a conversation" (`agents/risk-manager.md:26`); "Never weaken a limit to fit a trade" (`agents/risk-manager.md:49`); a REJECT names the failed gate and the trade is dead until new evidence arrives (`skills/desk-trade-lifecycle/SKILL.md:59`). "When two Bots disagree, do not average them. Say what each claims... and what would settle it" (`agents/desk-lead.md:55`) — disagreement is surfaced to the human, never resolved algorithmically.

The Trade Reviewer is adversarial only in retrospect and only in prose: it grades "process" and "outcome" separately and will say a profitable trade was a process failure (`agents/trade-reviewer.md:39`), but this happens *after* the trade closes and changes nothing about that trade's outcome — it's a lessons-learned log, not a mechanism with any causal power over the current episode. There is no analogue of `Object`/cross-inhibition silencing an advocate, no analogue of `Dissent` outranking other bid reasons, no notion of "silenced" supporters. Disagreement is either a hard veto (Risk Manager) that ends the workflow, or a narrative disagreement (analysts) that the Desk Lead is told to relay rather than adjudicate, or a retrospective grade (Reviewer) with zero effect on the trade it grades.

## Approval gates and propose-vs-execute

This is the single most rigorously engineered part of the repo, entirely at the *system-integration* level rather than the prompt-engineering level:

- **Read/write plane separation is declared, and enforced outside the repo**: the schema names exactly one Bot `writes_to_exchange: true`; every other Bot is read-only against `/info` and public web (`docs/ARCHITECTURE.md:52-56`). The repository itself does not check this flag (see the schema note above); what actually stops another Bot from writing is the host product's `Require Approval` rule on the exchange write path, the same external gate described below — not the YAML field.
- **"One approval, one send"** is enforced by a 7-item pre-send checklist the Execution Trader must run before every send (`agents/execution-trader.md:39-49`): ticket has a Risk PASS with exact fields; user approval by id, in-session, not stale beyond a default 30-minute expiry; network in ticket matches configured network; account address matches; live price is within slippage tolerance; size/price pass exchange rounding and minimums; one grouped action only.
- **The approval phrase cannot come from a Bot**: "An approval a Bot can read is one a Bot could have written. Enforcement lives outside the conversation, in Grok Bot's Require Approval rule on the exchange write path. No Bot may type, quote forward, infer or simulate your approval" (`docs/ARCHITECTURE.md:58`). The document is explicit that the in-chat phrase is *not itself* the security boundary — the actual gate is the host product's approval feature, external to the LLM conversation entirely. This is a meaningful, non-obvious design point: they don't trust a shared, LLM-authorable transcript as a capability token.
- **Ambiguous outcomes are a first-class state, not a failure or success**: a timed-out send is "an unknown result, not a failure. Do not resend... wait until the send's `expiresAfter` deadline has passed" (`agents/execution-trader.md:55`); missing data is `unavailable`, "a verdict of its own... never collapses into 'the condition did not fire'" (`docs/ARCHITECTURE.md:60`).
- **Propose-vs-execute default**: strictly propose-then-execute-once-approved. Nothing executes without both a machine PASS and a human-typed approval phrase; there is no "auto" mode described anywhere in the repo.

## Cost / budget accounting

None, in the tinyhivemind sense (no per-role token/turn budget, no market pricing a bid). The only "budget" concept is financial, not computational: `risk-limits.md` caps *trading* risk — max risk per trade as % equity, max total open risk, max leverage, max position count, a daily loss stop (`agents/risk-manager.md:30`), floored by hard-coded desk ceilings the user's file may only tighten, never loosen (`skills/desk-risk-limits/SKILL.md:15-30`, e.g. `open_risk_after <= equity * 6%`, `risk_usd <= equity * 2%` at `SKILL.md:90-91`). There's no equivalent of "this LLM call costs X" or "this role gets N turns" anywhere.

## Pattern vs. prompt engineering — bluntly

Almost all of it is prompt engineering, and the repo doesn't pretend otherwise (`docs/GROK_BOT_TEMPLATE.md`, `docs/PROVENANCE.md` frame this as "desk instructions... does not start a trading desk"). Concretely:

- **Prompt engineering, not a pattern**: role declaration (Markdown + YAML frontmatter with free-text description), the whole `@mention`-based routing (an LLM re-reading a table each turn), the "how you work" bullet lists, the handoff/report string formats. None of this survives reduction to a pure fold — it's all instructions for an LLM to follow voluntarily, with no code checking it did.
- **A real pattern, worth stealing conceptually (not the mechanism, the shape)**: the *separation between an unenforceable in-band approval phrase and an out-of-band enforcement gate*. `tinyhivemind`'s dispatch/mention layer already treats the transcript as untrusted input the LLM can shape; this repo independently arrives at the same conclusion for a human-authorization signal ("an approval a Bot can read is one a Bot could have written") and solves it by pushing enforcement into a layer the transcript cannot touch. `tinyhivemind` doesn't currently model "this action requires host-side confirmation outside the transcript" as a concept at all — everything is a pure fold over the transcript. This is a genuine gap the hive crate's `Commit` phase doesn't cover (a `!commit` trace *is* transcript-authorable), and worth a note even though it's a port/host concern, not core-crate material.
- **A real pattern, but already present and more general in tinyhivemind**: the veto. Risk Manager's unilateral REJECT is a strict subset of cross-inhibition — it silences the whole episode rather than one advocate, has no notion of decaying salience or partial credit, and cannot be overridden except by supplying new evidence and restarting. `QuorumPolicy`'s `Object`-silences-advocate mechanism is strictly more expressive (breaks ties between competing options; here there's only ever one option, buy-or-don't) and already pure/fixed-point.
- **Not a pattern at all, just documentation of a fixed workflow**: the seven-stage lifecycle. It's a state machine with exactly one path through it (with two rejection loops back to stage 1/2), expressible as an enum with five states and two error transitions; it has none of the properties `hive-mind.md` cares about (no plurality of proposals competing for support, no salience decay, no quorum threshold, no bidding). It's closer to a saga/workflow pattern than to deliberation.
- **Uncited/unmodeled but real-world useful and orthogonal to tinyhivemind's scope**: `unavailable` as a first-class third truth value alongside pass/fail, and "unknown result, not failure" for a timed-out write. Both are host/domain concerns (how to interpret a stale or missing observation), not algebra this crate should absorb, but worth flagging in `refutation-and-grounds.md`-adjacent thinking if a future spec ever needs a "no data" grounds kind.

## Mechanism → tinyhivemind has → does not have

| Mechanism in hypergrok-trading-desk | tinyhivemind has | tinyhivemind does not have |
| --- | --- | --- |
| Role = Markdown file + YAML frontmatter (`name`, `seat`, `skills`, `writes_to_exchange`) | `RosterMember {id, name}`, `AgentThreshold {agent_id, threshold, affinity}` — a typed, minimal record | Any notion of declared "skills"/tool grants or a write-capability flag on a member; out of scope by charter (host owns capabilities) |
| Fixed 7-stage pipeline with named per-stage owner | `Phase::{Deliberate, Commit}`, a strictly one-way transition | A general N-stage workflow/saga primitive — deliberately not a goal (`hive-mind.md` non-goals: "fan-out", not sagas either) |
| `@mention`-based routing decided fresh by an LLM each turn | A pure mention grammar + resolution fold (`mentions.md`) that a caller can unit-test and replay | The LLM-side judgement of *which* specialists a request needs — inherently a model call, correctly left to the host |
| Unilateral Risk Manager veto ends the whole workflow | `Object` cross-inhibition that silences one advocate without ending the episode; `Dissent` outranks `Knows`/`Quiet`/`Salience` in bid precedence | A "hard stop, no restart without new input" veto class — the hive crate always has a path back to bidding, never a terminal reject |
| Approval phrase must come from the user and is enforced by a host-side gate the LLM conversation cannot author | Nothing — every `Trace`, including `Commit`, is authored by an in-transcript message the host resolves against active membership | A concept of "this trace requires evidence the transcript itself cannot provide" (out-of-band confirmation); a real gap worth a future spec note |
| Financial risk budget (% equity, leverage, position count) gating execution | `WEIGHT_CEILING`, `dominance_cap`, `repetition_cap` — computational/attention budgets, not domain budgets | Any domain-level (dollar, time, token) budget concept — correctly out of scope; that's host/application state |
| Retrospective process-vs-outcome grading (Trade Reviewer) with zero effect on the graded episode | Nothing; an episode's outcome is `HiveStep` returned once, not graded after the fact | Any notion of post-hoc episode review — plausibly a good future host-level tool built *on* `Directory::entries()` / trace history, not core-crate material |
