# Find

The two pickers that need no host read: searching agents, people, and desks by
name over the roster and desk snapshots the caller already holds for the turn.
Searching a *transcript* has to wait on a log, so that lookup lives behind the
session port in the `tinyhivemind` crate — this module only covers what is
already in memory.

Every function here builds `select::Candidate`s from a snapshot and calls
[`select::rank`](../select/README.md), so the ordering is the one ordering
described in `select`, not a second one invented here.

## Files

- `mod.rs` — `agents`, `people`, and `desks`, plus their `_matching` variants
  that accept any `select::Pattern`.
- `test.rs` — unit tests covering label/id matching, retired-agent exclusion,
  detail scoring, and deduplication of a desk declared twice.

## Public surface

- `agents` / `agents_matching` — active agents whose id or display name (or,
  for the `_matching` form, whose id or name against a `Pattern`) matches.
  Retired agents are never offered.
- `people` / `people_matching` — people whose id or label matches.
- `desks` / `desks_matching` — desks whose id, name, or description matches. A
  description is supporting text and is scored at half weight (see
  [`select::Candidate::with_detail`](../select/README.md)), so a desk named
  for the query always outranks one that merely mentions it. A desk declared
  twice — once in `declared`, once in `added` — is offered once, at its first
  declared position.

## Operational constraints

- Read-only: these functions never mutate or validate the roster or desk set
  they are given; call `Roster::validate`/`DeskSet::validate` first if that
  matters to the caller.
- No IO, no allocation beyond the candidate vectors built for one ranking
  call.
- The `regex` feature, when enabled, flows through unchanged: `_matching`
  functions accept `Pattern::Regex` and score it exactly as `select` does.
