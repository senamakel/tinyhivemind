# The desk crosstalk harness

Three agents on one desk, a human instruction addressed to nobody, and a
question that gets handed between them — live, against real models.

```sh
# against a local ladder router serving deepseek-v4-flash as `flash`
cargo run -p tinyhivemind --example crosstalk -- \
  --api-base http://127.0.0.1:6969 --api-key-env LADDER_API_KEY --model flash

# the same run, with the agents talking in a thread rooted at the instruction
cargo run -p tinyhivemind --example crosstalk -- \
  --api-base http://127.0.0.1:6969 --model flash --thread

# through an agent CLI instead of an endpoint
cargo run -p tinyhivemind --example crosstalk -- \
  --agent-cmd "opencode run --pure -m openrouter/~deepseek/deepseek-v4-flash-latest"
```

## What it is for

The library's two routing edges are covered by unit tests, and the deliberation
protocol is measured by [the benchmark](../../../tinyhivemind-hive/examples/bench/README.md).
Neither answers the question a host actually asks first: *if I seat real models
on a desk, does a message one of them writes actually reach the peer it names,
carrying the desk's context?*

This harness answers exactly that, and prints its evidence:

1. **An instruction reaches one agent.** The operator posts a message
   addressing nobody. The desk runs in `ResponderMode::Auto`, so
   `choose_responder` asks its selector — the same endpoint the seats run on —
   and the library validates the answer through `accept_selection`. The run
   reports which rung produced the responder and what the selector did.
2. **An agent addresses a peer, and that peer takes a turn.** The responder's
   committed reply goes through `resolve` with a `MentionAuthor::Agent`, then
   through `dispatch_mention`. Exactly one child turn can come out of that —
   `MentionDispatchDecision::One` carries one request and there is no variant
   that carries two — and it is bound to the same desk and thread.
3. **What crossed was the desk, not a private hand-off.** Each turn prints the
   projection it was given, by sequence and author. The addressed agent's line
   shows the addressing agent's message attributed to it, alongside the
   operator's original instruction.
4. **It stops, for a reason with a name.** The chain ends at
   `HopLimitReached`, `NoDirectAgentMention`, `SelfMention`, or a host refusal,
   and the run prints which.

## A run

```text
backend      http http://127.0.0.1:6969 model=flash thinking=Off
instruction  routed to @planner by AutoSelection (Selected)
floor        a thread rooted at the operator's message

[2] hop 0 @planner (thread@1)
     saw: 1:Ada (human)
     said: @auditor Please review the payments table migration plan for schema risks and identify the worst failure mode.
     then: one child turn enqueued
[3] hop 1 @auditor (thread@1)
     saw: 1:Ada (human), 2:@planner
     said: @planner The worst failure mode is a silent data-type mismatch on the `amount` column (DECIMAL vs FLOAT) causing rounding drift in financial records.
     then: one child turn enqueued
[4] hop 2 @planner (thread@1)
     saw: 1:Ada (human), 2:@planner, 3:@auditor
     said: @archivist Please archive the migration plan with the identified worst failure mode for the payments table.
     then: one child turn enqueued
[5] hop 3 @archivist (thread@1)
     saw: 1:Ada (human), 2:@planner, 3:@auditor, 4:@planner
     said: Archived: payments table migration plan with worst failure mode = silent DECIMAL/FLOAT mismatch on `amount` causing rounding drift.
     then: no dispatch: HopLimitReached

desk channel sees  1:Ada (human), 2:@planner
the thread sees    1:Ada (human), 2:@planner, 3:@auditor, 4:@planner, 5:@archivist
```

The last two lines are the point of `--thread`. A thread is a *narrower
conversation over the same desk*: the thread projection carries the whole
exchange, and the desk channel carries the root plus its first reply — which is
what `project_session` promises a channel viewer, so a desk reading it sees an
answered question rather than a run of unanswered ones.

## What is host-owned here, and why that matters

Nearly all of `host.rs` is a consumer's obligation rather than the library's.
`tinyhivemind` opens no database and no socket, so this harness supplies:

- a `SessionLog` over a `Vec` of rows, reading newest-first with an exclusive
  `before` bound;
- a `MentionTurnQueue` that runs the revalidation the port's documentation
  demands — re-read the trigger row, verify its author, content and desk,
  re-check the live policy and the target's availability, refuse a duplicate by
  `(desk, thread, trigger_sequence)` — all under one lock standing in for a
  transaction.

Those checks are written out rather than stubbed on purpose. A harness that
skipped them would show that the library dispatches; it would not show that a
host can hold the contract while it does.

`RULES` in `agent.rs` is the other host obligation: the desk routes on the
*first* `@id` in a line and on nothing else, and `@everyone` starts no turn at
all. A host that does not tell its agents that ships agents that address the
room and wonder why nothing happens.

## What this does **not** show

**It is not evidence about answer quality.** Four turns of one model on one
synthetic question says nothing about whether agents deliberate well. That
question belongs to the benchmark, against a matched-budget control, and the
honest answer there is narrower than the multi-agent literature suggests.

**There is no privacy here.** Every message this harness writes is readable by
every member of the desk, and that is a property of the library, not of the
harness. `SessionMessage` has an author and no audience; `SessionQuery` carries
a conversation and no viewer. Two agents in the same desk or thread receive
byte-identical projections.

So `@auditor, quietly — is this plan wrong?` is *addressing*, not a direct
message. It reaches one agent, and the whole desk reads it.

### What a DM would need

A genuine agent-to-agent DM inside a desk is a wire-format change, not a host
convention, and it is not in this repository today. It would need at least:

- an audience on the row — the host's log and `LogMessage` gain a recipient
  set, with the serde-compatibility story written down first, since existing
  journals have no such field;
- a viewer on the query — `SessionQuery` gains an identity, because the
  projection currently cannot differ between two members of a desk;
- a decision about what the *rest* of the desk sees, which is the hard part. A
  desk whose members cannot see each other's exchanges loses the pooling that
  makes a desk worth having, and a private channel between two members is a
  correlation the room cannot audit.

Two mechanisms already in the library cover much of what a DM is usually
wanted for, without any of that. **Threads** (`--thread`) give a pair a
sub-conversation the desk can still read. **Referral**
(`crates/tinyhivemind-core/src/referral/`, off by default) lets one agent put a
question to an agent on *another* desk and carry one answer back — the routing
half of a DM, across a channel boundary, still written into both channels in
the open.

## Backends

| flag | what it drives |
| --- | --- |
| `--api-base URL` | an `OpenAI`-shaped `/v1/chat/completions` endpoint, for both the seats and the selector |
| `--agent-cmd "CMD"` | an agent CLI taking the prompt as its final argument — `opencode run`, `claude -p`, `codex exec` |

A CLI backend gets **no selector**: spending a whole agent process to answer
"which id?" is not what a host would do, so the run falls back deterministically
to the desk lead and reports the disposition as `Unavailable`. That is the
ladder behaving correctly with a rung it cannot reach, and it is worth seeing.

Every HTTP request goes through the `curl` binary rather than an HTTP crate,
because this workspace forbids a transport dependency in `tinyhivemind` and an
example is built alongside it. The whole request, headers included, is written
to `curl`'s stdin with `--config -`, so neither the key nor the body appears in
the process argument list.

`--thinking` defaults to `off`. A reasoning model asked for one line can spend
its entire completion budget on a scratchpad and return empty content, which
looks exactly like a broken endpoint; `flash` does this reliably at the default
budget. Turning it off is both cheaper and, for a one-line protocol, more
honest about what is being measured.

## Flags

| flag | meaning |
| --- | --- |
| `--api-base URL` | an `OpenAI`-shaped chat endpoint for every seat |
| `--api-key-env NAME` | environment variable holding its key (default `LADDER_API_KEY`) |
| `--model ID` | model id for the seats and the selector (default `flash`) |
| `--agent-cmd "CMD"` | run an agent CLI per turn instead of an endpoint |
| `--timeout N` | per-request deadline in seconds (default 120) |
| `--thinking on\|off` | let the endpoint reason first, or not (default `off`) |
| `--hops N` | host hop budget for agent-to-agent dispatch (default 3) |
| `--instruction TEXT` | what the operator posts to open the desk |
| `--thread` | run the agents' exchange in a thread rooted at the instruction |
| `--window N` | messages projected into one turn (default 30) |

The process exits non-zero if any claim fails, so a run is usable as a live
smoke test rather than only as something to read.
