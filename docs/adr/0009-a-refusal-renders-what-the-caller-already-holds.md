# 9. A refusal renders only what the caller already holds

- **Status:** Accepted
- **Date:** 2026-09-06

## Context

Two rules are written down in this repository and they pull against each other.

**A decline should be a sentence, not only a type.** Every refusal this library
returns is text an agent has to be able to repeat to a person.
[`docs/specs/mention-dispatch.md`](../specs/mention-dispatch.md) and
[`docs/specs/responders.md`](../specs/responders.md) both record that no refusal
type had a `Display`, and both call it a gap: seven `NoDispatchReason`
variants, eleven `NoReferralReason` variants, three `EnqueueRefusal` variants,
`ResponderRung` and `SelectionDisposition`, all typed-only. A host that wants
words writes them, so every host writes different ones and the safety property
below has to be rediscovered by each of them.

**Some distinctions must not reach the acting agent.**
[ADR 0008](0008-an-approval-decision-is-total.md) records the reason: OpenBot
returns an identical sentence for "does not exist" and "you may not see it",
because refusals a caller can tell apart are refusals a caller can probe, one
name at a time, until it has the roster
([`docs/research/grok-bots/copilotkit-openbot.md`](../research/grok-bots/copilotkit-openbot.md)).
[`docs/specs/mentions.md`](../specs/mentions.md) already applies it inside the
mention grammar: an unknown name, a retired agent, a tombstoned agent and an
ambiguous alias are refused identically, and `Error::NoActiveResponder` covers
all of them in one variant.

The specs reconciled the two only by **audience separation** — the structured
reason is the operator's log, the rendered sentence is the agent's — and left
open *which* reasons must collapse. `TargetInactive` versus
`NoDirectAgentMention` and `Unavailable` versus `InvalidOutput` were named as
the live pairs and settled nowhere.

Audience separation is necessary and it is not sufficient. It says who reads
which artifact; it says nothing about what the artifact for the agent may
contain. A host that obeys it exactly can still write one distinct sentence per
reason — in its own words rather than the enum's — and the oracle is intact.
The leak is in the information, not in the channel, so the rule has to constrain
information. And because the library shipped no sentences at all, the rule had
nowhere to live except in prose each host had to read and apply.

## Decision

The library owns the wording, as a `Display` impl on every refusal and outcome
type, and the wording is bound by one classification rule.

**The subject rule.** A refusal may render a sentence of its own only when what
it discloses is something the caller already holds: the policy it supplied, the
hop count it supplied, its own identity, its own conversation, or a desk
snapshot that is the same for every viewer. A refusal whose occurrence turns on
the **existence, activity, membership or reachability of a named other** renders
as one shared sentence, exported as
`tinyhivemind_core::dispatch::NO_AVAILABLE_TARGET`:

> there is no available agent to pass this to

**The reachability corollary.** Reachability is itself a channel. A reason that
can only be reached *after* the fold has resolved a named other reports, merely
by arriving, that the name resolved. Such a reason either renders the shared
sentence or renders **verbatim** the sentence of a reason reachable before that
point. This is what makes `NoDispatchReason::HopOverflow` share
`HopLimitReached`'s wording, and `EnqueueRefusal::FeatureDisabled` share
`NoDispatchReason::Disabled`'s: every queue refusal is reached only after a
target resolved, so any wording unique to that boundary would be an existence
oracle even though the fact it reports — the host turned the feature off — is
the caller's own.

**Classification is per variant, by its most disclosing path.**
`NoReferralReason::NoReferralTarget` is withheld although several of its paths
are purely local, because one of them is "nothing addressable was mentioned".

**A desk is not a named other.** `DeskSet` carries no per-viewer scoping, so
there is no "not yours to see" state a desk refusal could be confused with. This
keeps `Error::UnknownDesk` and `Error::AmbiguousDesk` separable, as they already
were, and keeps `SelfDesk` and `UnknownDesk` worded distinctly. Desk
*membership* is a roster fact about other agents, so `EmptyDesk` is withheld.

**Success is exempt, and can be.** The rule constrains refusals only. A
dispatch that succeeds proves the target exists — the answer arrives — so
concealing existence in the success path would be theatre.

**`Display` is the agent's, `Debug` is the operator's.** The safe rendering is
the one `{}` reaches for, because the wrong default is the one that leaks. A
host that wants the distinction matches the variant or prints `{:?}`.

| Type | Variant | Renders |
| --- | --- | --- |
| `NoDispatchReason` | `Disabled` | its own |
| | `HopLimitReached` | its own |
| | `HopOverflow` | `HopLimitReached`'s, verbatim |
| | `SourceInactive` | its own |
| | `SelfMention` | its own |
| | `NoDirectAgentMention` | `NO_AVAILABLE_TARGET` |
| | `TargetInactive` | `NO_AVAILABLE_TARGET` |
| `NoReferralReason` | `Disabled`, `HopLimitReached`, `SourceInactive`, `SelfMention`, `SelfDesk`, `UnknownDesk` | their own |
| | `HopOverflow` | `HopLimitReached`'s, verbatim |
| | `NoReferralTarget`, `TargetInactive`, `EmptyDesk`, `TargetDeskless` | `NO_AVAILABLE_TARGET` |
| `EnqueueRefusal` | `Unauthorized`, `TargetUnavailable` | `NO_AVAILABLE_TARGET` |
| | `FeatureDisabled` | `NoDispatchReason::Disabled`'s, verbatim |
| `ResponderRung` | every variant | its own |
| `SelectionDisposition` | every variant | its own |

The two pairs the specs left open are settled opposite ways, and the rule is
what separates them. `TargetInactive` versus `NoDirectAgentMention` **collapses**:
one says the name resolved and the other says it did not, which is the roster
oracle exactly. `Unavailable` versus `InvalidOutput` **stays separable**: the
ladder never declines, so both arrive beside the responder id they explain,
both describe the host's own selector rather than any participant, and neither
varies with a name a caller could probe.

Each classification is pinned by a module-local test whose `match` over the enum
is wildcard-free, so a variant added later does not compile until its author has
classified it, and the test then holds the rendering to that classification.

## Alternatives rejected

**Leave rendering to hosts, as audience separation implies.** Rejected. It is
the status quo, and it makes the safety property a thing every host reimplements
from a paragraph in a spec. Three hosts produce three vocabularies and at least
one roster oracle.

**Collapse every refusal to one sentence.** Rejected. It is safe and it is
useless: "passing this on is turned off" and "you have reached the end of the
chain" are both facts the caller supplied to the library in the same call, and
withholding them buys nothing while costing an agent the one thing that would
let it tell a person what to change.

**Render distinctly and let the host redact.** Rejected for the reason ADR 0008
rejected `Result<ApprovalDecision>`: the safe path must not be the one that
requires remembering. A default that leaks and a redaction step that can be
skipped is a leak with an owner.

**A `sentence()` method instead of `Display`.** Rejected. `{}` is what a host
reaches for in a log line, a template, or a tool result, and whichever rendering
`Display` carries is the one that ends up in front of a model. It has to be the
coarse one.

**Carry the collapsed sentence as a variant on the enums.** Rejected. It puts
the same value in two places, and the structured reason is the thing ADR 0008
preserved for the operator. Collapsing at render time keeps both.

**Split the shared sentence per feature** — one for dispatch, one for referral,
one for the queue. Rejected. The stages differ in how far a decision got, so a
sentence differing by stage tells the caller how far its name got.

## Consequences

- Hosts that want a distinct decline per reason can no longer get one from `{}`,
  and should not want one. The variant is still there, and `{:?}` still prints
  it into the operator's log.
- `NO_AVAILABLE_TARGET` is public API. Its text is pinned by test; changing it
  is a behavior change for hosts that compare or template against it.
- Nothing is wire-breaking. `Display` is additive, and no serde representation,
  variant name or field changed.
- The collapse is only as good as the *set*. A future variant that turns on a
  named other and is worded distinctly reopens the oracle, which is why the
  classification is a compile-forced match in each test rather than a comment.
- `SelectionDisposition::InvalidOutput` remains a feedback signal to an agent
  trying to steer selection by prompt injection. It is not a roster oracle, and
  the same signal is already available from which agent answered, so the
  separation costs nothing that is not already spent.
- `DenyReason` is specified but not implemented
  ([`docs/specs/approval.md`](../specs/approval.md)), and takes this rule when
  it lands rather than settling the question again. Reading the enumeration
  there against the rule: `Disabled`, `MalformedRequest`,
  `UnclassifiedAction`, `PolicyDenied`, `RememberedRefusal` and `NoRule` are
  about the caller's own request or the policy it was gated under, and
  `UnknownActor` is the caller's own identity — a prober cannot vary it, which
  is why `NoDispatchReason::SourceInactive` renders distinctly too.
  `UnresolvableApprover` and `NoApprover` turn on whether a named person or
  desk approver resolves, so they are withheld. That reading is recorded here
  as the expected application and is not binding on the implementation, which
  owes its own per-variant table and tests.
