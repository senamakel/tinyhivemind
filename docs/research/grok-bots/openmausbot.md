# OpenMausBot

**What it is.** A Telegram/Slack-style chat app (Electron desktop client +
local Node/TypeScript server) where every "contact" in the sidebar is a real
CLI agent (Claude Code, Codex, or a Grok/Gemini/Cursor-family CLI over the
Agent Client Protocol) with its own persona, model, and optional cloud/local
VM "computer". It bills itself as an open-source rebuild of xAI's "Grok Bot"
product idea. This is the closest analogue in the survey set to tinyhivemind's
roster + mention + one-shared-transcript model, because it is a full working
implementation of exactly that shape, in TypeScript, with no host-type
boundary at all — everything (roster, mentions, transcript, harness) lives in
one process against one JSON-file store.

- URL: https://github.com/milind-soni/OpenMausBot
- HEAD: `b3c2f74b6375eed347bbccd1803816b5ac6e857d` (shallow clone, 2026-09-06)
- Licence: Apache License 2.0 (`LICENSE`)
- Size: ~104M checked out, 623 TS/JS/etc. source files, Electron + Android +
  iOS + Cloudflare Workers + enterprise-server surfaces around one Node
  "harness server". The chat-room core is a small fraction of the repo
  (`server/index.ts` alone is 12,837 lines; the roster/mention/transcript
  logic surveyed here is a few hundred lines spread across
  `server/store.ts`, `server/peer-roster.ts`, `server/team-manifest.ts`,
  `src/lib/group-routing.ts`).

## Architecture

One local Node server (`server/index.ts` + `server/store.ts`) is the sole
owner of state: bots, rooms/groups, and per-thread message arrays, all
persisted as plain JSON files under `~/.openmausbot` (`server/store.ts:603-604`,
`BOTS_FILE`/`GROUPS_FILE`, plus one `messages-${threadId}.json` per thread).
Electron/Android/iOS clients talk to it over HTTP/WS; it in turn spawns one
OS subprocess per bot turn (or reuses a resumable session) through a
`ProviderAdapter`/`ProviderDriver` interface (`server/contracts.ts`) that each
CLI-specific driver under `server/drivers/` implements. There is no separate
append-only event log distinct from the per-thread `Message[]` array — the
JSON file *is* the transcript, in tinyhivemind's terms this is "the host owns
storage" taken to its simplest possible form (files, not even a database).

## 1. The roster

**Data structure** — `BotRecord` (`server/store.ts:456-582`) is the bot/agent
identity. Fields (partial, the load-bearing ones): `id`, `threadId`, `name`
(also the display/@-mention handle — there is no separate handle field),
`title`, `description`, `soul?` (persona/standing-instructions text, ≤24000
bytes, mirrored to a `SOUL.md` file — `soul-profile.ts:465-473`), `soulHash?`,
`color: MausColor`, `mascotExpression?/mascotBody?`, `avatarUrl?`, `unread`,
`modelSelection`, `resumeCursors` (per-provider CLI session-continuation
token), `computer?` (`"cloud"|"vm"|"local"|"browser"|"off"`), `cwd?`,
`autoApprove?`, `approvalMode?` (`"ask"|"auto"|"full"|"custom"`),
`alwaysAllow?: string[]`, `section?` (sidebar grouping), `chiefOfStaff?`,
`peers?: string[]` (this bot's allow-list of who it may talk to), `busy?`,
`activity?`, `createdAt`.

**Storage** — plain JSON on disk, not a DB or config file the user
hand-edits: `server/store.ts:603-604` (`BOTS_FILE`, `GROUPS_FILE`), loaded via
`JSON.parse(readFileSync(...))` and written via `writeFileAtomic`
(`server/store.ts:~889`).

**"Who is here"** answered at two grains:
- Global/section reachability: `reachablePeers<T>(bots, from)`
  (`server/peer-roster.ts:41-50`) — same `section`, not `hidden`, not self,
  and passing a per-sender allowlist check
  (`!Array.isArray(from.peers) || from.peers.includes(targetId)`,
  `peer-roster.ts:34-35`).
- Room membership: `GroupRecord.memberIds: string[]` (`server/store.ts:220`);
  `src/lib/room-members.ts:4-5`, `nextMemberIds(current, picked, order)`
  recomputes the ordered id list from picker UI state, preserving existing
  order and appending new picks.

**Aliases/identity** — canonical `id` (opaque, generated) is separate from
`name`, but `name` *is* the @-mention handle and the display name at once —
one field serves both roles, unlike tinyhivemind's roster record which
separates a stable id from a mention alias. Name collisions from imported
teams are actively repaired: `importedMemberProfile`
(`server/team-manifest.ts:252-272`) suffixes a colliding imported bot's name
("Scout" → "Scout 2"), because names are the identity used everywhere bots
address each other — @mention resolution, roster listing, peer-approval
prompts (`team-manifest.ts:241-247`). A team-manifest import additionally uses
a throwaway `key: /^[a-z0-9][a-z0-9_-]*$/` slug (`team-manifest.ts:44-46`)
purely to cross-reference members *within one manifest file* — it does not
survive import as a runtime identifier.

**Desk/room grouping** — `GroupRecord` (`server/store.ts:213-244`) is the
closest analogue to a "desk": `id`, `threadId`, `name`, `memberIds`,
`defaultResponder: {kind:"member",botId}|{kind:"everyone"}|{kind:"mentions"}`
(`store.ts:~654-671`), `bulletin` (shared instructions injected into every
member's system prompt), `dm?` (auto-created bot↔bot 1:1 channel), `section?`.
There is no entity above rooms; a bare `section: string`, shared by
`BotRecord` and `GroupRecord` and case/whitespace-normalized via
`sectionKey()` (`peer-roster.ts:22`), is the only coarser scoping level, and it
is used purely for reachability filtering, not for a typed "desk" concept
with its own membership algebra.

## 2. Mention/addressing grammar

**Literal syntax** (`server/store.ts:628-644`, mirrored client-side at
`src/lib/group-routing.ts:84-103`):
- `@everyone` — the only reserved token, matched by
  `/(?:^|\s)@everyone\b/i` (`store.ts:679`, `group-routing.ts:47`,
  `src/lib/group-call.ts:18`).
- `@<bot display name>` — any bot's exact `name`, arbitrary text.
- No `@here`/`@all`/`@channel`. Voice dictation separately normalizes spoken
  "everyone"/"everybody"/"all" into `@everyone` before it reaches the room
  (`group-call.ts:25`) — a speech-to-text convenience, not part of the typed
  grammar.

**Resolution rule** — `mentionedBots` is pure string matching, not an ID
syntax and not fuzzy: `@` must follow start-of-string or whitespace (guards
`user@host`); candidate bot names are sorted **longest-first** so `"@New Bot
2"` isn't shadowed by a shorter `"New Bot"`; match is case-insensitive
`startsWith`; the match must end on a non-alphanumeric boundary; hidden bots
are excluded from candidates; results are deduplicated. No alias table exists
— the mention grammar and the `BotRecord.name` field are the same namespace,
which is exactly why the name-collision repair on import
(`team-manifest.ts:252-272`) exists.

**Self-mention** — no explicit self-exclusion inside `mentionedBots` itself;
its doc comment states "callers pre-filter the sender out of `peers`"
(`store.ts:625`), i.e. self-exclusion is a caller responsibility, not part of
the resolver. (Not independently verified against the specific caller-side
filter line in `server/index.ts` — flagged rather than guessed.)

**Ambiguity** — cannot occur by construction: longest-name-first sorting plus
first-match-per-occurrence removes ties. A mention matching no bot is
silently dropped (not added to the result set, no user-facing error).
Mentioning an *archived* (hidden) bot is handled separately and visibly: it's
excluded from resolution candidates and the room gets an activity line,
`"${name} is archived and can't respond — restore it or mention an active
room member."` (`server/index.ts:6105-6112`).

## 3. Turn triggering and fan-out

`roomResponders` (`server/store.ts:676-687`) is the pure decision function:
explicit `@everyone`/`@name` mentions win outright; otherwise the room's
`defaultResponder` policy applies (`everyone` / `mentions` / a fixed lead
`member`). It's invoked from the room message entry point
(`server/index.ts:6118-6119`), and the resulting `responders` list plus an
`operation` handle (`beginGroupTurnOperation`, `index.ts:1643`) drives
execution via `runGroupMemberTurn` (`index.ts:5114`).

**One message *can* target N bots, but fan-out is sequential, not
concurrent**: the responder loop is a plain `for...of` with `await` on each
iteration (`server/index.ts:6210-6233`) — one bot's turn (or its skip/timeout)
completes before the next starts; there is no `Promise.all`. The whole
per-room turn is additionally serialized against other incoming messages to
the same room by a promise chain keyed on `groupId`
(`groupQueues.get(groupId) ?? Promise.resolve()` → `.then(...)` →
`groupQueues.set(...)`, `index.ts:5024`, `6183-6238`) — this is the actual
concurrency bound: one active room operation at a time. Each responder's wait
is itself capped by `waitForChatRoomMember` (`index.ts:1845`, returns
`"run"|"skip"|"stop"`, busy bots time out via `GROUP_GOAL_WAIT_MAX_MS`); a
goal-coordinator mode adds a hard `GROUP_GOAL_MAX_TURNS = 13`
(`server/group-goal-run.ts:5`). Net effect: `@everyone` in a 10-bot room does
start 10 turns from one message — the "N turns from one message" hazard
tinyhivemind's `mentions.md` spec explicitly designs against — but bounds it
to strictly sequential execution with a per-room single-flight lock and a
per-responder timeout, rather than bounding the *count* of turns started.

## 4. The shared transcript and projection

**One log per thread, not one fleet-wide log.** `ThreadState.messages:
Message[]` (`server/store.ts:1149`, `messagesFor(threadId)` at `:1168`) is an
independent array per thread — a 1:1 bot chat, a DM, or a room are each their
own thread/file. A room *is* one thread all its members share; there is no
higher-level log spanning rooms.

**Attribution is textual, not a role split.** `Message.role` is only
`"bot" | "user"` (`store.ts:108`) — there is no per-bot "assistant" role.
Sender identity in a room lives in `Message.from: {botId, name, color}`
(`store.ts:~163`). When a room turn's context is built,
`serializeRoomContext` (`server/index.ts:5041-5070`) flattens the whole
window into one text blob, one line per message:
`` `${speaker}: ${transcriptText(...)}` `` where `speaker` is either the human's
name or `peerName(m.from.name)`. So bot B's prior turn is presented to bot A
as plain text inside what the underlying CLI protocol sees as user-role
content — not as a first-class `assistant` turn reused across bots. This is
the opposite of tinyhivemind's projection design (an attributed
`SessionMessage` with a real role mapping, per `docs/specs/sessions.md`); here
attribution is baked into prose the target LLM has to parse back out.

**Provenance vs. redaction — two distinct, non-overlapping mechanisms**:
- Custody provenance: when a message crossed a boundary (another bot posted
  into this room via `post_to_room`, or delivered via `ask_bot`), a prefix
  note (`peerProvenanceNote(...)`) is prepended so the reading bot knows the
  line isn't organic room conversation (`index.ts:5069-5070`).
- Secret redaction (`server/redact.ts`, ~110 lines): strips credential-shaped
  text (API key prefixes, bearer tokens, PEM blocks) before it reaches the
  on-disk protocol log and decision log (`redact.ts:56-64`,
  `server/decision-log.ts:13-15`). This protects the audit trail, not
  inter-bot visibility — it does not filter what `serializeRoomContext` shows
  one bot of another's chat content.

**Scoping** is per-`threadId` only; cross-thread visibility is never a direct
read, only explicit mirrored calls (`getOrCreateChannel`, `mirrorExchange`,
`server/comms-visibility.ts:19-40,62+`).

**No read watermark.** Every room turn re-serializes the last
`GROUP_CONTEXT_MESSAGES = 30` messages fresh each time
(`index.ts:5026,5044`) — there is no per-bot "since last seen" cursor over the
transcript. The only cursor-shaped thing, `resumeCursors`
(`store.ts:1564-1579`), is a CLI-session-continuation token per (bot,
instance, task) for the underlying provider (e.g. a Claude Code `--resume`
session id) — unrelated to transcript pagination.

## 5. CLI-agent harness adapter

**Common interface** — `ProviderAdapter` (`server/contracts.ts:225-297`):
`sendTurn(input: SendTurnInput): Promise<TurnStartResult>`,
`interruptTurn(threadId, turnId?)`,
`respondToRequest(threadId, requestId, decision)`, optional `steer?`,
`hasSession(threadId)`, `stopAll()`, `onEvent(listener): unsubscribe`. Each
driver additionally implements `ProviderDriver<Config>`
(`contracts.ts:395-408`): `driverKind`, `metadata`, `decodeConfig`,
`defaultConfig`, `models`, `create(input): Promise<ProviderInstance>`.
`ProviderInstance` (`contracts.ts:373-391`) wraps the adapter plus
`snapshot()`, optional `generateText`/`reviewPermission`/`installRuntime`/auth
hooks, `dispose()`. `ProviderRegistry.load` (`server/harness/registry.ts:53-107`)
is the sole wiring point; an unknown driver slug or a decode/create failure
degrades to a `ShadowInstance` (`registry.ts:16-24`) instead of crashing boot.

**What's agent-specific**:
- `server/drivers/claude.ts` — per-turn CLI subprocess, bidirectional
  `stream-json` over stdin/stdout, session continuity via `--resume
  <sessionId>` (spawned at `claude.ts:980`, `:1313`).
- `server/drivers/codex.ts` — the official `codex` CLI's **app-server
  JSON-RPC** protocol (newline-delimited JSON over stdio, not stream-json);
  completion is a `turn/completed` notification; approvals arrive as
  in-process server→client JSON-RPC requests (spawned at `codex.ts:579`);
  resume via `thread/resume`.
- `server/drivers/grok.ts` (60 lines) is **not** a CLI driver — it's the xAI
  chat-completions HTTP API (`grok-4`/`grok-4-fast` via
  `createOpenAIChatRuntime`). The actual Grok **CLI** agent is
  `server/drivers/acp/grok.ts` (`driverKind: "grokAgent"`), which spawns the
  real `grok ... agent stdio` binary over the **Agent Client Protocol**, using
  `~/.grok/auth.json` subscription login rather than an API key, and reads
  `~/.grok/config.toml` for its model catalog (`acp/grok.ts:1-4,35-95`).
- ACP-family drivers (grok, cursor, gemini, droid, kimi, qwen, opencode-go,
  custom) all share one generic runtime, `server/drivers/acp/core.ts`; each
  file supplies only per-harness quirks via an `AcpSupport` object
  (`spawnArgs`, `resolveCommand`, `effortLevels`, `defaultCli`, install
  metadata — `acp/core.ts:22,82,101,151,158`). Claude and Codex are *not* on
  this shared ACP path; each has its own bespoke driver and wire protocol.

**Invocation mechanism** — subprocess spawn (`spawnCli` from
`server/procs.ts`) for every real CLI agent (`claude.ts:980/1313`,
`codex.ts:579`, `acp/core.ts:465`); only the pure-API drivers (the xAI
`grok.ts`, `openai-chat.ts`-backed ones) use `fetch`/HTTP.

## 6. Approval gates and permissions

Two independent layers:
- **Peer-comm approval** (bot→bot, `server/peer-approval.ts`):
  `requestPeerApproval` (`:148`) checks `bot.alwaysAllow` first (`:152`),
  else pushes an options card into the *source* bot's own thread
  (`pushApprovalCard`, `:96`), marks the bot `"waiting-on-you"`
  (`announceCard`, `:129`), and returns a promise resolved by
  `resolvePeerComms` (`:190`, driven by the API's respond endpoint) or timed
  out to `"deny"` after 15 minutes (`APPROVAL_TIMEOUT_MS`, `:161-168`).
  "Always allow" persists as `bot.alwaysAllow`, keyed by
  `peerAllowKey(action, targetId)`.
- **Tool/permission approval** (bot→tool): driven by `ApprovalMode`
  (`shared/approval-mode.ts`) — `"ask"|"auto"|"full"|"custom"` — reasserted on
  *every* turn so a resumed native session can't retain stale permissiveness
  (contracts.ts, `SendTurnInput` comment near line 163).
  `supportsApprovalMode`/`requiresNativeApproval`/`hasNativeAutoReview`
  (`approval-mode.ts:6-21`) gate which drivers may expose which mode.
  Provider-native permission requests surface as canonical
  `request.opened`/`request.resolved` `RuntimeEvent`s (`contracts.ts:130-146`)
  and are answered via `respondToRequest` (`contracts.ts:265-270`);
  `"unavailable"` is the explicit fail-closed default when nobody can decide.

**VM/sandbox** — `server/container-computer.ts:1-7,29-38` is a Podman
container (marketed as "Local VM"; not a hardware VM), pinned to
`docker.io/trycua/xfce-cua` by digest. Desktop automation inside it is
delegated entirely to `cua-driver mcp` running in the container — the harness
does not reimplement clicks/typing. This is surfaced to a driver as
`turn.integrations.computer` (`contracts.ts:191-199`,
`{kind:"box", boxId, token, control}`), gated per-driver by a `computerMcp`
capability flag (`contracts.ts:232-236`) so "a bot must never be told it has
a computer whose tools its driver cannot mount." `server/drivers/boxagent.ts`
is the cloud-VM ("box") counterpart (`CloudBackend = "box"|"vps"`,
`contracts.ts:14`).

## 7. Pure decision logic vs. IO

Foldable/pure (given arguments already at hand, no I/O):
- `mentionedBots` (`store.ts:628-644`) — string parse over `(text, names)`.
- `roomResponders` (`store.ts:676-687`) — decision over `(text, defaultResponder,
  memberIds)`.
- `reachablePeers` (`peer-roster.ts:41-50`) and its `peerAllowed` check.
- `nextMemberIds` (`room-members.ts:4-5`).
- `serializeRoomContext`'s line-formatting logic, given a message window — pure
  string building, though the window itself (last 30 messages) is fetched by
  IO immediately before.

Inherently IO (own the waiting, by design):
- Everything in `server/store.ts`'s read/write path (`writeFileAtomic`, JSON
  file loads) — there is no host-storage boundary; the store *is* the file
  system.
- The whole harness layer (`server/harness/*`, `server/drivers/*`) — process
  spawn, stdio streaming, JSON-RPC/ACP wire protocols.
- `requestPeerApproval`'s promise-and-timeout (`peer-approval.ts:148-190`) —
  waits on a human/bot decision or a 15-minute clock.
- The `groupQueues` promise-chain single-flight lock (`index.ts:5024`) — this
  is exactly the kind of "atomic host enqueue" tinyhivemind's
  `mention-dispatch.md` spec wants behind a port, but here it's an in-process
  `Map<string, Promise>`, not a durable, host-owned queue — a process
  restart mid-turn loses the lock silently.

## README overclaims / imprecisions

- README says bots "run on the `claude`, `codex`, and `grok` CLIs installed
  on your own machine." The `grok` *provider slug* (`server/drivers/grok.ts`)
  is in fact an HTTP API driver (xAI chat-completions), not a CLI at all; the
  actual Grok CLI-over-ACP driver lives at a different path
  (`server/drivers/acp/grok.ts`, `driverKind: "grokAgent"`) and is one of
  several ACP-family CLIs (cursor, gemini, droid, kimi, qwen, …) the README
  doesn't mention by name.
- "Local first... one small harness server" undersells scope: the repo also
  ships a Cloudflare Workers control plane and composio broker
  (`cloudflare/control-plane`, `cloudflare/composio-broker`) and an
  `enterprise/server` — optional, but the "local-only" framing in the README
  doesn't surface that cloud-hosted "box" computers and enterprise pieces
  exist in the same repo.
- "Agents with hands... isolated Local VM" is a Podman/OCI container
  (`docker.io/trycua/xfce-cua`), not virtualization — the README's "VM"
  language (also matched by this task's own brief, "a VM the bots can use")
  is the product's own marketing term, not the underlying isolation
  technology.

## Mechanism → tinyhivemind has → does not have

| OpenMausBot mechanism | tinyhivemind has | tinyhivemind does not have |
| --- | --- | --- |
| `BotRecord` roster record (id, name-as-handle, section, peers allowlist) — `store.ts:456-582` | Roster records with a stable id (`docs/specs/mentions.md`) | A single `name` field doubling as both display name and mention handle (tinyhivemind separates these); an explicit per-bot `peers` allowlist gating reachability |
| `mentionedBots` longest-match, silent-drop-on-no-match string parser — `store.ts:628-644` | A defined mention grammar with normalization (`docs/specs/mentions.md`) | Silent-drop semantics on unresolved mentions — worth comparing against tinyhivemind's own ambiguity rule |
| `@everyone` as a list expanded to sequential per-bot turns, single-flight per room — `index.ts:6210-6233`, `5024` | The "`@everyone` is a list, not a broadcast" rule and one-message-one-turn invariant (`CLAUDE.md` charter, `mention-dispatch.md`) | An in-process, non-durable promise-chain lock is OpenMausBot's whole bound; it is not a host-owned, restart-safe enqueue — this is a concrete example of the failure mode `mention-dispatch.md`'s atomic host enqueue contract is designed to avoid |
| `serializeRoomContext` flattening every sender into one text blob with a name prefix — `index.ts:5041-5070` | An attributed `SessionMessage`/role-mapped projection (`docs/specs/sessions.md`) | Baking attribution into prose the model must reparse; tinyhivemind's structured projection is a strictly stronger model |
| `ProviderAdapter`/`ProviderDriver` per-CLI contract — `contracts.ts:225-297,395-408` | A port-based boundary philosophy (host implements ports; core stays pure) | tinyhivemind doesn't (and by charter shouldn't) define a CLI-harness-invocation port — that's squarely host territory, but OpenMausBot's adapter shape is a useful reference for what such a host-side port needs to expose (session resume, event stream, approval round-trip) |
| Two-layer approval (`peer-approval.ts` promise+timeout; `ApprovalMode` reasserted per turn) | The "one message, one turn" approval-adjacent framing conceptually, no implemented approval port yet | A concrete approval-gate design: fail-closed default (`"unavailable"`), per-action "always allow" memory, and a hard timeout-to-deny — all IO-side patterns a future tinyhivemind host port could mirror |
| No read watermark; every turn re-serializes last N messages (`GROUP_CONTEXT_MESSAGES = 30`) | Caller-owned watermarks for continuous sharing (`docs/specs/continuous-sharing.md`) | OpenMausBot has nothing analogous — it re-derives context by truncation every turn, which is exactly the design continuous-sharing.md improves on |
