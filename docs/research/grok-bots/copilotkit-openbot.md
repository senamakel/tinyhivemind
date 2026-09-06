# CopilotKit OpenBot

## What it is

- **URL** — <https://github.com/CopilotKit/OpenBot>
- **SHA read** — `175678185c5a2ca69f032b0ac2b5bb4b9fcfda76` (2026-09-05)
- **Licence** — MIT (`LICENSE:1`)
- **Size** — 12 MB working tree, 752 tracked files, 557 TypeScript/TSX files,
  ~128k lines of TS/TSX/Python. Bun + Hono + Drizzle/PostgreSQL on the server,
  TanStack Router + React on the client, Docker/Kubernetes for the per-bot
  computers.
- **Self-description** — "AI coworkers you can hand real work to, and actually
  trust with the access" (`README.md:5`), explicitly "a template, not a product"
  and "alpha" (`README.md:30`, `README.md:32`).

The code is unusually well commented: nearly every design decision carries a
paragraph saying which production failure produced it. Those comments are the
most valuable thing here, and several are cited below because they document
bugs `tinyhivemind` would hit in the same shape.

## Architecture

Seven deployables (`README.md:239-247`, `docker-compose.yml`):

| Piece | What it is |
| --- | --- |
| `app/` | React SPA. **Drives the chat turn from the browser.** |
| `server/` | Hono API: auth, channels, agents, plugins, audit, the computer gateway, and a mounted CopilotKit runtime (`server/src/copilot.ts`). |
| `agent-bot/`, `agent-langgraph/` | Reference bots — plain AG-UI HTTP services (`agent-bot/src/index.ts:1-17`). |
| `agent-computer/` | The in-container agent: Chromium, shell, files, snapshots. |
| `supervisor/` | Docker/K8s driver that creates one container + two volumes per bot (`supervisor/src/names.ts:47-93`). |
| `worker/` | Sweeps the durable work queue: handoffs, routines, computer culling. |
| PostgreSQL | Product data, policy, audit, grants, channel metadata. |

Crucially, **the transcript is not in PostgreSQL**. Messages live in *CopilotKit
Intelligence*, a hosted platform the deployment refuses to boot without
(`server/src/config.ts:588-625`, `server/src/copilot.ts:30-41`: "There is no SSE
branch… a deployment without it silently forgets every conversation"). Postgres
holds only a pointer: `intelligence_channel_mappings(user_id, channel_id) →
thread_id` (`server/src/db/schema/core.ts:462-479`).

## Bot identity and roster

A bot is two rows plus a package entry.

- `agents` — `id`, `name`, `type ∈ {built_in, remote_ag_ui}`, an opaque
  `configuration` JSON, `package_id`, `override`
  (`server/src/db/schema/core.ts:254-265`).
- `agent_profiles` — the human half, keyed 1:1 on `agent_id`: `owner_user_id`,
  `title`, `role_description`, `avatar_seed`, `visibility ∈ {public, private}`,
  `callback_token_hash`, `deleted_at`
  (`server/src/db/schema/coworker.ts:28-65`).
- `agent_preferences(user_id, agent_id, hidden_at)` — per-person hiding, so
  "active" is a *viewer-relative* fact, not a global one
  (`server/src/db/schema/coworker.ts:67-79`).

The projected shape a caller sees is `AgentProfile`
(`server/src/agents/profile-types.ts:8-30`): it merges the two rows and adds
derived booleans — `systemOwned`, `hidden`, `hasAuth`, `hasCallbackToken` —
never the secrets themselves.

**Registration** happens two ways. Declaratively from a tenant package
(`examples/fintech/agents.yaml`, loaded by `server/src/tenant-package.ts`), or
at runtime via `/agents` → `server/src/agents/routes.ts` →
`profile-store.ts`. A remote bot is registered by pasting an AG-UI URL; the URL
is validated with the same SSRF target checks used for browser navigation, at
registration **and again on every redirect** (`README.md:179`,
`server/src/agents/connection-test.ts`).

**Addressing** is by id, with name as a fallback. The resolution rule lives in
`server/src/agents/handoff.ts:201-254` and is worth copying verbatim: ids win
outright over names because `agents.name` has no unique constraint and
duplicating a bot deliberately makes a second one with the same name; an
ambiguous name is refused *by naming the candidate ids*; and "does not exist"
and "you may not see it" return the **same sentence**, so a bot cannot
enumerate the roster by reading which refusal came back.

**Active/inactive** is a fold, not a flag. Three pure predicates in
`server/src/agents/profile-policy.ts:104-126`:

```
canAccessAgent(actor, agent) = deletedAt === null &&
  (visibility === "public" || ownerUserId === actor.id || actor.role === "admin")
canManageAgent(actor, agent) = !systemOwned && deletedAt === null &&
  (ownerUserId === actor.id || actor.role === "admin")
canRunAgent = canAccessAgent
```

A deleted bot is not removed from the runtime: it is re-registered as
`RegisteredUnavailableAgent` so its old threads still render and every run is
refused without contacting the endpoint (`server/src/copilot.ts:65-74`). That is
a tombstone in the roster, and `tinyhivemind` has no equivalent.

## The gateway

"Gateway" means two different things in this repo, and only one of them is a
message router.

**1. The computer gateway** (`server/src/computer/gateway.ts`) is the one the
README is about. It routes on `botId` and does three things in order
(`gateway.ts:1-18`): resolve the caller's opaque element `ref` against *the
snapshot this server fetched*, never against a label the caller supplied; ask
the policy (deny beats allow, absent policy denies, broken rule denies); write
the audit row, then act. The comment names the attack the ordering exists to
stop: `"never click Submit"` is otherwise evaded by sending
`{ref: "e13", name: "Continue"}`.

Its state is deliberately *not* in memory. `computer_snapshot` is one row per
computer holding the ref→element map plus a generation and a session id
(`server/src/db/schema/computer.ts:198-242`) — because "OpenBot runs several
processes behind a load balancer, and the process that took the snapshot is
rarely the one that handles the click that follows it", so the boundary the
whole gateway exists for silently stopped applying. `computer_page_frame` is
keyed on `(computer_id, tool_call_id)` — the *turn*, not the page — so a past
turn's screenshot cannot change under a reader (`computer.ts:244-286`).

**2. The message gateway** is the CopilotKit Intelligence platform, and it is
not in this repo. The server mounts a runtime against it
(`server/src/copilot.ts`) and reaches into five `ɵ`-prefixed private methods to
drive headless turns (`server/src/routines/run-turn.ts:20-30`, which warns that
the shapes are "RESTATED BY HAND" and "nothing fails loudly when the package
changes them").

**Is per-bot isolation only a container boundary?** No — there are two
boundaries and they are of different strength.

- *Container*: per-bot when `COMPUTER_SUPERVISOR_URL` is set — one container,
  one `/workspace` volume, one browser-profile volume, all names **derived**
  from the bot id and never accepted from a caller
  (`supervisor/src/names.ts:1-13`, `:67-93`). Without a supervisor the provider
  degrades to `shared` and every bot uses one browser, sessions and logins
  included — the code says so plainly and surfaces a warning string
  (`server/src/computer/provider.ts:36-59`).
- *Logical, in the message model*: a **thread has exactly one agent**. This is a
  platform invariant, not a choice: "An Intelligence thread is owned by exactly
  one agent: `assertThreadAgentOwnership` refuses any other one"
  (`server/src/agents/handoff-delivery.ts:83-100`). Everything downstream —
  scratch threads, relays, the one-responder rule — falls out of it.

## Conversation model

The nesting is: **channel** (shared metadata) → **membership** (per person) →
**thread** (per person *and* channel) → messages (off-box, in Intelligence).

- `channels` carries channel-grain state only: name, summary, `last_message`,
  `last_message_agent_id`, soft `deleted_at`
  (`core.ts:267-349`). The comment explains the split explicitly: "what was said
  last is a property of the conversation, and a copy per member is the same fact
  stored N times, drifting."
- `channel_memberships(channel_id, user_id, pinned_at, last_read_at)` is the
  per-member half — pins and read state, nothing else (`core.ts:351-373`).
- `channel_agents(channel_id, agent_id)` is the roster of the channel
  (`core.ts:375-387`).
- `intelligence_channel_mappings(user_id, channel_id) → thread_id`
  (`core.ts:462-479`).

That last table is the load-bearing one: **the transcript is per person, not
shared.** Two people in one channel have two threads and never read each other's
messages. There is no shared medium here at all — a channel is a folder, not a
room. `channels.last_message` is the only cross-person view, and its own comment
admits it is "a cache of what a client observed rather than an authoritative
mirror of the thread" (`core.ts:293-303`).

**Does a bot see another bot's messages?** Only through one narrow, computed
path. A forward handoff runs the addressed bot in a *scratch thread* (never
mapped to a channel, so nobody is shown it), and seeds it with the asking
thread's history passed through a filter
(`handoff-delivery.ts:255-267`, `:436-460`):

```ts
function conversationOnly(messages) {
  return messages.filter(m =>
    (m.role === "user" || m.role === "assistant") && textOf(m.content).length > 0);
}
```

That is the entire visibility fold: keep user and assistant text, drop every
tool call and tool result. Two reasons are given, and both are ours: a stored
thread "is not a valid prompt on its own" (the platform keeps tool *results* but
not the assistant message that made the call, so the pairing dangles), and "the
addressed Bot is being brought into a conversation, not into another Bot's
workings."

A second, larger fold sits beside it. `sanitizeSeededHistory`
(`server/src/agents/history-sanitize.ts`) drops dangling tool calls before any
turn replays history — written after the same failure was found twice in
production, where one interrupted turn poisoned *every later turn* in that
thread ("a permanent failure grown out of transient damage, and nothing the
person did wrong", `:21-40`). Its rules are stated as an ordered list, ids are
never rewritten, and unchanged messages are returned as the same object. It is a
pure function over `Message[]` in everything but its import of AG-UI types.

## Turn triggering

There are exactly three things that start a turn.

**1. A person sends a message.** The turn is driven *from the browser*
(`server/src/channels/routes.ts:200-205`: "the runtime does not tell this
deployment when a person's turn begins. So the browser reports it"). The
composer parses the draft into `{text, agentId, commandIds}` where `agentId` is
`agentChips.at(-1)` — the **last** `@` chip
(`app/src/components/channels/composer/draft.ts:32-42`) — and
`enforceSingleAgent` physically deletes every earlier agent chip from the
segment list (`draft.ts:45-61`). The type comment states the rule outright:
"Exactly one responding agent per message." There is no `@everyone`, no
`@desk`, no `@this`, and no broadcast of any kind.

**2. A routine fires.** `routines` is a cron row owned by a person; the sweep
CAS-advances `next_run_at` and the worker runs one headless turn as the owner
into the channel they asked in (`server/src/db/schema/coworker.ts:87-131`,
`server/src/routines/run-turn.ts`). Caps: a 15-minute floor, 20 enabled
routines per person, and ten consecutive failures disable the routine
(`README.md:166`).

**3. One bot hands work to another.** This is the fan-out mechanism and it is
the closest analogue to `mention-dispatch.md`. `server/src/agents/handoff.ts`
*decides*; `handoff-runner.ts` + `handoff-delivery.ts` *deliver*, separated by a
durable Postgres queue because "deciding happens inside somebody's run and must
be fast and fail closed, while delivering is a whole agent turn that has to
survive the pod it started on" (`handoff.ts:8-12`).

The bounds, all enforced server-side:

| Bound | Where | Note |
| --- | --- | --- |
| `maxDepth` | `handoff.ts:159-169` | Counter travels in the **signed run assertion** (`callback-token.ts:88-100`) because "A to B to C is three runs on up to three pods, and a counter held in a variable stops applying the moment the second hop lands somewhere else." `0` disables handoff entirely. |
| `maxPerRun` | `handoff.ts:308-317`, `:171-179` | **Not** checked before offering. Counted *inside the queue insert* over rows sharing a hashed run prefix, because "five calls passed a cap of three, every time, on a single pod" when a model emitted several tool calls at once. |
| Grant | `handoff.ts:265-274` | `mayAddress(from, to)`, read per hop, never cached. |
| Roster scope | `handoff.ts:181-201` | Target resolved against the roster **the asking person** may see, with the role fetched per hop (a hardcoded `role: "user"` refused administrators' own hops). |
| Idempotency | `handoff.ts:276-306` | Key = `hop:H(actor,run)` + `H(target, task, constraints, expecting)`. The run id is hashed rather than interpolated, after a run calling itself `notice` collided with the deployment's failure-notice keyspace. |
| Self-address | `handoff.ts:256-263` | Refused. |

**Every refusal is a sentence, never an exception** (`handoff.ts:14-16`,
`handoff-tool.ts:524-533`) — the asking bot is mid-run with a person waiting, so
a throw reads as the bot ignoring them. The tool is also *withheld* from a run
already at the depth cap, so the model does not spend attention on a door it
cannot open (`handoff-tool.ts:475-493`).

**Approval gate:** there is no human approval on the handoff path. Grants and
caps are the only gate. Human approval exists in two other places: the computer
policy (CEL expressions, deny-beats-allow, `dry-run` vs `enforce`,
`server/src/computer/policy.ts:1-33`), the control handover (while a person is
driving, bot actions are *refused*, not queued — `README.md:154`), and
`ask_person`, an escalation tool deliberately **not** gated on a grant, "because
a deployment could otherwise switch off the safe exit and leave the expensive
one" (`server/src/agents/escalation.ts:74-80`). The escalation *route* is a
seam: the shipped one just tells the model to ask the person in front of it
(`escalation.ts:50-60`).

The answer never appears in the addressed bot's own channel. The runner relays
it back through the **asking** bot, in the conversation the person is watching,
attributed — because a thread has one agent so the addressed bot literally
cannot speak there (`handoff-runner.ts:145-174`). A relay carries `answerIn`,
and that same marker is what stops a relay relaying and a failure notice
recursing.

## Harness / model swapping

The interface is **AG-UI over HTTP**, and nothing else. A bot is any endpoint
speaking it (`README.md:48`, `server/src/copilot.ts:30-41`), reached as an
`HttpAgent`; built-in bots are `BuiltInAgent` instances in-process. There is no
framework adapter in the repo — LangGraph, Mastra, Pydantic-AI examples all just
serve AG-UI (`examples/`).

What crosses the seam to a remote bot:

- **One system message.** The standing role is an ordinary AG-UI system message,
  not `forwardedProps`, "because the endpoint on the other side may be
  LangGraph, Mastra, ADK or a hand-written server, and a system message is the
  only thing all of them already understand"
  (`server/src/copilot.ts:92-100`). Its id is derived from the agent so a copy
  can be recognised and refused.
- **A signed run assertion in `forwardedProps`** — `{botId, actorId, runId,
  threadId, depth, exp}` — deliberately not a header, "because that is the part
  of an AG-UI run a framework hands back to the code the customer writes"
  (`server/src/agents/callback-token.ts:104-113`).
- **A callback token**, per agent, stored only as a SHA-256 hash
  (`callback-token.ts:1-21`). Token says *which agent*; assertion says *which
  bot and person this run is for*. The two are checked against each other so a
  replayed assertion cannot be spent as somebody else's bot.
- **Tools in `input.tools`**, resolved per person per run. Built-in bots get
  them via `LoadToolsForBot`; the reference bot publishes none of its own
  (`agent-bot/src/index.ts:9-17`).

Model swapping for built-ins is a `provider/model` string plus a vault-resolved
key (`copilot.ts:builtInAgentConfiguration`); a missing key produces an agent
whose *first iteration throws a readable sentence* rather than a bot that
silently fails.

One notable prompt-assembly fold: role → the person's standing instructions →
granted-tool guidance → computer guidance, in that order, with the ordering
justified by observed failures (a bot that read the emphatic browser prose last
reached for the browser even holding the right tool; a person's instruction
"always answer in one line" silently overrode a role that existed to produce a
sourced filing, until a precedence sentence was made part of the block)
(`copilot.ts:187-300`).

## Persistence and storage ownership

Split across three owners, and the split is the interesting part.

1. **PostgreSQL** — users, roles, agents, profiles, channels, memberships,
   grants, credentials (encrypted, with a partial unique index so two replicas
   cannot both insert a live secret, `core.ts:389-410`), audit, policy, work
   queue, snapshots, page frames.
2. **CopilotKit Intelligence** (hosted, off-box) — every message, every thread,
   thread locks (Redis-backed, keyed by thread), and memory. Required at boot.
3. **Docker volumes** — per-bot `/workspace` and browser profile.

Two mechanisms are worth stealing regardless of storage choice:

- **The work queue** (`server/src/work/queue.ts`): `select … for update skip
  locked`, leases, and — the load-bearing detail — *every moment named in SQL*,
  because "leases used to be computed as `Date.now() + leaseMs` on the replica
  and compared against `now()` in Postgres, which is two clocks pretending to be
  one" (`queue.ts:12-17`). `attempts` is exposed as a number, not folded into a
  status, because "ONE MEANS IT HAS CERTAINLY NOT RUN" and anything more may
  have already spent money (`queue.ts:66-77`).
- **`repeatAfterEach`** (`server/src/work/loop.ts`): schedule the next sweep
  when the last one *ends*. With `setInterval`, overlapping sweeps do not
  contend — they each claim a *different* batch, "which is the worse failure",
  and a two-second interval starts 150 more sweeps inside one long delivery.

Audit is append-only with no foreign key to `users` (a cascade would be an
update the trigger refuses) and four covering indexes, because "the trail
becomes the largest table in the deployment within weeks" (`core.ts:412-460`).
Idleness is read *from the audit trail* rather than by dialling the computer,
since asking would wake it and "the bill would never fall"
(`server/src/work/culler.ts:1-13`).

## Pure decision logic vs genuinely IO-bound

**Already pure, or one refactor away** — every one of these is a fold over
arguments the caller already holds:

| Function | Path |
| --- | --- |
| `canAccessAgent` / `canManageAgent` / `canRunAgent` | `server/src/agents/profile-policy.ts:104-126` |
| `toDraft` / `enforceSingleAgent` / `applyCommandChips` | `app/src/components/channels/composer/draft.ts` |
| `conversationOnly` (the visibility filter) | `server/src/agents/handoff-delivery.ts:451-460` |
| `sanitizeSeededHistory` (transcript repair) | `server/src/agents/history-sanitize.ts` |
| target resolution + refusal wording | `server/src/agents/handoff.ts:201-274` (currently entangled with two awaits) |
| `selectTools` narrowing and its `SelectionReason` | `server/src/plugins/selection.ts` |
| `evaluateActionPolicy` (deny-beats-allow, fail-closed) | `server/src/computer/policy.ts` |
| `namesFor` (id → container/volume names) | `supervisor/src/names.ts:67-93` |
| `createThreadIdentity` (mint/owns) | `server/src/channels/thread-identity.ts:69-88` |
| cursor encode/decode, `channelName`, `previewOf` | `server/src/channels/routes.ts:88-226` |
| `standingRoleMessage`, `standingInstructionsGuidance`, prompt assembly | `server/src/copilot.ts:100-300` |
| `attribute` / `summarise` (the two texts of a hop) | `server/src/agents/handoff-runner.ts:530-573` |

**Genuinely IO-bound** — lease renewal against the database clock, `for update
skip locked` claiming, the thread lock (`acquire`/`renew`/`release`, NX so a
busy conversation refuses rather than queues), the fan-out cap (it *must* be the
same statement as the insert), snapshot resolution, the model call itself, and
every container lifecycle operation.

The most instructive boundary case is the fan-out cap. It looks like arithmetic
and cannot be: checking a count and then writing is "a cap that holds only while
nothing else is offering, and the case it has to hold in is precisely the
opposite one" (`handoff.ts:171-179`). The *policy* (`maxPerRun`, `maxDepth`,
which refusal sentence) is a fold; the *counting* is a database primitive. That
is exactly the core/port line `tinyhivemind` draws, arrived at independently
from a production incident.

## README overclaims

- **"a coworker with a channel of its own"** (`README.md:23`) and the handoff
  module's own opening — "A person can put several Bots in a channel and address
  them with `@`" (`handoff.ts:4-5`) — are not true of the shipped UI. The
  channel view refuses to render any channel with more than one agent: *"This
  channel has more than one coworker, which is not supported yet"*
  (`app/src/routes/_authed/_app/channel/$channelId.tsx:283-291`). The server
  model supports N agents per channel and the example package ships a
  two-agent channel (`examples/fintech/channels.yaml:7-11`), which is therefore
  unopenable.
- **`channels.allowed_groups`** is written by the package loader and *read by
  nothing*; `users.groups` is empty on every row because no sign-in path
  populates it. The schema comment says so itself: "It is a control the name
  promises and nothing keeps" (`core.ts:274-284`, `:58-66`). Package channels
  get no membership rows either, so they are unreachable rather than open —
  which the comment notes is luck, not design.
- **"one gateway that decides and records it"** (`README.md:42`) covers computer,
  file, MCP and component calls. Bot-to-bot handoff does **not** go through that
  gateway; it is an ordinary granted tool with its own audit events.
- **"Every action decided before it happens"** (`README.md:5`) holds only when a
  policy is configured. An absent policy denies, which is the right failure, but
  `mode: "dry-run"` decides and records while letting everything through
  (`policy.ts:21-28`) — deliberately, and worth knowing before quoting the line.
- **Isolation** silently degrades to one shared browser (shared logins, shared
  files) when `COMPUTER_SUPERVISOR_URL` is unset. The code raises a warning
  string; the README's headline sentence does not carry the condition
  (`provider.ts:51-59` vs `README.md:5`).

## Mechanism → tinyhivemind has → does not have

| Mechanism in OpenBot | tinyhivemind has | Does not have |
| --- | --- | --- |
| Bot = `agents` row + `agent_profiles` row, projected to `AgentProfile` | `RosterRecord` in `mentions.md` | An owner, a visibility enum, and a *per-viewer* hidden flag as first-class roster fields |
| `canAccessAgent` / `canRunAgent` as pure predicates | roster + desk membership folds | A tombstone state: a deleted participant kept registered so old transcripts still render, every run refused |
| Addressing: id beats name, ambiguity refused *by naming ids*, "no such bot" ≡ "not yours to see" | mention grammar + normalization | The enumeration-resistant refusal rule, and duplicate-name ambiguity as an explicit outcome |
| One `@` chip per message, enforced by deleting earlier chips | one-message-one-turn; `@everyone` is a list | Nothing — this is the same invariant, reached independently |
| Thread = one participant's view; a thread has exactly one agent | projection fold over a shared transcript | The inverse: OpenBot has no shared medium at all, so it has nothing to teach us about a room; but its *scratch thread* is a working answer to "where does a delegated turn run" |
| `conversationOnly` visibility filter across a hop | projection with visibility filters | An explicit "tool traffic never crosses a delegation boundary" rule, and the reason (a stored transcript is not a valid prompt) |
| `sanitizeSeededHistory` | — | Transcript *repair* as a read-side fold: dangling tool calls dropped, ids preserved, unchanged messages returned identically |
| Handoff: decide (fast, fail-closed) / deliver (durable queue) split | `mention-dispatch.md`'s port boundary | Nothing structural — but the *reason* (deciding is inside a run, delivering must outlive the pod) is a sharper statement than ours |
| Depth cap carried in a **signed** assertion across processes | hop-bounded dispatch | Signing the hop counter so no participant can edit it; our bound assumes an honest host |
| Fan-out cap counted *inside* the queue insert | one-message-one-turn makes fan-out unrepresentable | The concurrent-tool-call failure mode, which we avoid by construction rather than by counting |
| Idempotency key = H(actor,run) + H(envelope) | dispatch idempotency | Hashing the caller-supplied run id so it cannot poison a shared key prefix |
| Refusals as sentences the model can say, never exceptions | typed `Error` per crate | A refusal *vocabulary* aimed at the model: every decline is text a bot can repeat to a person |
| Tool withheld at the depth cap | — | Shaping the *offered* action set by remaining budget, not just refusing at the edge |
| `ask_person` escalation, ungated, competing with delegation | `!defer` in `expert-delegation.md` | An explicit "ask a human" outcome ranked beside "ask another agent", with the route as a host seam |
| Typed handoff envelope `{task, constraints, expecting}` | `BidContext`, grounds/refutation | The named-fields-over-free-text argument for what one agent sends another |
| Two texts per hop: one for the model, one for the transcript | attributed `SessionMessage` | The split itself — a delegated turn's prompt and its one-line transcript entry are different artefacts |
| Relay: the answer comes home in the *asking* agent's voice, attributed | cross-desk referral returns one answer | The attribution rule for a relayed answer, forced by one-agent-per-thread |
| CEL policy, deny-beats-allow, dry-run mode | — | Out of scope (host-side) |
| Work queue: DB clock everywhere, `attempts` exposed | ports; host owns storage | Nothing to add to core — but a `MentionTurnQueue` port doc should say the lease clock is the store's, and that attempt count must be visible |
| `repeatAfterEach` | — | Out of scope (host-side) |
| Prompt assembly order with per-failure justification | — | An ordering rule for role vs. standing instructions vs. capability guidance, and a precedence sentence inside the block |
