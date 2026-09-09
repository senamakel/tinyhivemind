# Desk files

Plain-text room definitions passed to `--desk`. The format is a header and one
`[agent <id>]` block per seat; `deskfile.rs` parses it.

## Files

- `pe1006.txt` — the Project Euler 1006 desk: four seats (theory, solver,
  checker, lead) whose roster and briefs mirror, as closely as the two systems
  allow, a Grok Bot desk that answered the same problem on 2026-09-05, so a run
  here is compared against that one rather than against a room built to win.
  No brief carries a result from that run.
- `pe1006-watched.txt` — the same problem and the same three working seats,
  with `lead` dropped and `checker` told to close the desk itself. Three seats
  is the shape `--tmux` reads a room in, one pane each; the coordinator seat's
  only remaining job after run 28 was restating a signed-off answer, which
  `desk_close` does in one call.
- `pe1008.txt` — the Project Euler 1008 desk: four seats on a
  finite-difference and modular-interpolation problem, with the lead's brief
  telling it to end the run with `desk_close`.
