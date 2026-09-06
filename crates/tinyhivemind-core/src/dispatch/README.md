# Pure mention dispatch

This module turns one committed, revalidated agent reply into either no action
or one canonical child-turn request. It performs no IO and retains no state.

The decision order is intentionally observable: disabled policy, exhausted
hop budget, active source, then the first reading-order nonquiet direct agent
mention. Once a direct agent mention is reached, a self or inactive target
stops selection; a later target cannot become a fallback. Person, desk, and
everyone mentions never dispatch, so one committed reply cannot fan out.

`DispatchConversation` is the minimal pure snapshot needed to bind an enqueue
scope. Runtime hosts map every General alias to `GENERAL_DESK`, preserve named
desk ids exactly, and preserve the optional raw thread-root sequence.
`DispatchKey` supplies the committed
trigger sequence. The runtime queue must use both values as its idempotency
scope.

`max_hops` is a host-owned finite `u32`. The core imposes no smaller limit and
uses checked child-hop arithmetic.

## The words a refusal comes back in

`NoDispatchReason` renders through `Display` as one lowercase sentence an
acting agent may repeat to a person, so a host does not have to invent one.
`Debug` still prints the variant, and that is the operator's record.

The sentences are coarser than the variants on purpose. A refusal an agent can
tell apart is a refusal an agent can probe, so `NoDirectAgentMention` and
`TargetInactive` come back in the same words — `NO_AVAILABLE_TARGET` — and a
caller cannot learn which ids the roster holds by mentioning names one at a
time. A reason about the caller's own request — the policy it supplied, the hop
it supplied, its own id — discloses nothing it did not already know and is
worded distinctly. `HopOverflow` is the exception that proves the rule: it is
only reachable once a target has resolved, so it borrows `HopLimitReached`'s
sentence verbatim rather than reporting, by its wording alone, that the
mentioned agent exists.

The classification is
[ADR 0009](../../../../docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md),
and every variant is held to it by a test.
