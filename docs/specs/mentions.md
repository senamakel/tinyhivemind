# Roster and mention resolution

**Status:** Implemented
**Owner:** tinyhivemind maintainers

## Problem

A shared agent conversation needs one deterministic interpretation of authored
mentions. Hosts currently have to improvise alias matching, code-span handling,
and routing, which makes the same message address different teammates in
different surfaces. The pure collaboration layer must answer who a mention
means without dispatching a turn or consulting host state.

## Goals

- Represent active agents and signed-in people without host-specific types.
- Keep a removed agent attributable without ever letting it run again.
- Refuse an unknown name and an unavailable one the same way.
- Extract `@` and desk-only `@#` mentions while preserving authored spans and
  UTF-8 byte offsets.
- Revalidate supplied mentions against the current roster and desks.
- Normalize mentions for safe one-turn routing and context assembly.
- Pin the records' serde wire representation.

## Non-goals

- Starting agent turns, broadcasting, persistence, model calls, or IO.
- Treating a person as a host `User`; `Person` is intentionally neutral at the
  crate boundary and can represent any human participant supplied by a host.
- Validating references in stored mentions as structural roster failures.

## Proposed behavior

`RosterMember { id, name }` and `Person { id, label }` form a borrowed `Roster`.
Construction borrows member, person, and retired-member snapshots, and
`with_tombstoned` adds a fourth snapshot of tombstoned member ids. Validation
rejects blank or duplicate ids within either namespace; aliases may collide,
because ambiguous aliases fail closed during resolution.

### The three agent states

An agent is in exactly one of three states. The state is a fold over the
borrowed id lists, not a field on `RosterMember`, so no wire record changes.

| State | How the host says it | May run | Addressable | In `@everyone` | Attributable |
| --- | --- | --- | --- | --- | --- |
| Active | in `members` only | yes | yes | yes | yes |
| Retired | id in `retired_member_ids` | no | no | no | yes |
| Tombstoned | id in `tombstoned_member_ids` | no | no | no | yes |

A **tombstone** is a removal the host cannot take back. The host deletes the
participant but keeps its `RosterMember` in the `members` snapshot, so messages
it already committed still render with their author's name; every attempt to
run it, address it, or carry it into a turn is refused. Retirement is the
reversible case: the host takes an agent out of rotation and may put it back by
dropping the id from `retired_member_ids`.

Keeping the record is what protects history. Because the tombstoned member
stays in `members`, its id stays taken, and a host that re-registers it hits
`Error::DuplicateRosterMemberId` instead of silently re-attributing an old
conversation to a different agent.

`Roster::registered_member(id)` is the attribution lookup: it finds any agent in
the `members` snapshot whatever its state. It is not a routing input, and no
fold in this crate calls it. `Roster::active_member(id)` remains the only
runnability question, and `is_retired` answers `true` for a retired *or*
tombstoned id.

People have no equivalent state. A `Person` is never dispatched, so there is
nothing to refuse, and `Roster::person` already attributes any person the host
supplies.

### One refusal

An unknown name, a retired agent, a tombstoned agent, and an alias two
teammates share are refused **identically**, and no public outcome or error
tells them apart:

- extraction produces no mention at all for any of the four;
- a supplied mention naming any of them is retained as quiet context, with no
  reason attached;
- `direct_responder` returns `None`, and `mentioned_members` omits the id;
- `dispatch` and `referral` report their existing single `TargetInactive` /
  `SourceInactive` reason, and no variant is added for a tombstone;
- a reached responder fallback is `Error::NoActiveResponder`, whose
  documentation records that it covers unknown and unavailable alike.

The rule is CopilotKit/OpenBot's: "does not exist" and "you may not see it"
return the same sentence, so a bot cannot enumerate a roster by reading which
refusal came back
([`../research/grok-bots/copilotkit-openbot.md`](../research/grok-bots/copilotkit-openbot.md)).
[`../adr/0008-an-approval-decision-is-total.md`](../adr/0008-an-approval-decision-is-total.md)
draws the same line for the approval gate and settles where a reason may still
live: a structured reason is for the operator's log, and must not be rendered
to the acting agent.
[`../adr/0009-a-refusal-renders-what-the-caller-already-holds.md`](../adr/0009-a-refusal-renders-what-the-caller-already-holds.md)
then settles which reasons may nonetheless carry a sentence of their own. A
refusal renders distinctly only when what it discloses is something the caller
already holds — the policy or hop it supplied, its own identity, or a desk
snapshot identical for every viewer. Every reason turning on the existence,
activity, membership or reachability of a named other renders the one shared
sentence `NO_AVAILABLE_TARGET`, which is what makes the four refusals above
indistinguishable in words as well as in outcome.

`MentionTarget` is a tagged union of `Agent { id }`, `Person { id }`,
`Desk { id }`, and `Everyone`. A `Mention` stores the exact authored `text`, its
UTF-8 byte `offset`, its target, and a `quiet` flag omitted from JSON when false.
`MentionAuthor` identifies an agent, a person, or an unclassified author.

`resolve(body, supplied, author, roster, desks)` has two input modes:

- `None` extracts mentions from the body.
- `Some` treats the supplied list as authoritative. An empty list suppresses
  extraction. Malformed spans are dropped. Well-formed but stale targets are
  retained and made quiet.

An extracted mention opens at the start of the body or after ASCII whitespace
or one of `([{`. `@#` restricts lookup to desks. The alias begins with a Unicode
alphanumeric character or `_` and ends at the body end, whitespace, or one of
`,;.?!:)]}'\"`. The whole token is then compared for case-insensitive equality
against each current alias; the alias table is sorted longest-first, but the
token boundary is already fixed by the closer rule, so the sort changes no
outcome and there is no greedy or prefix matching.
Aliases include agent ids and nonempty names, person labels and stable ASCII
label slugs, desk ids and names, and `everyone`, `channel`, and `here`.
Ambiguity across distinct targets resolves nothing; `@#` deliberately bypasses
non-desk collisions. Candidate spans are tokenized on authored character
boundaries before case-insensitive comparison, so Unicode case mappings never
change or corrupt their byte offsets.

Alias lookup is the only case-folding step in the workspace. `DeskSet::resolve_id`
compares desk ids and names **case-sensitively**, so `@#engineering` resolves
through the alias table to the desk whose id is `Engineering`, while
`DeskSet::resolve_id("engineering")` returns `Error::UnknownDesk`. The asymmetry
is deliberate and safe: a resolved `MentionTarget::Desk` carries the canonical
id rather than the authored casing, and it is that canonical id every later
lookup — revalidation, member expansion, referral — is given. Folding case in
`resolve_id` would make two desks named `Ops` and `ops` collide, and refusing to
fold it in alias lookup would make an authored `@#Engineering` stop addressing a
desk it plainly names. Neither half should be "fixed" to match the other; see
[`grammar-mentions.md`](grammar-mentions.md) §1.3.

Closed inline-backtick spans and CommonMark-style fenced spans opened by three
or more backticks or tildes are ignored. An unclosed inline backtick masks
nothing. All offsets remain offsets in the original UTF-8 body.

Normalization sorts by reading order, keeps the first item at an offset, drops
self-mentions, makes repeated targets quiet, and allows at most
`MENTION_CAP` (50) nonquiet mentions. A supplied mention must have an in-bounds
character-boundary span outside code whose text is exactly one complete mention
token. A current alias yields its current target; an unknown, stale, retired,
tombstoned, or wrong-current-alias target is retained quiet.

`direct_responder` returns the first reading-order, nonquiet, active agent
target. A retired or tombstoned agent is never active, so neither can be
selected. Person, desk, and everyone targets never select a responder.
`mentioned_members` expands agent, desk, and everyone targets to active roster
members, deduplicates in reading order, and excludes the chosen responder.
Everyone means the addressed desk's members, or the full active roster for an
unaddressed/General conversation.

## Invariants and constraints

- Every operation is a pure fold over borrowed input.
- Agent and person id namespaces validate independently.
- Unknown or ambiguous mention input fails closed, never with a routing error.
- Unknown, retired and tombstoned targets are refused indistinguishably.
- A tombstoned agent is registered forever and runnable never.
- Resolution never dispatches and `Everyone` never fans out turns.
- One target can ping at most once per message; excess mentions remain context
  as quiet mentions.

## Acceptance criteria

- Exact serde shapes are pinned for every public payload.
- Tests cover boundaries, punctuation, Unicode offsets, aliases and ambiguity,
  desk-only syntax, inline/fenced code, supplied suppression and revalidation,
  self/repeat/cap normalization, retired agents, responder selection, and
  addressed-desk/everyone context expansion.
- A tombstoned agent's exclusion is tested in each context it could otherwise
  appear in — as an alias target, a responder candidate, an `@everyone` member
  and a desk member — and a test pins that unknown, retired and tombstoned
  references are refused identically in each.
- `tinyhivemind-core` remains accepted by the purity assertion.

## Open questions

None for P3. Dispatch policy is deliberately deferred to P7.
