# Mention dispatch

**Status:** Implemented
**Owner:** tinyhivemind maintainers

## Problem

An agent reply can explicitly address another agent, but resolving the mention
does not safely start the next turn. Hosts need a single, idempotent dispatch
edge that cannot fan out, loop without a bound, or create a second journal.

## Goals

- Decide from committed reply data whether one direct agent mention may start
  one child turn.
- Bound chains with a finite, host-supplied `max_hops` and no library hard cap.
- Put durable, authorization-aware, exactly-once enqueueing behind one host
  port.
- Keep the feature explicitly disabled until a host opts in.

## Non-goals

- Fan-out for person, desk, or `@everyone` mentions.
- Library-owned persistence, retries, environment configuration, or model
  selection.
- OpenCompany integration or a live provider test; those follow this library
  phase.

## Proposed behavior

`MentionDispatchPolicy { enabled, max_hops }` is supplied for every decision.
Zero hops disables dispatch. There is no compiled-in ceiling: values through
`u32::MAX` are valid, while child-hop arithmetic is checked.

`mention_dispatch` evaluates in this order: disabled policy, exhausted hop
budget, inactive source, then the first reading-order nonquiet `Agent` mention.
Quiet, person, desk, and everyone mentions are skipped. If that first direct
agent mention is the source itself or names an inactive target, the decision
fails closed and does not try a later mention. Otherwise the decision contains
one canonical `MentionTurnRequest` whose child hop is the parent hop plus one.

Dispatch keys bind the committed trigger sequence. The request also binds the
conversation (`desk_id` and optional thread root), source id, target id,
content, and child hop. The runtime `MentionTurnQueue::enqueue_once` port
receives that single owned request and returns `Enqueued`, `Already`, or
`Refused { reason }`. `EnqueueRefusal` contains only the expected
`Unauthorized`, `TargetUnavailable`, and `FeatureDisabled` reasons, so a
successful enqueue cannot be represented as a refusal.
`dispatch_mention` invokes it zero or one time, returns refusals as outcomes,
and returns unexpected host failure as a typed error with its source. It never
retries or considers another mention.

All public payload fields are required in JSON. Enum tags and variants use
snake case. A host maps its richer runtime conversation to the pure dispatch
conversation; this is a snapshot, not a host type or callback. The runtime
adapter canonicalizes every General alias to `GENERAL_DESK`, while named desk
ids remain exact and case-sensitive. Thread roots remain exact, including the
difference between a desk channel and each individual thread.

## Trust assumptions

The hop bound is only as good as the counter it reads, and the spec has never
said so. `input.hop` arrives inside the request a host assembles,
`mention_dispatch` compares it against `policy.max_hops`, and the `child_hop`
it hands back travels out through the host's storage and transport before
anything reads it again. The library never authenticates either number.

**What the library guarantees.** Given an honest `hop` and an honest roster:
a chain cannot exceed `max_hops`, because each decision admits at most one
child and increments by exactly one under checked arithmetic; the hop budget is
tested before any mention is read; and no self mention, inactive target, or
inactive source is ever dispatched to.

**What it does not.** That the `hop` it was handed is the one the parent
decision produced. A participant able to author an enqueue request can also
write `hop: 0` into it, and the loop the bound exists to stop then runs
unbounded with every individual decision comfortably inside its budget. The
same holds for `max_hops` and `enabled`, which are supplied per call and are
therefore only as trustworthy as whatever computed them.

**What a host must do.** Where every participant is trusted — one process
assembling requests from rows it wrote itself, which is the expected
deployment — nothing. Where they are not, the counter must be carried in
something a participant cannot edit: derived from the host's own stored trigger
row rather than from the message, or, across a process boundary, carried in an
assertion the receiving side verifies. OpenBot carries its depth cap in a
signed assertion for exactly this reason
([`../research/grok-bots/copilotkit-openbot.md`](../research/grok-bots/copilotkit-openbot.md)).

This library will not sign it. It has no transport to sign for and no key
material to sign with, and a signature it could neither issue nor verify at the
boundary that matters would be decoration. The assumption is stated here so a
host does not read the bound as stronger than it is.

## Refusals, and what is offered

Two rules the OpenBot survey produced, and where each stands here.

**A run at the cap is not offered the action.** Refusing at the edge teaches a
model that the action exists and then wastes a turn on it; withholding it is
cheaper and clearer. The decision fold already has this shape: `max_hops` is
tested before any mention is read, so a reply at the cap constructs no
`MentionTurnRequest` at all and there is nothing to refuse. What does not have
it is the text the model reads. `TeamBriefing::system_text` states
unconditionally that "a direct @agent mention may start at most one bounded
child turn when host policy enables mention dispatch", and the briefing carries
no hop and no policy, so an agent at the cap is told the capability exists and
then finds it inert. Narrowing that sentence by the remaining budget is a host
concern today, and stated here as a known gap rather than a solved one.

**A decline should be a sentence, not only a type.** Every reason the library
returns — the seven `NoDispatchReason` variants and the three `EnqueueRefusal`
variants — is a typed value with no rendering. There is no `Display`, and no
vocabulary of sentences an acting model could repeat to a person. A host that
wants one writes it.

That is deliberate rather than merely absent, and it is bounded by
[ADR 0008](../adr/0008-an-approval-decision-is-total.md): a refusal that is
distinguishable is a refusal an agent can probe, and `TargetInactive` versus
`NoDirectAgentMention` is exactly the pair that turns a failed dispatch into a
roster oracle. The two rules are compatible only if they are separated by
audience — the reason is for the operator's log, and any sentence rendered to
an acting model must be the *same* sentence across the reasons a caller must
not be able to tell apart. Which reasons those are is unsettled; see
[`responders.md`](responders.md) for the same tension on the selection path.

## Invariants and constraints

- One committed message creates at most one turn.
- The host owns storage, child-turn records, and the only idempotency record.
- `enqueue_once` must atomically re-read and validate the committed reply,
  current policy, authorization, target availability, and conversation binding,
  then durably enqueue at most once under the conversation-and-trigger key.
- **The idempotency check is decided in the same statement as the insert.**
  Reading the key and then writing if it was absent is a check that holds only
  while nothing else is enqueueing, and the case it has to hold in is precisely
  the opposite one. Decide it in the write — a conditional insert, an upsert, a
  unique index the write races against — and read `Already` off that write's
  own outcome. One message, one turn makes fan-out unrepresentable here, so the
  count OpenBot had to move inside its insert is a count this library never
  keeps; the race it lost to survives as the idempotency check, which has the
  same shape and the same failure.
- **The lease clock is the store's.** Whatever lease or visibility timeout the
  host puts on a claimed child turn must be computed *and* compared inside the
  store, never as a moment the calling process computes and the store then
  compares against its own. Two clocks pretending to be one is how a turn gets
  delivered twice, which spends a model call this contract has already promised
  is spent once.
- **The attempt count is visible.** A queue that folds attempts into a status
  hides the only distinction a retry decision can safely rest on: an attempt
  count of one means the turn has certainly not run, and anything higher means
  it may already have spent money and written a message. The port answers
  `Already` for a duplicate rather than a number, so the count lives on the
  host's own row and has to be readable there rather than inferred.
- The library never reads environment variables or enables the feature through
  a Cargo feature. OpenCompany's eventual adapter default is two hops, but its
  first integration remains disabled until deliberately enabled.
- Host refusal is final for the call; there is no library retry or fallback.

## Acceptance criteria

- Pure tests pin wire forms and cover disabled/zero, hop limits 1 and 2, a
  large limit, `u32::MAX`, inactive/self targets, reading order, quiet and
  non-agent mentions, and exactly-one decisions.
- Runtime tests prove zero-or-one queue calls, refusal mapping, source-preserved
  host failure, canonical bound-scope keys, and concurrent/retried duplicate
  enqueue producing exactly one durable key and one durable child turn.
- The queue contract documents the required atomic host transaction, the
  same-statement idempotency check, the store-owned lease clock, and the
  visible attempt count.
- Host integration tests revalidate stored rows and transaction rollback.
  A later live test proves two agents exchange at least one attributed turn
  through a real provider; neither is simulated in this crate.

## Open questions

- Whether any refusal reason may be rendered to the acting agent, and if so
  which reasons must share one sentence so the set cannot be used to enumerate
  a roster. ADR 0008 settles the direction for the approval gate; the dispatch
  reasons have not been read against it one by one.
- Host integration and live verification. Nothing else is open for the library
  phase.
