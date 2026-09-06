# grok-ship

**What it is.** A prompt/Markdown pack ("charters" + Claude-Skills-style
`SKILL.md` files) that turns a Grok Bot install into a small software factory:
one persistent orchestrator ("Firstmate") talks to the human ("captain"),
delegates project work to per-repo "crewmate" agents, which launch ephemeral
Cursor cloud agents to write code and a fresh adversarial-review subagent to
gate it before a PR is opened.

- URL: https://github.com/kunchenguid/grok-ship
- HEAD: `87825cca5c16c1c5da4f532676bccd5c3342918b`
- Licence: MIT (`LICENSE`)
- Size: 380K, 13 files, 842 lines of Markdown/Python total, no application code
  — it is entirely prompt text plus one 44-line Python fetch script
  (`skills/triage-eligible-fetch/fetch.py`).

## Architecture

Three tiers, all prose charters read by an LLM host (Grok Bot), not code:

1. **Firstmate** (`GROK_BOT_FIRSTMATE.md`) — the one agent the captain talks
   to. Classifies incoming work as `scout` (investigation, report-only) or
   `ship` (authorized change), writes a row to a local SQLite backlog, and
   hands the row to the mapped crewmate.
2. **Crewmate** (`GROK_BOT_CREWMATE.md`) — one per project/repo. Receives a
   task-id-tagged message from Firstmate, launches a Cursor cloud agent for
   the actual coding, then starts a review subagent before opening a PR.
3. **Triage crewmate** (`GROK_BOT_TRIAGE.md` + `TRIAGE.md`) — an optional,
   opt-in standing job for repo maintenance (stale-PR close, VISION.md
   alignment checks); explicitly walled off from the ship pipeline's merge
   authority (`GROK_BOT_TRIAGE.md:10`, `TRIAGE.md:30`).

State lives in one SQLite file (`skills/project-management/SKILL.md:13-45`):
`projects` (repo → crewmate mapping) and `tasks` (`kind`, `status`, `gate_kind`,
`gate_ref`, `result`). This is the host-owned "journal" tinyhivemind refuses to
duplicate — grok-ship is a real instance of exactly the pattern the charter's
"no second append-only journal" rule (tinyhivemind `CLAUDE.md`) warns against:
a second source of truth for task state, separate from whatever transcript the
chat/session already is.

## Roles

Roles are **prose charter files**, not config records or code objects. There is
no schema with typed fields for expertise, tools, or thresholds — everything is
natural-language instruction interpreted at agent-boot time:

- Firstmate: `GROK_BOT_FIRSTMATE.md:1-51` — identity, delegation rules,
  communication tone ("address the captain as 'captain'" — `:25`), no typed
  capability list.
- Crewmate: `GROK_BOT_CREWMATE.md:1-33` — a *template* filled in by Firstmate
  at signon time (`:27-29`: "`<project name, repo list, source control, agent
  id>`") plus a free-text "Learning notes" section (`:31-33`) that Firstmate
  edits later ("update learning notes in their charter description" —
  `GROK_BOT_FIRSTMATE.md:23`). This is the closest thing to a persisted,
  evolving "expertise" field, and it is unstructured prose, not data.
- Review subagent: `skills/adversarial-review/SKILL.md:1-90` — spun up fresh
  per review (`:12`, "do not resume an old review subagent"), given the whole
  skill file as its prompt (`:14`), and torn down after one JSON verdict. It
  carries no persistent identity or budget field at all — it is closer to a
  pure function call than a role.

There is no field anywhere for a numeric threshold, a budget, or a bid weight.
"Tools" are implicit in charter prose ("launch a Cursor cloud agent", "use the
source control CLI recorded for the project").

## Routing

**Fixed pipeline, not a market or ad-hoc mention grammar.** The routing rule is
written down once, in prose, in two places that must agree:
`GROK_BOT_FIRSTMATE.md:39-41` (scout vs. ship decision, and the ship stage
sequence) and `GROK_BOT_CREWMATE.md:6-17` (the crewmate's mirror of the same
sequence). The pipeline is linear and stage-gated:

```
intake → classify(scout|ship) → [scout: cloud agent → report, stop]
                               → [ship: cloud agent → branch
                                   → adversarial-review subagent
                                     → auto-fix: loop back to cloud agent
                                     → ask-user: card to captain, hold
                                     → error: do not raise, hold
                                     → clean/info-only: open PR]
                               → PR checks green → captain word → merge
```

There is no dynamic bidding, no salience decay, no addressing grammar between
agents beyond "Firstmate messages the mapped crewmate by task id"
(`GROK_BOT_FIRSTMATE.md:17`). Selection of *which* crewmate handles a task is a
static lookup keyed by repo (`projects.crewmate_id`,
`skills/project-management/SKILL.md:22-32`; reused, never re-derived —
"Do not overwrite crewmate_id" appears three times:
`GROK_BOT_FIRSTMATE.md:7,37,49`).

## Termination

Termination is **stage-based and human-gated**, never quorum-based:

- Scout episodes end when the cloud agent files a report (`GROK_BOT_CREWMATE.md:8`)
  — a fixed, single-stage pipeline with no possible disagreement to resolve.
- Ship episodes end only when: the review loop returns empty/info-only findings
  *and* CI is green *and* the captain gives explicit merge word
  (`GROK_BOT_CREWMATE.md:15,17`; `GROK_BOT_FIRSTMATE.md:41`, "Factory ships
  never merge without the captain's explicit word, and never while checks are
  red"). A human is the terminal condition, full stop — there is no
  auto-converge path in the default factory ship flow.
- The one auto-merge escape hatch is triage-only, and is bounded by an
  external document (`VISION.md`) rather than by any accumulated in-episode
  signal: "corrective or opt-in work only (green CI, VISION aligned with no
  cannot-tell, not default-behavior, not security)" (`GROK_SHIP.md:13`,
  `TRIAGE.md:34`).
- The adversarial-review sub-loop itself has **no explicit iteration cap** —
  "auto-fix: reply to the same cloud agent. Then a new fresh review subagent"
  (`skills/adversarial-review/SKILL.md:80`) can in principle loop forever;
  nothing here plays the role of `turn_budget` or `Exhausted`.

## Adversarial / review role

Disagreement is represented as a **flat JSON findings list with a severity and
an action tag** (`skills/adversarial-review/SKILL.md:60-76`): `severity:
error|warning|info`, `action: ask-user|auto-fix|no-op`. This is advisory data
about the *artifact* (the diff), not a position taken by a participant in a
deliberation among peers — there is no second voice that could out-argue the
review, no supporter count, no topic, no citation graph.

Whether dissent can change the outcome: **yes, but only in the narrow,
hardcoded sense that any `error`-severity finding is a hard veto** — "Severity
`error` must not merge" (`:44`) and "error: do not raise" (`GROK_BOT_CREWMATE.md:15`).
This is closer to a static assertion (a boolean gate) than to
cross-inhibition: there is no mechanism by which the *cloud agent* (the
"advocate" for the shipped code) can rebut the reviewer and win the floor back
without simply changing the code. The review subagent is also structurally
unable to be overruled by anyone except the human captain (`ask-user` path) —
there is no peer of the reviewer that could out-vote it, so "dissent" here
never contests a tie between two options; it only blocks or doesn't block one
option.

## Approval gates and propose-vs-execute

Once a ship task is authorized, implementation itself proceeds without a
human in the loop (`GROK_BOT_FIRSTMATE.md:41`, "Ship is the default once
implementation is authorized") — the default *within* an authorized task is
execute, not propose. Two explicit human gates bound that execution, both
hardcoded, not derived from any accumulated signal:

1. **Merge** is always propose-then-human: "Factory ships never merge
   without the captain's explicit word" appears twice verbatim
   (`GROK_SHIP.md:13`, `GROK_BOT_FIRSTMATE.md:41`), and once negatively scoped
   in the triage carve-out ("that is not a factory ship and does not weaken
   this bar" — `GROK_BOT_FIRSTMATE.md:41`).
2. `ask-user` findings from review are also a hard human gate before a PR can
   even open (`skills/adversarial-review/SKILL.md:81`, `GROK_BOT_CREWMATE.md:15`).

Scout tasks are propose-only by construction — "Never open a pull request"
(`GROK_BOT_CREWMATE.md:9`) — the deliverable is a report, and a human
(Firstmate/captain) must explicitly re-authorize before any code changes.

## Cost / budget accounting

**None, anywhere in the pack.** No token budget, no per-role cost field, no
turn cap, no dollar or compute accounting. The one adjacent idea is a *model
choice* instruction ("grok 4.6, high reasoning, not fast" —
`GROK_BOT_FIRSTMATE.md:41`, `GROK_BOT_CREWMATE.md:8,11`), which is a fixed
capability tier, not a budget that varies with anything. "Default to handing
work off" (`GROK_BOT_FIRSTMATE.md:9`) is a compute-placement policy (push
grind off the shared VM), not a cost model. There is no analogue anywhere to a
`turn_budget`, `dominance_cap`, or per-agent threshold that rises/falls with
participation.

## Pattern vs. prompt engineering — bluntly

Almost all of this is prompt engineering, and grok-ship's authors would
probably agree — it is a charter pack, explicitly not a program. The parts that
are genuinely *structural* (the two below) are decisions any pure algebra could
express as constants; nothing here is folded from a transcript or computed.

Worth abstracting as a pattern:

- **Stage-gated pipeline with a hard veto on one severity class.** The
  scout/ship split and the `error`-blocks-merge rule are a fixed two-state
  machine with one absorbing "hold" state per gate. This maps directly onto
  `HiveStep`'s `Speak`/terminal variants, but grok-ship's version has no
  salience, no decay, no accumulation — it is `if severity == error: block`
  applied once, not a standing that could be overturned by more evidence.
- **Task identity threading ("task id") for async handoff.** Firstmate
  tagging every delegation with an id and requiring a reply against that id
  even when "nothing happened" (`GROK_BOT_FIRSTMATE.md:17-18`) is a real,
  reusable correlation-id discipline, independent of any deliberation
  mechanics — it is closer to `mention-dispatch.md`'s idempotent one-target
  dispatch than to anything in the hive crate.

Mostly prompt engineering, not pattern:

- The captain-tone instructions, the "nautical seasoning," and the whole
  three-tier crew metaphor are pure prompt styling with zero algebraic content.
- The adversarial review's JSON schema is an API contract for one LLM call, not
  a deliberation primitive — there's no trace, no topic, no citation, no
  accumulation across multiple review rounds beyond "start fresh" (explicitly
  *discarding* history each iteration, the opposite of a folded transactive
  memory).
- SQLite-backed task rows duplicate exactly the "second journal" antipattern
  the tinyhivemind charter forbids; nothing to borrow, only a smell.
- There is no routing market and no expertise estimator: crewmate selection is
  a static hash lookup, and there is nothing resembling `BidReason::Knows` — no
  member's history of correct diagnoses ever changes who gets picked.

## Mechanism → tinyhivemind has → does not have

| Mechanism in grok-ship | tinyhivemind has | tinyhivemind does not have |
| --- | --- | --- |
| Prose "charter" as role definition | `RosterMember {id, name}` — deliberately thin | Any structured or free-text role/capability field; by design, out of scope for the pure core |
| Static repo→crewmate routing table | Roster + desk membership algebra (who's on a desk) | A persistent, host-declared static routing table — this is explicitly host-owned state |
| Fixed scout→ship→review→merge pipeline | `HiveStep` state machine (`Speak`/`Converged`/`Deadlocked`/`Exhausted`/`Idle`) with a `Deliberate`→`Commit` one-way flip | A fixed *n*-stage pipeline primitive; the hive crate's stages are earned by quorum, not hardcoded by position |
| `error` severity as a hard veto | `require_grounded` support, cross-inhibition via `Object` silencing an advocate | A single-vote absolute veto; tinyhivemind's mechanisms are all accumulative/decaying, never a one-shot boolean gate |
| Fresh, memory-less review subagent per round | `Directory`/credibility fold that *deliberately* remembers who was credible across the live window | Any notion of "discard history each round" — the opposite instinct |
| Human "captain word" as terminal gate | `HiveStep::Exhausted`/`Idle` as pure terminal states; commit requires a `!commit` trace recorded in-band | An out-of-band human approval gate as a first-class terminal condition — a host would have to model "captain says yes" as its own trace kind or hold the state itself |
| Task-id correlation for async handoff | `mention-dispatch.md`'s bounded, idempotent one-target dispatch | Nothing missing here — this is the closest actual overlap; grok-ship's task-id discipline is a weaker, hand-rolled version of the same idea |
| No cost/budget accounting | `EpisodePolicy.turn_budget`, `dominance_cap`, decaying thresholds | grok-ship has nothing to compare — it is simply absent there |
