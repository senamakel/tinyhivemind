# Masking module

This module answers one question for every authored grammar in the workspace:
which byte ranges of a message body are code, and therefore inert. It holds no
state, allocates only the ranges it returns, and never looks at what a marker
means — it only says where quoting begins and ends.

It exists because three grammars once answered that question separately and
disagreed. Mentions, hive traces and pin directives each carried their own
fence scanner with different rules, so the same body could have a span that one
grammar read as quoted documentation and another executed as a live directive.
The scanners are now one.

## Public surface

- `fenced_ranges` — fenced and indented code blocks.
- `code_ranges` — the above plus closed inline code spans. **This is the one
  every consumer should call.**
- `is_masked` — whether an offset falls inside a returned range.

Ranges are half-open byte ranges over the original body, ordered and
non-overlapping, and every edge is a UTF-8 character boundary. Offsets are
never rewritten, so a caller can mask and still report the position the author
wrote at.

## Why every consumer reads the same level

`code_ranges` masks inline spans as well as blocks, and the line-leading
grammars need that even though their markers can never sit mid-line. An inline
span may open on one line and close on a later one:

```text
run `
!propose #hidden
` carefully
```

`!propose #hidden` is line-leading *and* inside a code span. A scanner that
masked only fenced blocks would let quoted text cast a real vote. Traces and
pins read `code_ranges` for that reason, and the earlier fenced-only split —
justified at the time by "a marker preceded by a backtick is not line-leading"
— was simply wrong.

That the two levels agree for a line-leading marker is not an accident. For a
marker at offset `o` on a line starting at `s`, the bytes `s..o` are spaces and
tabs only; a range from `code_ranges` starts at a line start or on a backtick
and ends at a line start or just past a backtick run, so no edge can fall
strictly inside that whitespace. `is_masked(s)` and `is_masked(o)` therefore
always agree, which is what pins and traces (testing `s`) and mentions
(testing `o`) each ask.

## What it implements, and what it does not

It follows CommonMark for fence openers and closers (character, run length,
the three-space indent limit, the backtick info-string rule), for indented
blocks (four columns with tabs expanded to the next stop, opened only where a
paragraph is not already running), for inline runs (backtick-run matching,
backslash escapes, no pairing across a paragraph break), and for CRLF.

It is deliberately **not** a block parser, and three divergences follow from
that. All are pinned by tests so they are choices rather than surprises:

- **Containers.** Indentation is measured from the left margin, so a list
  item's continuation paragraph reads as an indented block. This over-masks: a
  directive written there is lost, never invented.
- **Inline pairing across a container boundary.** A list item interrupting a
  paragraph ends it in CommonMark; here a span may still pair across it. Also
  over-masking.
- **HTML blocks.** An indented line beneath an HTML block is a code block in
  CommonMark and paragraph continuation here. This is the one divergence that
  **under-masks**, so a directive quoted that way stays live. Recognising it
  needs CommonMark's seven HTML block start and end conditions.

Over-masking loses an authored directive; under-masking executes a quoted one.
When a future change trades one for the other, prefer over-masking, and read
`test.rs` before assuming a rule here is arbitrary — most of them were written
after a real body parsed the wrong way.
