# Roster

Agent and person identities visible in one shared conversation, as a borrowed
view over host-owned snapshots. The roster answers "who is here" — desk
membership, mention resolution, and turn selection all read this module
rather than each re-deriving availability from their own copy of the rules.

## Files

- `types.rs` — the wire records: `RosterMember` and `Person`.
- `mod.rs` — `Roster`, the borrowed view, its three-state agent model, and
  validation.
- `test.rs` — unit tests for structural validation and the active/retired/
  tombstoned projection.

## Public surface

- `RosterMember` — one agent: an opaque `id` and an optional `name` that can
  also be mentioned.
- `Person` — one human participant, deliberately not a host `User` type; hosts
  project their own identity into this neutral boundary record.
- `Roster::new` — borrow members, people, and retired agent ids.
- `Roster::with_tombstoned` — add the ids of agents removed for good.
- `Roster::validate` — check id uniqueness and non-emptiness.
- `Roster::active_members` — active agents in roster order; this is exactly
  what `@everyone` means.
- `Roster::active_member` — find an active agent by exact id (the
  **runnability** question).
- `Roster::registered_member` — find any registered agent, active or not (the
  **attribution** question).
- `Roster::person` / `Roster::people` — look up or iterate people.
- `Roster::is_retired` — whether an agent id is out of the active roster.

## The three-agent-state model

An agent's state is a fold over the borrowed id lists, not a flag on the
record:

- **Active** — in `members`, in neither id list. May run, is addressable, and
  is part of `@everyone`.
- **Retired** — in `retired_member_ids`. Out of rotation; reversible by the
  host dropping the id from that list.
- **Tombstoned** — in `tombstoned_member_ids`. Removed for good, refused
  exactly as retired is, but the `RosterMember` record stays registered so a
  message it already committed still renders with its author's name. Because
  the record stays, the id stays taken — re-registering it is
  `Error::DuplicateRosterMemberId`, not a silent re-attribution of someone
  else's history.

`active_member` and `registered_member` exist as a pair for exactly this
split: `active_member` returns one refusal (`None`) for "unknown", "retired",
and "tombstoned" alike, because a resolver whose refusals differ is a
directory an agent could read by probing names. `registered_member` is the one
lookup that sees past that refusal, and it is deliberately not a routing
input — nothing in this crate calls it to decide whether an agent may run.

## Operational constraints

- Agent and person ids occupy independent namespaces; a display alias may
  collide with another and mention resolution fails closed on that, not this
  module.
- No IO. `Roster` borrows its slices for the lifetime of one turn and copies
  nothing.
