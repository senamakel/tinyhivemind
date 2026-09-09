# `pane` — the room, watched

The desk normally runs a seat as one child process and reads its stdout
([`../agent.rs`](../agent.rs)). That is the cheapest thing that works, and while
a turn is running it shows you nothing: the first trace it leaves is a line of
telemetry twenty minutes later, after the seat has already spoken. Run 28 spent
forty minutes in a turn whose only visible symptom was a cursor.

This module is the same turn made watchable. Each seat gets its own
`opencode serve` and its own real terminal attached to it in a tmux pane, and
the host takes the turn the way a person would — put the prompt in the box,
press enter — over that server's `/tui` endpoints.

```sh
cargo run --release -p tinyhivemind --example desk -- \
  --desk crates/tinyhivemind/examples/desk/desks/pe1006-watched.txt \
  --task ./TASK.md --workspace ./ws --tmux pe1006
# then, from anywhere:
tmux attach -t pe1006
```

## What makes it the session rather than a picture of one

- **There is no second channel.** The prompt the host submits is the prompt the
  terminal shows, because it is the same input box; the tool calls scrolling
  past are the ones the desk is about to be told happened.
- **The desk still owns the turn.** The server publishes, the host decides. A
  seat speaks by calling `desk_post`, the outbox is drained per turn, and
  `session.idle` is where a turn *ends* — not where a message becomes true.
- **One message, one turn is untouched.** Nothing here dispatches; `run.rs` is
  still the only thing that starts a turn, and it still starts one.

## Files

| file | holds |
| --- | --- |
| `../pane.rs` | `PaneDesk`: start a server per seat, drive one turn, tear down |
| `http.rs` | the three calls a pane needs, over `curl`, as [`../chat.rs`](../chat.rs) does |
| `events.rs` | folding a server's event stream into one `TurnOutput` |
| `tmux.rs` | the window: three seats become three stacked panes |
| `test.rs` | the layout it would build and the feeds it would fold |

## Two things that are easy to get wrong

**A part is republished, not emitted once.** `opencode run --format json` prints
each tool part once, finished. A server publishes the same part id again every
time it moves `pending → running → completed`. Pushing parts as they arrive
turns three tool calls into twenty; `events.rs` keeps the last state per id, in
first-seen order, which is what makes the two transports fold to the same thing.

**The prompt comes back on the same bus.** The text submitted to the terminal is
published as a `text` part exactly like the reply. Folding it in posts the
seat's own prompt to the room, so the user message's id is recorded and its
parts are skipped.

## The cost, stated plainly

A watched seat keeps its terminal across turns, so its conversation accumulates
in a way a fresh child process's never did — which is the failure the desk's
own [`../README.md`](../README.md) records as tried and turned off: the request
grows without bound until every call stalls. `--pane-compact-at` is the guard,
not a fix: when a session's reported tokens cross it the host asks the server to
summarize before the next turn goes in. Whether that holds over a long desk is
not yet established, and a run that stalls late is the thing to watch for.

The window is left standing when the desk closes, because the last thing each
seat did is the reason to have watched. `tmux kill-session -t <name>` reclaims
it.
