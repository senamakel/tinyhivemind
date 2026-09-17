# Error

One crate-wide error enum, as `CLAUDE.md` requires: every fallible public
function in `tinyhivemind-core` returns `Result<T, Error>`, and every variant
here corresponds to exactly one way a caller's borrowed snapshot failed
validation or a query found nothing.

## Files

- `mod.rs` — the `Error` enum (built with `thiserror`) and the crate-wide
  `Result<T>` alias.
- `test.rs` — one test asserting every variant renders as a standard
  `std::error::Error` with a lowercase, unpunctuated message, per the style
  rule in `CLAUDE.md`.

## Public surface

- `Error` — one variant per typed failure: empty or duplicate roster/person
  ids, empty/duplicate/reserved/ambiguous/unknown desk identities, malformed
  membership additions and orders (unknown desk, duplicate order, duplicate or
  unknown or missing order member), a duplicate selector candidate, and
  `NoActiveResponder` for a ladder fallback that names no active agent.
- `Result<T>` — `std::result::Result<T, Error>`, the alias every fallible
  public function in this crate returns.

## Design notes

Several variants are deliberately coarser than the failures that reach them.
`NoActiveResponder` covers an unknown id, a retired one, and a tombstoned one
with one message and one variant — splitting it would let a caller learn which
ids the roster holds by reading which refusal came back, and the ladder has
nothing to do differently in any of the three cases. This is the same
non-disclosure principle documented in [`roster`](../roster/README.md) and
[`mention`](../mention/README.md), applied to error variants rather than
lookup results.

## Operational constraints

- Every message is lowercase and carries no trailing punctuation; the unit
  test enforces this mechanically so it cannot regress one variant at a time.
- Add a specific variant instead of stuffing context into a string — this is
  the crate's stated convention in `CLAUDE.md` and every existing variant
  follows it.
- No IO, no panics: this module is pure data and `Display` formatting.
