# Team initialization

A team briefing is ephemeral input to a model session, separate from durable
history. It identifies the viewer and desk, lists teammates deterministically,
and explains attribution and mention safety. Hosts can provide richer optional
role and description strings directly, or derive a conservative briefing from
validated core desk and roster snapshots.

The rendered rules are P7-aware: a direct `@agent` mention may cause at most
one child turn only when host policy enables it, while person, desk, and
`@everyone` mentions remain context and never fan out. The briefing does not
carry or choose that policy. The host supplies a finite configurable
`max_hops` for each dispatch decision, and the library imposes no smaller hard
ceiling.

`initialize_session` keeps this briefing separate from projected messages. It
therefore has no sequence number, consumes no history window, and is never
written back through this crate.

## Audience

`TeamBriefing::asides` gates the aside grammar in `system_text`. A grammar is a
fixed cost paid in every agent's prompt on every turn, so teaching a move
nobody may make spends that budget for nothing.

An enabled desk also gains a fourth shared-session rule, stated as something an
agent can act on rather than as a disclaimer: some rows show only that an aside
happened, you cannot read them, and if one matters you ask its author in the
desk. An agent that is not told its view may be narrower than a peer's reads
silence as disagreement rather than as absence.

`BrevityPolicy::window` states the window a viewer actually received when the
projection elided something, rather than the nominal one. Only a projection
that actually elided is restated, so a young desk still reports the window it
will grow into.
