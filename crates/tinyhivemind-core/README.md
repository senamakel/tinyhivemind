# tinyhivemind-core

The pure algebra behind the room a hive of agents shares: desks and
membership, the roster, the mention grammar and its resolution, bounded
dispatch and referral, and the fold that projects a shared transcript into one
viewer's turn history.

See the [repository README](../../README.md) for what `tinyhivemind` is and
why it exists, and `CLAUDE.md`'s Charter section for the three-crate split.
This file covers only what is specific to this crate.

## What this crate deliberately does not hold

Everything here is a fold over data the caller already holds — no async, no
storage, no journal, no transport, and nothing that returns a `Result` for an
IO reason. The caller does the one roster read and the one transcript read and
hands the results in.

That is not style. This crate sits on the hot path of every agent turn, so it
has to compile in a host's default build with no feature flags behind it, and
it has to stay a leaf a host can call into without growing a path back out.
`.github/scripts/assert-pure.sh` enforces this at the workspace level: no
async runtime, transport, HTTP client, web framework, SQL client, git
implementation, or `anyhow` may enter this crate's dependency tree.

The waiting half — the paging walk over a session log, the optional model
selector call, and the mention-dispatch edge — lives in the sibling
`tinyhivemind` crate, which owns the ports a host implements. The responder
ladder's decisions remain pure here.

## Layout

Each feature area lives in its own module directory under `src/`, with a
`mod.rs` module root, an optional `types.rs`, and a `test.rs` or `test/`
holding its unit tests. See [`src/README.md`](src/README.md) for what
question each module answers.

- `src/` — the feature modules (see its own README for the index).
- `examples/` — runnable, CI-compiled usage of the public API. See
  [`examples/README.md`](examples/README.md).
- `tests/` — the public-contract regression suite, exercised only through
  what `src/lib.rs` re-exports. See [`tests/README.md`](tests/README.md).

## Where the crate-level docs live

`src/lib.rs` carries the crate overview: the four questions this crate
answers, the no-IO rationale, the module index, and a runnable doctest. Start
there for the authoritative description of the public API.
