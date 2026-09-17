# Test support

Shared fixtures for the integration tests in the parent `tests/` directory.
Each file here compiles into whichever integration test declares it with
`#[path = "support/....rs"]`, since every file under `tests/` builds as its
own crate root — there is no shared `mod.rs`.

## Files

- `harness.rs` — `ChatHarness`, a small in-memory host that owns an
  append-only journal (`Message`) and dispatches one agent turn per call
  through the `AgentEngine` trait. Deliberately mirrors the same host/library
  boundary a production consumer has: the library under test contributes only
  conversation identity (`same_conversation`), and the harness owns
  everything else — the journal, the engines, and the dispatch loop.
- `scripted_agent.rs` — `ScriptedAgent`, a deterministic `AgentEngine` that
  plays back a fixed queue of responses and records every transcript it was
  handed, so a test can assert exactly what context each turn saw.

Used by `../coordination_harness.rs` and `../openrouter_live.rs`; see
[`../README.md`](../README.md) for how the integration suite as a whole is
organized.
