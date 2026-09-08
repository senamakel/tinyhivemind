# What the PE 1006 desk taught us

Working notes from running `crates/tinyhivemind/examples/desk` against Project
Euler 1006 across runs 21–27, set against a Grok desk that solved the same
problem. Every claim here is backed by a measurement taken from the archived
transcripts, the router logs, or the raw event streams; where a number is an
estimate it says so. The companion record of what happened is
[`2026-09-07-pe1006-desk.md`](2026-09-07-pe1006-desk.md).

The purpose of this file is to say what to build next, and why.

## The finding that reframes everything else

The obvious hypothesis was that the Grok desk won because its agents talked to
each other and ours did not. Both halves of that are false.

**Our agents talked constantly.** In run 26, 28 of 31 agent-authored rows named
a peer, and each of those started that peer's turn — 90%.

**Grok's agents had no private channel.** All 88 of its messages are the same
kind, `send-message`, and scanning all ten recovered transcripts for any
routing field — `recipient`, `to`, `dm`, `private`, `audience`, `visibility`,
`channel`, `targetAgent` — returns none, in every file. It was one shared room
where seats addressed each other by name. That is our topology exactly.

So communication topology is not the variable. Two desks with the same
broadcast-with-mentions shape produced very different outcomes, which means the
difference is somewhere we had not been looking.

### Asides were available and nobody wanted them

Worth recording because it is a null result on a feature we built. Run 26 ran
with `AsidePolicy { enabled: true, max_members: 1, max_messages: 6 }`. All 55
rows came out `{"kind": "desk"}` and `!aside` appears zero times.

This is consistent with the `exchange` experiment
([`2026-09-07-why-asides-lose.md`](2026-09-07-why-asides-lose.md)): peer
information is worth +9..+31 points, and spending a floor turn to ask for it
costs more than that. The seats were not failing to use a feature. They were
correctly declining a bad trade. Any future aside design has to make asking
cheaper than a floor turn, or it will keep being right to skip it.

## What the variable actually is

| | Grok | ours |
|---|---|---|
| messages | 88 | 31 agent-authored |
| wall clock | 3h08m | ~10h |
| **one message per** | **2.1 min** | **19.4 min** |
| seats | 6 (incl. Lit, Lemma) | 4 |
| answer first stated by | Johnny, a continuously-contexted seat | — |

Two things fall out of that table.

**Cadence: 9× slower.** A room that turns over every two minutes can afford to
be wrong. One that turns over every twenty cannot — a wasted turn costs a third
of an hour, so every defect below is multiplied by twenty minutes.

**Continuity.** Grok's `Johnny` was declared up front as a seat who would "also
work the math in parallel", carried context across the whole three hours, and
was the one who stated `Ψ(10^18) ≡ 62418970 (mod 101001001)`, crediting Solver
and Theory for the `S_constrained` step. We have no equivalent. Every seat is a
fresh process per turn reading a ten-row window, which is why `NOTES.md` grew
to 37k characters: the seats were hand-rolling, in a file, the continuity
Johnny had for free.

## Host defects this surfaced

Ordered by what they cost.

1. **Both turn safeties failed to fire.** A child reached 44 minutes against a
   900s stall threshold and a 2400s turn deadline, with the host's main thread
   verifiably inside its own poll loop (`sample` put it at
   `AgentRunner::run → nanosleep`). Killing it externally printed
   `silent after 1157.29s` — a quiet timer that had already passed the
   threshold it was being checked against. **Cause not established.** This is
   the most important open item in the host.

2. **A post fence has two spellings.** A seat closed with
   `<<<POST>>> … <<<POST>>>`; `extract_post` read the last marker as an opener
   and delivered the three characters `>>>`. A verified result reached the room
   as silence and the chair nudged the seat that had just spoken. Fixed, with
   tests for both fences. The general lesson is that a model-authored delimiter
   is an *interface with a fallible producer*, and every such interface needs a
   test for the wrong-but-plausible form.

3. **The workspace silently rescued that failure.** The result survived only
   because the same turn wrote `psi_sublinear.py` and a `NOTES.md` section
   before posting, and the chair's nudge says to read `NOTES.md`. Stigmergy
   carried what messaging dropped. This was luck, not design, and it argues the
   shared workspace is load-bearing rather than a convenience.

4. **Restarts flood the projection window.** Each restart re-appends the task.
   By run 26, five identical copies occupied half of every seat's ten-row
   window and two thirds of a 13k-character prompt. A fresh transcript and an
   865-byte brief cut the prompt to 4.9k.

5. **Router timeout and stall threshold are one budget, and it was retuned
   three times in an hour — twice wrongly.** Details in the companion doc. The
   durable part: the config comment that recorded the earlier finding was
   correct, and was overridden without being read.

## Measurement discipline

These are process failures, not code failures, and they cost more than any
single bug. They are recorded because they will recur otherwise.

- **Do not measure a shared resource while using it.** Liveness was read off
  `docker logs ladder` twice while probe requests were being sent to the same
  ladder, so the count included the measurer's own traffic. Filter by the
  workload's own tag, and take probes to a separate ladder.
- **Do not infer a process's internal state from the outside.** Four diagnoses
  of the turn-timeout subsystem were made this way and three were wrong. The
  host prints nothing about its deadline, quiet timer, or buffer length, so
  every reading was a guess dressed as a finding.
- **Check the timestamps are real.** opencode stamps `step_start` and
  `step_finish` identically, so every per-step duration computed from the event
  stream is 0.0s. A whole latency argument was built on those before anyone
  noticed.
- **Read the comment before overriding the value.** The router's
  `request_timeout` carried a written record of why 600s had already been tried
  and rejected. It was raised to 600s anyway.
- **State the clock you actually read.** Several status reports carried
  inferred times that were hours out.

## What to build next

Ordered. Each item names the evidence above that motivates it.

1. **Instrument the turn loop before touching it again.** Print the deadline,
   the quiet timer, and the buffer length on every poll, behind a flag. Defect
   1 is unexplained and defects of that shape have already absorbed four wrong
   diagnoses; nothing else in the host should be tuned until its own state is
   observable. This is the prerequisite for everything below.

2. **Give the desk a continuously-contexted seat.** The single clearest
   structural difference from the Grok run. This does not require breaking
   one-message-one-turn: it requires one seat whose process, or whose
   conversation state, survives across turns rather than being rebuilt from a
   ten-row window. Design question to settle first: whether that is a resumed
   CLI session, a seat with a larger window, or a distinct role in the algebra.

3. **Attack cadence directly, and measure it as a first-class number.** 19.4
   minutes per message is the headline defect. The one-process-per-turn seat
   boundary is the known cost — a seat re-reads the room and the workspace
   every turn. Add turnover time to `desk-status.sh` so it is visible while a
   run is happening rather than reconstructed afterwards.

4. **Make the task a pinned row, not an appended message.** Fixes defect 4 at
   the root: a restart should update the brief in place rather than push five
   copies through the window.

5. **Treat every model-authored delimiter as a fallible interface.** Defect 2
   was one delimiter. Audit the others — `!pin`, `!aside`, `!surface`,
   `ANSWER:` — and give each a test for the wrong-but-plausible spelling.

6. **Decide what the workspace is.** Defect 3 says it is currently doing
   memory's job by accident. Either promote it deliberately — and then the
   projection window's size matters much less — or give the transcript the
   durability the seats are clearly reaching for when they write 37k characters
   of `NOTES.md`.

7. **Revisit asides only with a cheaper ask.** The null result above is not a
   reason to build more aside surface. It is a reason to change the price.

## Open questions

- Why did neither turn safety fire? (Defect 1.)
- What makes one large tool-loop request hang identically on two independent
  providers, when small probes on both answer in 3s? A hard probe returned
  nothing in 864s, and the step before the hang showed reasoning tokens jumping
  from ~40 to 29,234 — suggestive, not conclusive.
- Would a Lit-style seat have helped? Grok ran one and we did not; its four
  messages are in the recovered transcript and have not been read closely.
- Is the ten-row window the right size, or is it a number nobody has tested?

## Status of the mathematics

Not solved by our desk. `Ψ(10^18) mod 101001001` remains uncomputed here;
`PrefG`, `P1w`, `PrefF` and `Tail1` are still brute-forced and the assembly on
them is unwritten.

What the desk did establish, verified independently against brutes written
without reference to its code:

- `B(x,n)` matches at 160 `(x, n)` pairs to n≈6000, past the n≤610 it tested
- `C`, `T`, `R` match at 60 pairs to n=1200, past its n≤200
- `S(10^18)/10^36 = 0.3090169943749474` against β/2 = 0.30901699437494745
- `B(g,10^18) = 79414112` and `R(g,10^18) = 91481916`, both in milliseconds

The step that produced this came from `@checker` refusing to sign off on the
desk's own `O(log n)` claim, having measured the recursion at depth ~1.23n.
That refusal is the best thing the room did, and it is the behaviour worth
protecting in any redesign above.
