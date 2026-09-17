# Desks

A desk is a declared group conversation plus whatever a host has layered on
top of it since: members added after founding, and a stored final member
order. This module merges those borrowed snapshots into one deterministic view
without ever copying them into owned state.

## Files

- `types.rs` — the wire records a host stores: `Desk`, `DeskMember`,
  `DeskOrder`, and the `ResponderMode` enum that picks between lead and auto
  responder selection.
- `mod.rs` — `DeskSet`, the borrowed view that resolves identities, validates
  the whole overlay, and projects a desk's effective member list.
- `test.rs` — unit tests covering resolution, validation failures, member
  projection, and order handling.

## Public surface

- `Desk`, `DeskMember`, `DeskOrder`, `ResponderMode` — stable, serde-derived
  wire types (`snake_case` fields, `lowercase` for the mode enum).
- `DeskSet::new` — borrow declared desks, added desks, member additions,
  orders, and retired agent ids.
- `DeskSet::with_tombstoned` — add the ids of agents removed for good; see
  below.
- `DeskSet::resolve_id` — turn an exact id or exact name into the canonical id.
- `DeskSet::contains` — whether an identity resolves at all.
- `DeskSet::members` / `DeskSet::lead` — a desk's deduplicated, available
  members in effective order, and its first member.
- `DeskSet::iter` — declared then added desks, in that order.
- `DeskSet::validate` — check every desk, addition, and order for the first
  typed structural failure.

## Design notes

**Retired vs. tombstoned.** Both are excluded from `members`/`lead`, and
nothing in the result says which exclusion applied — that is deliberate, the
same non-disclosure the `roster` module practices for the same reason. A
tombstoned agent additionally cannot appear via `with_tombstoned` unless it was
already a raw member of the desk; the module tracks "was a member and became
unavailable" separately from "never a member" (`raw_members`) so a stale order
entry is *skipped*, not rejected, while a genuinely unknown order member is
still a validation error.

**Reserved identity.** Only the exact pair `id == name == "General"` is exempt
from the reserved-spelling check; every other desk is rejected if its id or
name case-insensitively collides with `"General"` or `"main"` (see
[`chat`](../chat/README.md)).

**Validation order.** `validate` walks desks, then member additions, then
orders, each in the caller's declared-then-added / input order, and returns
the first typed `Error` it finds — it does not collect every failure.

## Operational constraints

- No IO. Every method is a fold over the four or five borrowed slices given to
  `DeskSet::new`/`with_tombstoned`.
- `resolve_id` and everything built on it call `validate` first, so a
  malformed snapshot fails the same way from every entry point.
- A stored `DeskOrder` need not be rewritten the moment an agent retires or is
  tombstoned; validation tolerates a stale entry as long as that entry was a
  real member of the desk at some point.
