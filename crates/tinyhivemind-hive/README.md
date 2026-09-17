# tinyhivemind-hive

Bounded group deliberation for agent group chats: traces, salience, quorum
with cross-inhibition, and the attention market — all pure.

`tinyhivemind-hive` answers a different question from the other two crates in
this workspace: not *who responds to this message* but *how does a room of
agents reach a decision*. See the [repository root README](../../README.md)
for the three-crate split and the charter that governs all of it.

## What it is

- **Pure, and defines no port.** An episode is
  `step(state, transcript, roster, desks, policy) -> HiveStep`, a fold over
  arguments the caller already holds. The host does its waiting through the
  `SessionLog`, `Selector` and `MentionTurnQueue` ports `tinyhivemind` already
  defines — nothing here is awaited, and nothing here is a trait a host
  implements.
- **One message, one turn, still.** `HiveStep::Speak` carries exactly one
  `HiveTurn`; there is no variant that carries two. Independence between
  participants is bought as a visibility filter on the projection, never as
  concurrency. See
  [ADR 0002](../../docs/adr/0002-hive-episodes-are-sequential.md).
- **Fixed-point throughout.** Every score is integer arithmetic, so every
  payload derives `Eq` and every fold is reproducible — the same transcript
  folds to the same step on any machine.
- **Opt-in.** A host that never calls into this crate gets exactly today's
  behaviour: one responder off the ladder, one turn, done.

The full crate-level overview — the module list, what the crate deliberately
does not hold, and a runnable example — is in
[`src/lib.rs`](src/lib.rs). Start there for the API; start here only for
orientation.

## Where things live

- [`src/README.md`](src/README.md) indexes the feature modules.
- [`examples/README.md`](examples/README.md) covers the runnable examples,
  including the benchmark harness.
- [`docs/specs/hive-mind.md`](../../docs/specs/hive-mind.md) and
  [`docs/adr/`](../../docs/adr/) hold the behavioral spec and the design
  decisions this crate implements.
