# `tinyhivemind`

The session runtime: the ports a host implements, the paging walk over a
session log, the responder ladder, and the mention-dispatch edge.

See the [repository root README](../../README.md) for what `tinyhivemind` is
and why it exists. This crate is the runtime half of the two-crate split the
Charter describes: `tinyhivemind-core` answers what can be decided from
arguments alone (the pure algebra — desks, roster, mention grammar,
projection fold); this crate holds what must wait on something — the paging
walk over a live session log, ephemeral team briefings, the responder ladder's
one optional model call, and the mention/referral dispatch edges — expressed
against ports a host implements.

`tinyhivemind` depends on `tinyhivemind-core` and re-exports its entire public
surface (`pub use tinyhivemind_core::*;` in [`src/lib.rs`](src/lib.rs)), so a
host takes one dependency rather than two, and the types a host handles are
the same types, not structural twins.

Crate-level docs, the runnable example, and the full module list live in
[`src/lib.rs`](src/lib.rs). Feature-module documentation is indexed in
[`src/README.md`](src/README.md); the example harnesses are indexed in
[`examples/README.md`](examples/README.md).
