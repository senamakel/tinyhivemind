# Briefing for the Grok Bot survey (working file)

`tinyhivemind` is *hive mind mechanics for agents*, delivered as a pure Rust
algebra plus a thin port layer. Read `CLAUDE.md` at the repo root and
`docs/specs/README.md` before writing. The questions this workspace answers:

1. **Roster and desks** — who is present, what a desk is, who is on it.
2. **Mentions** — the grammar (`@agent`, `@person`, `@desk`, `@everyone`,
   `@this`), how one resolves, and the rule that **one message starts at most
   one turn**.
3. **Projection** — the fold that turns one shared transcript into what a single
   participant sees (`docs/specs/thread-scoped-conversations.md`,
   `docs/specs/recall.md`).
4. **Dispatch** — `docs/specs/mention-dispatch.md`: a hop-bounded, idempotent
   edge behind a host port.
5. **Deliberation** — `crates/tinyhivemind-hive`: stigmergic traces, decaying
   salience, quorum with cross-inhibition, response thresholds, an attention
   market, an episode state machine. All fixed-point integer, all pure.
6. **Delegation** — `docs/specs/expert-delegation.md`, `cross-desk-referral.md`,
   `refutation-and-grounds.md`.

Hard constraints that shape what we can borrow: the host owns all storage; no
host types may appear in these crates; no async runtime, transport, HTTP
client, web framework, SQL client, git implementation or `anyhow` in the pure
crates. Anything we take from another project has to survive being reduced to a
pure fold over arguments the caller already holds, or to a port trait.
