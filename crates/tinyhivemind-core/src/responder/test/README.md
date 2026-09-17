# Responder tests

Unit tests for the deterministic responder ladder, split by behavior area.

## Files

- `mod.rs` — wires the submodules in; no tests of its own.
- `ladder.rs` — `responder_plan`'s routing: explicit mentions, desk defaults,
  direct-agent chats, the orchestrator fallback, and how selector candidate
  detail is folded in or rejected along the way.
- `selection.rs` — `accept_selection`'s tolerant parsing of a model's raw
  selector output.
- `wire.rs` — pins the serde wire form of every responder payload type.
- `disclosure.rs` — holds every `ResponderRung` and `SelectionDisposition` to
  the ADR 0009 disclosure classification: which outcomes must share one
  sentence because they turn on a named other, and which render their own
  because they are about the caller's own request. Wildcard-free, so a rung
  or disposition added later fails to compile until it is classified here.
- `support.rs` — shared fixtures (`member`, `desk`, `mention`, `request`,
  `decision`) and wire-form assertion helpers used across the other files.
