# grok-bot-0.18-reconstructed — a survey for `tinyhivemind`

## What the project is

- Repository: <https://github.com/b-nnett/grok-bot-0.18-reconstructed>
- Commit read: `a9f633e09d49a85829b8236331b9e21f7e612634` (2026-08-23,
  "Document project and preserve original installers"), the only commit on a
  shallow clone of the default branch.
- Licence: **none.** There is no `LICENSE` file. `NOTICE.md` states that "no
  upstream source-code license is asserted or granted here" and asks anyone
  redistributing to run their own rights review. Nothing here may be copied into
  `tinyhivemind`; this is a read-and-learn exercise only.
- Size: 2,111 files. About 177,000 lines of hand-written TypeScript under
  `source/` (a further 264,000 lines there are generated protobuf), plus 52,000
  lines of React/TypeScript under `frontend/`. Eight `node --test` files under
  `tests/`, all of them packaging/publication regressions plus two router tests.

It is an unofficial, source-oriented reconstruction of the shipped Grok Bot
0.18.0 macOS app (Anysphere; upstream bundle id `com.anysphere.sand`, hence the
`sand`/`Sand` prefix on almost every identifier). The reconstruction was
recovered from the compiled `app.asar`, so module boundaries and names are
inferred. `PROVENANCE.md` sets an "evidence-only reconstruction rule": recovered
source may express only behaviour supported by an inspectable artifact anchor.
Thirteen modules are honest empty stubs carrying a comment such as "No named
runtime surface survived tree shaking" (for example
`source/packages/agent-core/conversation-actions/steer-outbox.ts:1`).

On top of the reconstruction the author added four things of their own: an
inference router, routed MCP tooling, local usage counters, and an optional
local Docker sandbox.

## Architecture as found

Four processes, each a separate source root:

- `source/electron-main/` — desktop lifecycle, settings, auth, box connectors,
  and ownership of the coordinator child process.
- `source/electron-preload/` — the trusted bridge to the renderer.
- `source/node-agent-coordinator/` — the process that actually talks to
  inference providers. Its control channel is `source/shared/rpc/coordinator-port.ts`,
  a five-variant frame union (`lifecycle`/`request`/`cancel`/`reply`/`event`) with
  a pure parser, `parseCoordinatorFrame` (`coordinator-port.ts:35`), returning
  `{accepted:true, frame}` or `{accepted:false, rejection}` rather than throwing.
- `source/host/` — the interesting layer: sessions, transcripts, turns, groups,
  tools, permissions.

The host is assembled from 35 named extensions
(`source/host/extensions/extension-ids.generated.ts:1`) started through a tiny
DI graph in `source/internal/host-extensions.ts`. `defineHostExtension`
(line 16) takes `{id, dependencies, start}`; `resolveHostExtensionBootOrder`
(line 57) is a **pure topological sort** over the declarations that raises named
errors for a duplicate id, a self-dependency, a missing peer, and a cycle (with
the cycle path rendered by `describeCycle`, line 113). `startHostExtensions`
(line 84) is the impure half: it walks that order, injects each extension's
peers' APIs, and unwinds teardowns in reverse on failure. The pure/impure split
here is the same one `tinyhivemind` makes, and it is done well.

The renderer reaches the host over a flat command table:
`SAND_GATEWAY_COMMANDS` in `source/host/gateway-protocol.ts` maps **122** string
command names onto methods of one `GatewayApi` object. That object is `any`
(`gateway-protocol.ts:2`) — the table is a dispatch switch, not a typed
boundary.

## Agent identity and the roster

There is no single `Agent` type. Identity is scattered across four persisted
surfaces per agent, and reassembled on demand.

The closest thing to a schema is `AgentMetadata`
(`source/packages/agent-kv/agent-store.ts:40`), zod-validated at line 76 and
stored hex-encoded under the key `"metadata"` in the agent's SQLite `kv` table:
`agentId`, `latestRootBlobId`, `name`, `mode` (`"default"|"plan"|"debug"|"search"`),
`isRunEverything`, `approvalMode` (`"allowlist"|"unrestricted"|"auto-review"`),
`createdAt`, `lastUsedModel`, `lastDebugServerPort`, `currentPlanUri`,
`subagentInfo` (`parentAgentId`, `rootParentAgentId`, `toolCallId`, `typeName`),
`blobEncryptionKey`.

Alongside it, on disk in the same directory: `profile.json`
(`SandAgentProfile` — `name`, `description`, `title`, `avatarShape`,
`avatarColor`; `source/host/agents/agent-profile.ts:6`), `settings.json`
(`notifyOnAgentUpdates`, `hiddenFromSidebar`;
`source/host/agents/settings-file.ts:8`), and further KV keys on the DB
(`unreadState`, `awaitingUserResponse`, `origin`, `purpose`,
`conversationPartners`; `source/host/extensions/session/agent-db.ts:62`).

**The roster is the filesystem.** `listAgents`
(`source/host/extensions/session/session-roster.ts:7`) does
`readdir(host.rootDir)`, treats every directory as an agent, `stat`s its
`store.db`, and folds each into a summary via `buildSummary`
(`session-summaries.ts:16`), sorting by `updatedAt` descending. There is no
roster table, no roster record type, and no handle namespace: the agent id *is*
the directory name, validated by `assertValidSandAgentId`. Caps are constants —
`MAX_AGENTS_PER_USER = 50`, `GROUP_MAX_MEMBERS = 6`
(`source/shared/agents/agents.ts:53`).

`buildSummary` returns an anonymous object; the shape of a roster entry is
nowhere declared as an interface. Only the *input* is typed: `DbExtras`
(`session-summaries.ts:8`). The one genuinely pure piece is
`upsertAgentSummary` (`source/shared/agents/agent-summaries.ts:13`) — an
array-in, array-out fold keyed on `.id` and re-sorted by `updatedAt`. Everything
around it is stateful: `RosterProjection`
(`source/host/extensions/transcript/roster-projection.ts:25`) is an
`EventEmitter` with six mutable maps and a debounced flush, despite the name.

Human versus agent is only discriminated in two places, and only for group
contexts. `GroupMessage.speaker` is a tagged union of
`{kind:"user", name?}` and `{kind:"member", id, name}`
(`source/host/groups/group-chat.ts:2`). Cross-user rooms are explicit:
`RemoteRoomMember = {kind: "agent" | "human", authId, agentId, displayName,
avatarUrl?}` (`source/host/groups/remote-room-store.ts:1`). A local group is
agents-only — `SandGroupConfig` is `{version, memberIds[], remoteMembers?,
sharedRoomId?}` (`group-store.ts:1`) — with the human implicit as the sender.

Per-agent capability configuration barely exists. The model is a **global**
setting (`SandStoredSettings.agentDefaultModel`,
`source/shared/node/settings/sand-settings-store.ts:21`), resolved at inference
time with experiment overrides layered on
(`source/host/extensions/inference/inference-service.ts:13`);
`AgentMetadata.lastUsedModel` records what was used, it does not configure.
MCP tool policy is likewise global (`mcpDisabledToolsByServerId`). The only
per-agent persona is `GroupMember.description`, consumed by
`buildGroupMemberSystemPrompt` (`group-chat.ts:15`).

## The conversation unit, and how a transcript is addressed

The unit is **the agent**. One agent is one conversation is one directory is one
SQLite file (`store.db`, `session-paths.ts:9`). A group chat is itself an agent
directory with a `group.json` beside it. There is no separate session, thread,
or channel entity — "channels" (`session/channel-store.ts`) are external
connector bindings (Discord, Slack), not conversation scopes. Threads exist only
as a stamp on entries (`send-thread-stamping.ts`) and a `getAgentThread` gateway
command.

The transcript schema is three columns
(`source/host/extensions/session/agent-db-schema.ts:15`):

```sql
CREATE TABLE IF NOT EXISTS transcript_entries (
  seq INTEGER PRIMARY KEY,
  id TEXT NOT NULL UNIQUE,
  entry TEXT NOT NULL
) STRICT;
```

So a message is addressed **both** ways: by monotone `seq` for paging, and by a
stable string `id` for reference. This is very close to what `tinyhivemind`
assumes, with one difference worth noting — the string ids are *derived from
transcript position*, not minted randomly. `nextEntryId`
(`source/host/extensions/transcript/transcript-entry-ids.ts:5`) is a **pure fold
over the entries the caller already holds**: it counts user messages to get a
turn number and mints `t3u`, `t3a0`, `t3s1`, `t3ua0` — turn, kind, ordinal —
probing with `firstUnusedId` until it finds a free one. A boot turn is `"b"`.
The whole file (78 lines) is pure and would port to Rust unchanged.

Paging is a fold over rows too. `readTranscriptPage` / `readTranscriptWindow` /
`readTranscriptTail` (`agent-db-transcript-pages.ts:14-16`) take a prepared-
statement bundle plus `{beforeSeq, sinceMs, untilMs, limit}`, over-fetch by one
to detect more, reverse the rows, and return `{entries, nextBeforeSeq?}`. The
cursor is a `seq`. Visibility is pushed into SQL as reusable filter fragments —
`WINDOW_ENTRY_FILTER_SQL` drops tool-calls and branched entries;
`MAIN_TRANSCRIPT_MESSAGE_FILTER_SQL` keeps only sends, attachments, user
messages, and messages carrying `fromAgent`/`toAgent`
(`agent-db-schema.ts:1,27`). That filter *is* the projection, expressed as a
predicate over `json_extract` rather than as a fold — which is exactly the piece
`tinyhivemind` keeps pure and portable.

`session-projection.ts` holds the small pure derivations on top:
`getLastEntryFromTranscript` (line 14) filters `hidden`/`branched`/`peerAgentId`
entries and walks backwards for a preview; `collectLastAttachmentBatchKinds`
(line 10) folds a trailing attachment batch. `transcript-store.ts` is an
immutable-update module-level cache (`appendEntry`, `updateEntry`,
`removeEntry`) — pure functions over one hidden `let`.

## How a message causes a turn

There are two paths, and they follow different rules.

**One-to-one.** `SendPipeline.sendPrompt`
(`source/host/extensions/transcript/send-pipeline.ts:100`) dedupes on a
`clientNonce`: an in-flight nonce coalesces onto the running promise; an already
accepted nonce with a matching digest is a no-op; a reused nonce with a
different digest raises `PromptAcceptanceDigestMismatchError`. The digest is a
SHA-256 over canonicalised `(agentId, prompt, richText, replyToId, isFork,
attachmentPaths, attachmentNames)` (`source/shared/send-acceptance.ts`), and the
ledger persists to `send-acceptance.json` with `MAX_RECORDS = 256`
(`prompt-acceptance-ledger.ts`). `dispatchUserTurn` then mints one epoch and
enqueues exactly one run. Any in-flight turn on that session is *interrupted*,
not run alongside (`send-turn-dispatch.ts:129`). This is one message, one turn.

**Group.** `send-group-fanout.ts:46` also enqueues exactly one task — but that
task is `runGroupTurn`, which builds a `GroupChatOrchestrator` and runs a
bounded sequential round-robin (`group-chat-orchestrator.ts:45`):

```ts
for (let round = 0; round < GROUP_MAX_ROUNDS; round += 1) {
  const responderIds = resolveResponders(members, this.deps.readHistory())...
  for (const memberId of orderRoundSpeakers(responderIds, round)) {
    if (totalMessages >= GROUP_MAX_MEMBER_TURNS || !this.deps.isCurrent()) return;
    const sent = await this.runOneTurn(args.group, member, members);
```

Each member turn is `await`ed — serial, never `Promise.all`. Bounds are
constants: `GROUP_MAX_ROUNDS = 3`, `GROUP_MAX_MEMBER_TURNS = 10`,
`GROUP_MAX_MESSAGES_PER_TURN = 2`, `GROUP_PROMPT_HISTORY_LIMIT = 24`
(`group-chat.ts:1`). A round that produces no messages ends the episode
(line 74). So the answer is neither fan-out nor one-turn: it is **one message,
one bounded sequential episode of up to ten turns**, cancellable by epoch.

### The mention grammar

It exists, it is host-side, and it applies only in group chats. There is no
mention parsing anywhere in the `send-*` pipeline; `send-message-shaping.ts`
carries `richText` opaquely.

- `memberMentionHandles(name)` (`group-chat.ts:7`) derives handles from a
  display name: lowercased whole name, whitespace-stripped variant, and the
  first token. Three handles per member, no explicit `@handle` field anywhere.
- `hasMentionAt(lower, handle)` (line 8) is a word-boundary scan for `@handle`
  using an ASCII `[a-z0-9]` boundary test.
- `parseGroupMentions(text, members)` (line 9) returns
  `{isEveryone, memberIds}`, where `isEveryone` is
  `/(?:^|[^a-z0-9])@(everyone|all)\b/.test(lower)`.
- `resolveResponders(members, history)` (line 10) walks back to the last user
  message, unions mentions across every message since, and returns **all members
  if `@everyone`/`@all` appeared *or if nothing was mentioned***; otherwise the
  mentioned subset.

All four are pure functions of `(text, members)` or `(members, history)`. The
"unmentioned means everyone" default is the notable design choice, and it is
what turns a plain group message into a ten-turn episode.

Two more pure pieces sit next to them and are directly relevant to
`tinyhivemind`'s response-threshold work: `isPassContent` (line 11) recognises
`(pass)` — group members are instructed to send exactly `"(pass)"` when they
have nothing to add, and `isPotentialPassPrefix` lets a stream be suppressed
before the word finishes. `orderRoundSpeakers` (line 3) rotates the speaker
order by round so the same member does not always open.
`messagesSinceMemberLastSpoke` (line 16) is the per-viewer projection: it slices
the history from that member's last utterance, which is what each member's turn
prompt is built from (`buildGroupTurnPrompt`, line 18).

### Scheduling

`SandRunScheduler` (`run-scheduler.ts`) keeps one queue per `agentId` with three
lanes — `pendingUser`, `pendingAgent`, `pendingBackground` — and a single
`active` slot: strict single-flight per agent. `takeNextUserTask` (line 45)
prefers a non-`"group-member"` source within the user lane, so a direct message
to a member jumps ahead of that member's pending group slot. A generation
counter guards against a late settlement clobbering a newer run (line 259).

There is a watchdog: after `RUN_WATCHDOG_DEFAULT_MS = 120_000` with a user-lane
item waiting, `onWatchdogFired` interrupts the wedged run; after a
`watchdogGraceMs = 30_000` grace, `escapeWedgedRun` force-resolves the stuck
promise, parks it in a `zombies` set, and pumps the next task rather than
deadlocking. `TurnRuntime.runTurn` (`turn-runtime.ts:327`) short-circuits a
stale epoch as `"superseded"` (line 367), and `ensureUserReply` (line 534) will
re-invoke the runner up to `MAX_REPLY_NUDGES = 3` times if the model owed a
delivery and produced none, each nudge re-checking the epoch.

### Agent-to-agent messaging

An agent addresses another by explicit id through a `SendToAgent` tool
(`source/host/agents/agent-messaging.ts:6`), not by parsing `@`. Guards are
thin (`agent-to-agent-messaging.ts:60`): empty message, self-message, deleted
target, remote-room target. Text is clamped at
`AGENT_MESSAGE_MAX_TEXT_LENGTH = 8_000`. Delivery is asynchronous — the message
is appended to `pendingAgentInbound` and drained by `reviveForAgentInbound`
(line 149), which enqueues on the recipient's `"agent"` lane.

**There is no hop bound, TTL, or cycle detector between distinct agents.** A→B→A
ping-pong is prevented only by prompt text: "Respond only when you actually have
something to say or were asked something — if there is nothing to add, just
stop, so two agents never ping-pong acknowledgements"
(`agent-messaging.ts:76`). Contrast the group path, whose bounds *are*
code-enforced. The `loop-detection` package
(`source/packages/agent/loop-detection/`) does not help here: it detects
*textual* self-repetition within a single generation (repeated lines, repeated
periods, box-drawing runs) and aborts with `AgentLoopError` — it is not a
recursion guard.

## The coordinator/host split, and what counts as a port

`source/host/ports/` is not a hexagonal port layer. It is six files of
constants, error classes, and small adapter factories: `box.ts` is user-facing
strings plus `boxNotReadyMessageForError`; `transport.ts` (5 lines) is a
last-message-id tracker; `mcp-state-executor.ts` groups tools by provider into a
protobuf. Naming it `ports/` overstates it.

The real seams are elsewhere and they are good ones:

- `TurnExecutor` (`host/extensions/turn-execution/extension.ts:10`) — three
  methods (`isInferenceReady`, `createRunner`, `createGroupMemberRunner`) bound
  late by the composition root, with an explicit double-bind error: "a second
  executor would mint a second runner for the same agent" (line 8).
- `GroupOrchestratorDeps` (`group-chat-orchestrator.ts:17`) — six methods
  (`resolveMembers`, `readHistory`, `isCurrent`, `runMemberTurn`,
  `postMemberMessage`, `finalizeMemberTurn?`). The orchestrator itself does no
  IO. This is the closest analogue in the repository to `tinyhivemind`'s hive
  episode: a bounded loop over a history the caller supplies, with waiting
  pushed behind an interface.
- `HostRosterBookkeeping` (`host/host-roster-bookkeeping.ts:46`) — six named
  single-purpose interfaces (`AttachmentRosterPort`, `TranscriptRosterPort`,
  `ForeverBoxRosterPort`, `SnapshotBackstopPort`, `BoxStoreSnapshotPort`,
  `SourceMapRosterPort`) resolved by an overloaded `api(id)`.
- `TranscriptPageStatements` (`agent-db-transcript-pages.ts:5`) — the paging
  functions depend on three prepared statements, not on a database.

Against that, `host/host-runner-composition.ts` is 2,646 lines and
`host/sand-host.ts` is 958; `TranscriptManagerLike = Record<string, any>`
(`transcript-hub.ts:43`) is how most of the transcript extension refers to its
own container. The discipline is real in the leaves and absent in the trunk.

## The inference router

**One global setting, re-read per turn, with no signal from the agent or the
message.** `SAND_INFERENCE_PROVIDERS = ["cursor", "claude-code", "codex",
"openrouter"]` (`source/shared/inference-router.ts:1`); the choice is a single
optional field `inferenceProvider` on `SandStoredSettings`
(`sand-settings-store.ts:29`), defaulting to `"cursor"` (line 158). The
selection is a plain branch:

```ts
// source/host/extensions/inference/inference-service.ts:56
createSession(onRequestId, sessionOptions) {
  const provider = routerSettings.getInferenceProvider();
  if (provider === "cursor") return routedSession(cursor.createSession(...), provider);
  return createProviderPromptSession(provider) as ...;
}
```

`getInferenceProvider()` reads and JSON-parses `settings.json` synchronously
each call (`sand-settings-store.ts:95`), and it is called freshly on every
`createSession`, every routed `sendPrompt`
(`node-agent-coordinator/inference-router.ts:188`), and in three other places.
So it behaves per-turn, but the signal is always the same user toggle. There is
no cost-, latency-, capability- or task-based routing anywhere.

Within a provider the model is an env/config lookup, not a decision:
`configuredCodexModel()` defaults to `"gpt-5.4"` reading `SAND_CODEX_MODEL` or
`~/.codex/config.toml` (`provider-session.ts:134`); Claude Code takes
`SAND_CLAUDE_MODEL` (line 213); OpenRouter takes `SAND_OPENROUTER_MODEL`,
default `"openai/gpt-5.2"` (line 248). Only the `"cursor"` branch reaches the
original Statsig-experiment machinery (`sand-model-experiment.ts`,
`sand-labeling.ts`), and the router only decides whether to enter it.

## Approvals, permissions, tools, sandboxing

The permission model is the most interesting non-obvious mechanism in the
repository, and most of it is pure.

A standing setting takes three values — `"never"`, `"ask"`, `"always"` — and a
per-request resolution takes four: `SandLocalToolResolution =
"allow-once" | "deny" | "always" | "never"`
(`local-tool-permission-controller.ts:20`). The **scope key** is
`askKey(scope, request) = ${agentId}\0${toolCallId}\0${action}\0${target}`
(line 34), so an approval is scoped to one tool call on one agent for one
action against one target, and `completeScope` (line 48) retires every approval
for that tool call unless it was marked `outlivesScope`. An approval may instead
be pinned to a resource path (`resourcePath`), which is how a grant survives
across tool calls that touch the same file or terminal folder.

Whether a stored approval covers a new request is a **pure predicate** —
`localToolApprovalCovers` (`source/shared/local-tool-permission-machinery.ts:83`),
four lines: same action and target, or the same normalised resource path. The
same predicate is reused by the out-of-process local-exec daemon
(`host/local-exec/local-exec-daemon.ts:59`), which is the right shape: one
decision function, two callers, no duplicated policy.

Around it sits a *direction epoch* per agent. A refusal is remembered against
the epoch in which it happened (`refusalFor`, line 64); when the user gives new
direction the epoch advances and old refusals stop applying. Widening the
standing permission to `"always"` records `alwaysGrantedAtEpoch` per agent so
that a tool call minted *before* the grant does not silently benefit from it
(`predatesStandingGrant`, line 65; `noteStandingPermission`, line 67). Memory
is explicitly bounded — 64 settled ids, 512 refused actions per agent, 256
forgotten agents, a 10,000-char target cap (lines 13-17). Resolution from the UI
goes through `resolveLocalToolPermissionAsk`
(`local-tool-permission-resolution.ts:9`), which validates the resolution
string, refuses a mismatched agent, and settles a stale card rather than
throwing.

A second, separate gate is the *spend guard*, and despite the name it measures
attention, not money. `evaluateAutomationSpendGuard`
(`transcript/sand-automation-spend-guard.ts:32`) is a **pure function** from
`{nowMs, lastViewedAtMs, unreadCount, firesSinceViewedCount, nudgedAtMs,
snoozedUntilMs, optedOut}` to one of seven verdicts (`opted-out`, `user-active`,
`snoozed`, `pause`, `awaiting-ack`, `nudge`, `below-thresholds`) against fixed
thresholds: idle for 3 days, 15 unread, 20 automation fires since last viewed, a
3-day pause delay, a 30-day snooze (lines 2-6). The impure half
(`automation-spend-guard-runtime.ts:82`) reads the unread state, posts a widget
card, and disables automations. This is a response-threshold rule with the
decision already lifted out of the IO.

Tool execution is MCP-shaped. `createSandMcpStateExecutor`
(`host/ports/mcp-state-executor.ts:7`) groups `SandMcpTool
{providerIdentifier, name, toolName, description?, inputSchema?}` by
`providerIdentifier` into `McpStateServer` protobufs — the routing table is
`providerIdentifier`. Claude Code gets a real local HTTP MCP bridge
(`node-agent-coordinator/routed-mcp-bridge.ts:39`); Codex and OpenRouter get a
direct tool-calling loop instead (`provider-session.ts:153`, `230`), proxying
back to the same dispatch. Two mechanisms, one tool inventory.

The isolation boundary is the "box" — normally a remote sandbox VM, optionally a
local Docker container
(`electron-main/box/local-docker-host-connector.ts`), bound to `127.0.0.1`
only, mounting content-addressed host/daemon bundles and the user's
`~/.codex`/`~/.claude` read-only, health-checked before the coordinator
connects. Host-side there is `assertPathOutsideProtectedRoots`
(`host/box/protected-path-guard.ts`), which checks a candidate path against
protected roots both as resolved and as `realpath`, so a symlink cannot escape.
Agent state is further isolated in worker processes
(`host/agent-isolation/`), each agent owning its own `conversation-blobs.db`.

## Usage and cost tracking

Measured per provider: `requests`, `inputTokens`, `outputTokens`,
`cacheReadTokens`, `cacheWriteTokens`, `lastUsedAt`
(`source/shared/inference-router.ts:4`). No cost figure is retained. The
accumulation *is* a fold — `recordInferenceUsage`
(`sand-settings-store.ts:161`) computes `previous + delta` over the record it
already holds, clamping each delta through
`Number.isFinite(v) && v >= 0 ? Math.round(v) : 0`. It is wrapped in a full
read-JSON/mutate/write-JSON round trip per turn, so the fold is right and the
storage is not.

Claude Code returns a real `total_cost_usd`, and it is captured into per-turn
provider metadata (`provider-session.ts:223`) and then dropped — never
accumulated, never displayed.

## What is already a fold over data the caller holds

Ordered by how directly it maps onto `tinyhivemind`:

1. `resolveResponders` / `parseGroupMentions` / `memberMentionHandles` /
   `hasMentionAt` (`host/groups/group-chat.ts:7-10`) — the whole mention
   grammar, pure over `(members, history)`.
2. `orderRoundSpeakers` (`group-chat.ts:3`), `isPassContent` /
   `isPotentialPassPrefix` (line 11), `messagesSinceMemberLastSpoke` (line 16),
   `formatGroupHistory` (line 14) — speaker rotation, response threshold, and
   per-viewer projection.
3. `GroupChatOrchestrator.run` (`group-chat-orchestrator.ts:35`) — a bounded
   episode loop that is pure but for six injected methods.
4. `evaluateAutomationSpendGuard` (`sand-automation-spend-guard.ts:32`) — a
   seven-verdict decision over a window.
5. `nextEntryId` and its counting helpers (`transcript-entry-ids.ts`) — the
   whole 78-line file.
6. `localToolApprovalCovers` (`shared/local-tool-permission-machinery.ts:83`) —
   a four-line permission predicate shared by two processes.
7. `truncatePromptFairly` / `computeMaxMinFairAllocations`
   (`packages/agent-summarization/prompt-truncation.ts:17,70`) — **max-min fair
   allocation** of a character budget across messages: sort by size, hand each
   the smaller of its size and `floor(remaining/remainingCount)`, drop anything
   allocated under `minUsefulChars = 200` and replace it with an
   `[omitted N chars]` marker. A directly reusable attention-market fold.
8. `resolveHostExtensionBootOrder` / `describeCycle`
   (`internal/host-extensions.ts:57,113`) — topological sort with named errors.
9. `parseCoordinatorFrame` (`shared/rpc/coordinator-port.ts:35`) — a
   non-throwing frame parser returning accept/reject.
10. `upsertAgentSummary` (`shared/agents/agent-summaries.ts:13`),
    `getLastEntryFromTranscript` / `collectLastAttachmentBatchKinds`
    (`session/session-projection.ts:10,14`), the paging folds in
    `agent-db-transcript-pages.ts`, `estimateTokenCount`
    (`agent-summarization/token-estimate.ts:67`), and
    `single-message-loop-detector.ts` (config injected as thunks).

## Where the README overclaims

Very little, and the drift is more about *layering* than about *existence*. All
four added features are real, IO-performing implementations, not stubs.

- **"an inference router"** — the word oversells the mechanism. It is a
  four-valued enum in a settings file with a `switch`; there is no routing
  policy, no fallback, no per-agent or per-task selection. Read as "a provider
  switch", the claim is exact.
- **"Grok Bot plugin/MCP tools across the routed providers"** — true, but via
  two unlike paths (an HTTP MCP bridge for Claude Code, a direct tool loop for
  Codex and OpenRouter). The phrasing implies one mechanism.
- **"local usage tracking"** — accurate, and the README is more careful than it
  needed to be: it says "request and token totals" and disclaims billing
  authority, which matches the code exactly, including the discarded
  `total_cost_usd`.
- **"an optional local Docker sandbox"** — accurate line for line.
- The genuine overclaim is architectural, and the repository makes it in its own
  vocabulary rather than in the README: `source/host/ports/` is not a port
  layer, and `RosterProjection` is not a projection. The README's architecture
  diagram also renders `coordinator + host` as a single box, which hides that
  the host is 35 extensions and a 2,646-line composition root.
- `frontend/` is described honestly as a "readable partial reconstruction" that
  should not be mistaken for the original source, and `PROVENANCE.md` repeats
  it. That restraint is warranted: packaged builds keep the shipped renderer and
  apply only a hash-recorded settings transform.

## Mechanism → what `tinyhivemind` already has → what it does not

| Mechanism (as found) | `tinyhivemind` already has | Not covered here |
| --- | --- | --- |
| Mention grammar `@name` with three derived handles per member (`group-chat.ts:7`) | `docs/specs/mentions.md` — roster records, grammar, normalization | Deriving handles from a display name rather than storing one; the ASCII-only word boundary |
| `@everyone` / `@all` regex → all members (`group-chat.ts:9`) | `@everyone` as a list, one message one turn | Their default: **no mention also means everyone** |
| `resolveResponders` over history since last user message | Responder ladder, `docs/specs/responders.md` | Unioning mentions across *every* message since the last user turn, not just the triggering one |
| Bounded episode: 3 rounds × 10 member turns × 2 messages, ends on a silent round (`group-chat-orchestrator.ts:45`) | `tinyhivemind-hive` episode state machine, sequential by ADR 0002 | Their caps are plain constants, not a policy record; the "silent round ends it" stop rule |
| `"(pass)"` convention + `isPotentialPassPrefix` streaming suppression | Response thresholds, cross-inhibition | An explicit *textual* pass token the model emits, and prefix-suppression while it streams |
| `orderRoundSpeakers` round-rotation | Deterministic one-responder selection | Rotating the opener by round number to avoid a fixed speaker order |
| `messagesSinceMemberLastSpoke` + `formatGroupHistory(limit=24)` | Projection fold, `docs/specs/sessions.md` | Slicing from the viewer's own last utterance as the whole prompt input |
| `seq` + stable `id` on one table (`agent-db-schema.ts:15`) | Sequence-number addressing across host surfaces | Nothing — this agrees |
| `nextEntryId` position-derived ids `t3a0` (`transcript-entry-ids.ts:5`) | Host-minted ids | Deriving an id from a fold over the transcript so it is reproducible |
| SQL visibility filters as the projection (`agent-db-schema.ts:1,27`) | Pure projection fold | Pushing the filter into the store — the opposite trade; worth knowing as a host-side option |
| `clientNonce` + canonical SHA-256 digest ledger, 256 records | Idempotent dispatch, `docs/specs/mention-dispatch.md` | The digest-mismatch error on a reused nonce; a bounded persisted ledger |
| Epoch supersession + `MAX_REPLY_NUDGES = 3` (`turn-runtime.ts:534`) | Bounded turns | Re-invoking a runner that owed a delivery and produced none |
| Three-lane per-agent queue with watchdog and zombie escape (`run-scheduler.ts`) | `MentionTurnQueue` port, atomic enqueue | The wedged-run escape hatch and lane preemption of `group-member` by `user` |
| Agent-to-agent send with **no** hop bound (`agent-to-agent-messaging.ts:60`) | Hop-bounded dispatch, `cross-desk-referral.md` | Nothing to take — this is the failure mode `tinyhivemind` exists to avoid, observed in the wild |
| `localToolApprovalCovers` pure predicate + `agentId\0toolCallId\0action\0target` scope | — | An approval algebra: scope key, `outlivesScope`, resource-path pinning, one predicate two processes |
| Direction epochs invalidating refusals and standing grants (`local-tool-permission-controller.ts:64-67`) | — | Epoch-scoped consent — a grant must not retroactively cover an older request |
| `evaluateAutomationSpendGuard` seven-verdict window fold | Decaying salience, quorum | A pure "should I keep spending attention here" verdict over unread/idle counters |
| `truncatePromptFairly` max-min fair char allocation (`prompt-truncation.ts:17`) | Attention market, stated per-message budget | The concrete max-min algorithm and the `minUsefulChars` drop-and-mark rule |
| Provider switch as a global enum re-read per turn (`inference-service.ts:56`) | Model selector boundary, `docs/specs/responders.md` | Nothing worth taking; confirms the selector belongs behind a port, not in the algebra |
| Usage fold `previous + clamped delta` per provider (`sand-settings-store.ts:161`) | — | A per-provider counter fold; note their storage (JSON round trip per turn) is the part not to copy |
| `resolveHostExtensionBootOrder` pure topo-sort with named cycle errors | Crate-level module wiring | A pattern for a host assembling ports in dependency order with diagnosable failures |
| `RemoteRoomMember {kind: "agent" \| "human", authId, agentId}` | Roster records distinguishing participants | Carrying an owning account id alongside the agent id for cross-user rooms |
| One agent = one conversation = one directory = one SQLite file | Host owns storage; desks as overlays | Their conflation of identity and conversation — `tinyhivemind`'s desk overlay is the thing they lack |
