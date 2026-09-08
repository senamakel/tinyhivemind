# Referral tests

Unit tests for bounded cross-desk referral, split by behavior area.

## Files

- `mod.rs` — wires the submodules in; no tests of its own.
- `routing.rs` — how `referral` picks a forward candidate and where the child
  turn lands: staying local, crossing to a mentioned agent's home desk,
  resolving a desk mention, every way a candidate is refused, and the
  compatibility statement against `mention_dispatch` for a plain policy.
- `returns.rs` — carrying one answer back to the conversation that asked: the
  `returns` knob, its precedence against a fresh forward, and every way a
  return is refused.
- `wire.rs` — pins the serde wire form of a referral, its policy, and a
  refusal decision.
- `disclosure.rs` — holds every `NoReferralReason` to the ADR 0009 disclosure
  classification and pins its settled wording.
- `support.rs` — shared fixtures: a payments/platform roster and desk
  snapshot, the fully open policy the swarm harness runs at, and helpers to
  build inputs and unwrap a decision (`agent_mention`, `desk_mention`,
  `input`, `accepted`, `refused`, `conversation`).
