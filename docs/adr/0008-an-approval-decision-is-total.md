# 8. An approval decision is total: it denies rather than fails

- **Status:** Accepted
- **Date:** 2026-09-06

## Context

[`docs/specs/approval.md`](../specs/approval.md) adds a pure approval gate to
`crates/tinyhivemind-core`. That approval *decides* and never enacts was never
in doubt — it is the charter's core/port line restated, and the Grok Bot
survey found five independent projects that had already drawn it. The genuinely
contested question is narrower and sits one level down.

This crate has a written convention: fallible public functions return
`Result<T>`, the crate alias, and a specific `Error` variant is added rather
than context stuffed into a string. Every fold here follows it. `DeskSet` fails
on an ambiguous desk, `Roster::validate` fails on a duplicate id, the directory
fold fails on a zero cap.

An approval gate has more ways to be malformed than any of them. The actor may
not be on the roster. The action may carry no classification. The verb may be
empty. The named approver may not exist, or may resolve through a desk identity
that names two desks. The grant slice may hold a record whose declared lifetime
is longer than the policy allows. Under the convention, each of those is an
`Error` and `approve` returns `Result<ApprovalDecision>`.

That is a hole, and it is the hole the survey watched three projects fall into
in a different form. gawkbot's gate is bypassed by `WUPHF_UNSAFE=1`, by a
dry-run flag, and by a verb-token heuristic that classifies an unrecognised
action as read-only; its own authors flag the standing-grant path as resting on
a single shared token. Each is a place where "could not decide" became "carry
on". A `Result` puts that same place at every call site, spelled `?`, and the
nearest handler is a host that has no idea whether the action it was about to
take was gated. The one convention this crate has that must not apply here is
the one that lets a gate disappear up the stack.

## Decision

`approve` is **total**. Its signature is

```rust
pub fn approve(..) -> ApprovalDecision
```

and it adds no variant to the crate-wide `Error`. Every condition that would
otherwise be an error is a `DenyReason`, and every one of them denies:
`Disabled`, `MalformedRequest`, `UnknownActor`, `UnclassifiedAction`,
`PolicyDenied`, `RememberedRefusal`, `UnresolvableApprover`, `NoApprover`,
`NoRule`. `PolicyDenied` and `RememberedRefusal` are ordinary policy
decisions rather than malformed input, and are listed here for the same
reason: none of the nine is a `Result::Err` anywhere in this gate.

The failure modes do not disappear; they change category. They stop being
something a caller may propagate and become something a caller must read out of
a decision it is already matching on. The repository's rule that every error
variant needs a test that produces it is discharged against `DenyReason`
instead, one test per variant, and the spec's testing section lists them.

Fail-closed is carried in the types wherever it can be. `DefaultVerdict` has no
`Allow` variant, so a policy cannot spell "allow by default" at all, and
`enabled: false` denies rather than bypasses.

## Alternatives rejected

**`Result<ApprovalDecision>`, following the crate convention.** Rejected. The
convention exists so a caller can handle a specific failure; here the only
handling a caller can safely do is deny, so the `Result` adds a second way to
write the answer and a `?` that skips writing it. A gate that can fail is a
gate that can be bypassed by failing.

**An `ApprovalDecision::Error { .. }` variant.** Rejected. It is `Deny` wearing
a hat. Hosts would match it separately from `Deny`, and the branch that forgets
to is the same bypass with a different shape. One denial variant means one
denial branch.

**Panicking on a malformed request.** Rejected outright. Library code here does
not panic, and a panicking gate inside a host that catches unwinding is an
allow with extra steps.

**Validating at construction — `ApprovalRequest::new() -> Result<_>` — and
keeping `approve` total.** Rejected as the mechanism, though the goal is
right. A host can still assemble the struct from unvalidated fields and never
call the constructor, and splitting validation across two places means the
question "was this checked" has two answers. `ApprovalRequest` stays a plain
struct of owned fields and `approve` is the only place the question is asked.

**Classifying the action's effect inside the library, so `Unclassified` cannot
arise.** Rejected on the survey's evidence. gawkbot infers mutation from about
forty-five verb tokens, and a vendor using an unanticipated verb is classified
read-only and skips the gate entirely. A heuristic that fails open is worse
than an input that denies. The host declares `Effect`, and `Unclassified` is a
legal value that denies.

**Clamping an over-long grant to the policy cap instead of ignoring it.**
Rejected. gawkbot clamps to a thirty-day `maxGrantTTL`. Clamping rewrites what
a person agreed to and then acts on the rewrite; a grant outside current policy
means policy moved or the record is wrong, and both answers are "ask again".

## Consequences

- Hosts cannot distinguish "your request was malformed" from "policy said no"
  without reading `DenyReason`, and they should not surface that distinction to
  the acting agent. OpenBot returns the same sentence for "does not exist" and
  "you may not see it" so a bot cannot enumerate the roster by reading which
  refusal came back; the same reasoning applies to a gate. `DenyReason` is for
  the operator's log.
- `DenyReason` becomes a growth surface. Every new failure mode is a new
  variant on a public enum, which is a breaking change for an exhaustive match
  in a host — the same cost `NoDispatchReason` already carries, accepted for
  the same reason: a non-exhaustive denial enum invites a catch-all arm, and a
  catch-all arm is where a new denial silently becomes an old one.
- Test coverage of failure paths moves from the error suite to the module
  suite, and the per-variant obligation moves with it.
- A host that genuinely wants the distinction between "cannot decide" and
  "decided no" can recover it from the reason and choose to escalate. What it
  cannot do is receive nothing and proceed.
