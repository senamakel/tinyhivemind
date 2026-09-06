# Session projection

This module walks a host-owned, globally sequenced log without owning storage.
The `SessionLog` port returns newest-first pages through boxed futures so it is
object-safe and independent of an executor. `project_session` validates every
page, counts all raw rows against a fixed scan budget, filters one desk/channel
or direct-child thread, and returns attributed messages chronologically.

The host must treat `before` as exclusive and return `next_before` no newer than
a nonempty page's oldest sequence and strictly older than the requested cursor.
Empty pages end a walk. A malformed page is a typed error rather than a reason
to retry or silently accept incomplete order. Reaching a thread root ends the
walk even when blank-content filtering omits that row from the projection.

## Audience

`SessionQuery` carries a `Viewer`, and a row whose `Audience` does not admit it
is **elided rather than dropped**: it keeps its sequence, its author and its
audience, and loses only its content. Dropping it would leave `^N` citations
naming nothing and would hide from a peer that there is something to ask about.

A run of elided rows from one aside collapses into a single stub carrying the
sequence range, the message count and where the aside settled. Collapsing runs
after the window is filled, so a viewer with asides in view receives fewer
messages than one without — the alternative is backfilling from older history,
which would make two viewers of the same desk disagree about how far back the
window reaches. `project_as` applies the same narrowing to a transcript a
caller already holds.
