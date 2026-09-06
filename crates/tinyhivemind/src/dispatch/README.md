# Mention-turn queue boundary

The runtime dispatch module composes the pure core decision with exactly one
host call. A no-dispatch decision makes no call. A one-target decision passes
the canonical request unchanged to `MentionTurnQueue::enqueue_once`; expected
refusals are final outcomes and unexpected failures retain their source.
Person, desk, and `@everyone` mentions never reach the queue, and a direct
agent mention can select only the first eligible child target rather than fan
out. The host supplies the explicit enablement flag and finite configurable
`max_hops` on every decision; the library has no smaller hard ceiling.

The host implementation is the transaction boundary. Under a key composed of
the request conversation and committed trigger sequence, one transaction must:

1. re-read the committed reply and match sequence, source, content, and scope;
2. revalidate live feature policy, authorization, and target availability;
3. durably enqueue no more than one child turn; and
4. return `Already` for a duplicate without creating another turn.

A rollback must leave no idempotency marker and no child turn. This crate owns
no journal and never retries. Environment variables and Cargo features do not
enable dispatch; the host passes an explicit policy into every decision.

## The words a refusal comes back in

`EnqueueRefusal` renders through `Display` as a sentence an acting agent may
repeat to a person, and `Debug` still prints the variant for the operator's log.

Every refusal at this boundary is reached only after the pure decision resolved
a target, so a sentence unique to the queue would report that the mentioned
agent exists. `Unauthorized` and `TargetUnavailable` therefore come back in the
same words as a mention that resolved to nobody at all
(`NO_AVAILABLE_TARGET`), and `FeatureDisabled` borrows the wording of
`NoDispatchReason::Disabled`, which a caller could already have received before
any mention was read. See
[ADR 0009](../../../../docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md).
