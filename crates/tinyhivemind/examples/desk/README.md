# A live desk

A host, written out in full, for a room of real agents that share one
transcript and speak one at a time. The deliberation benchmark next door asks
whether a *protocol* beats a poll on a synthetic room; this asks something
smaller and more literal: can the ports in this crate carry a real problem that
takes hours and code, with real models in the seats.

```sh
cargo run --release -p tinyhivemind --example desk -- \
  --desk crates/tinyhivemind/examples/desk/desks/pe1006.txt \
  --task ./TASK.md --workspace ./ws --rounds 8
```

## What the library does and what this file does

Everything that waits on something is here, and none of it is in the library:

| the host (this example) | the library |
| --- | --- |
| `log.rs` — the transcript, as JSONL on disk | `SessionLog`, the paging port it satisfies |
| `DeskQueue` in `main.rs` — the turn queue and its idempotency | `MentionTurnQueue`, and `dispatch_mention` deciding *whether* to enqueue |
| `agent.rs` — one `opencode run` per turn | nothing: the library never starts a process |
| `memory.rs` — CortexDB recall and capture | nothing: the library holds no memory |
| the chair's nudge when the room falls quiet | `choose_responder`, deciding who answers the chair |
| `compose_prompt` | `TeamBriefing::system_text` and `project_session`, which say what a seat may see |

The rule the whole thing turns on is **one message, one turn**. A reply may
mention four teammates; only the first one runs. That is `mention_dispatch`
refusing to fan out, and it is why a desk cannot quietly start N processes.

## The desk file

Plain text. A header, then one block per seat:

```text
desk: pe1006
name: PE 1006 Desk
person: steven

[agent theory]
name: Theory
role: Fibonacci-word and Sturmian combinatorics specialist
brief: What this seat owns, in as many `brief:` lines as it takes.
```

`role:` is what the responder ladder's selector would see. `brief:` is private
standing context prepended to every turn that seat takes — it is not in the
shared transcript, because a fact every seat can read is not private
information.

## Flags

| flag | meaning |
| --- | --- |
| `--desk PATH` | the desk file (required) |
| `--task PATH` | the chair's opening message (required) |
| `--workspace DIR` | where seats read, write, and run code |
| `--transcript PATH` | the JSONL log; an existing one is resumed |
| `--agent-cmd CMD` | the agent CLI; the prompt is appended as its last argument. Name the workspace here too (`--dir`), not only through the child's working directory — see obligation 4 |
| `--rounds N` | how many times the chair may nudge a room that went quiet |
| `--max-turns N` | hard ceiling on turns |
| `--max-hops N` | `MentionDispatchPolicy::max_hops`: how long one chain may run |
| `--window N` | messages of transcript a seat sees |
| `--timeout SECS` | per-turn wall clock before the process is killed |
| `--cortex-base URL` | CortexDB root; `CORTEX_API_KEY` supplies the key |
| `--library-scope` `--session-scope` | the durable and per-run memory scopes |
| `--no-memory` | run with no recall and no capture |

`OPENCODE_CONFIG_CONTENT` is passed through to the agent process, which is how
a run pins one model — for instance a ladder rung that only ever serves
`deepseek-v4-pro`.

## Private asides

A seat may address one peer alone by making `!aside @peer` the first line of
its post. The harness never acts on that marker: it hands the line to
`tinyhivemind_core::aside::aside`, which resolves who it may reach and refuses
with a named reason otherwise, and a refusal leaves the row desk-visible —
the safe direction to fail in. The policy here is a pair, six rows, and a
settlement owed before another may be opened, because a desk solving one
problem wants two seats to sort out a disagreement without spending the room's
attention on it, and wants that to end in something the room can read.

The budget is folded out of the journal rather than stored, so this host keeps
no state the transcript does not already carry.

## Five host obligations found by running it

Both are the same shape as the ones `../../../tinyhivemind-hive/examples/bench/LIVE.md`
records: things the library cannot impose and a host has to.

1. **A model narrates.** Its answer arrives wrapped in reasoning that every
   other seat would then have to read. The prompt asks for the room message
   inside `<<<POST … POST>>>` and `agent.rs` takes only that.
2. **A model mentions everybody.** Told that four seats exist, it addresses all
   four, and only the first one runs — so the chain continues to whoever
   happened to be named first rather than to whoever is needed. The prompt says
   so in as many words: put the seat you need first.
3. **A turn that is killed has said nothing.** A seat can spend a whole budget
   inside tool calls and never speak: one run cost 90k tokens across 27 tool
   calls and posted nothing at all. The library cannot know a process was
   killed, so the host owes the room a message — `chat.rs` asks for one
   through a plain completion where there is no tool to call. Asking the CLI
   nicely does not work: told in as many words not to run anything, it ran
   eight more commands and hit the deadline again.
4. **Setting the child's working directory is not enough.** The seats wrote
   their notes and their code into the *repository checkout* rather than the
   workspace: an agent CLI resolves a write against its own project root,
   which it finds by walking up for a `.git`, not against the working
   directory its parent handed it. This run's own commit history is the
   evidence — an auto-commit hook filed four of the seats' scratch files into
   the branch before anyone noticed. Pass the workspace on the command line —
   `opencode run --dir <workspace>` — and check where the first file actually
   lands before trusting a room to accumulate anything.
5. **`max_tokens` is spent on reasoning first.** That wrap-up came back empty
   with `finish_reason: length` at a 1200-token cap, and again at 8000 — the
   whole budget went to reasoning tokens and `content` was the empty string,
   on both DeepSeek tiers. Uncapped, the same call answers in 350 tokens. A
   silent seat looked like a model refusing to speak and was a cap set too
   low.
