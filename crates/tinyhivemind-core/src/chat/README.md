# Conversation identity

This module answers one question, everywhere it needs answering the same way:
does a stored chat id name the default desk, and do two stored chat ids name
the same conversation? A message is journaled under a chat id, and every
surface reading the journal back — history rendering, thread resumption, an
agent's context seed — has to agree on the answer, or a transcript splits
across the ids that happened to write it.

## Files

- `mod.rs` — the constants and the two pure predicates.
- `test.rs` — unit tests. The four-spelling fold is asserted spelling by
  spelling rather than through a loop, so a broken case fails on its own name
  rather than hiding inside a shared assertion.

## Public surface

- `MAIN_THREAD_ID` and `GENERAL_DESK` — the two literal spellings a host may
  see on the wire for the default desk.
- `is_general_chat` — is this stored chat id the default desk? Folds `None`,
  `""`, `"main"`, and `"General"` (case-insensitive) into one answer.
- `same_conversation` — do two stored chat ids name the same conversation?
  Every General spelling is equivalent to every other; everything else
  compares verbatim, case included.

## Operational constraints

- The General fold is deliberately narrow: it is a fact about one desk's
  history, not a general-purpose case-insensitive string compare. Two named
  desks differing only in case remain two desks.
- `same_conversation` is not reusable as `a.eq_ignore_ascii_case(b)` — it must
  route through `is_general_chat` first, or a thread root stored as `None`
  will fail to match the `"General"` it renders under.
- Pure: no IO, no host types, no allocation beyond what `&str` comparison
  needs.
