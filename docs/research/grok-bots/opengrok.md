# opengrok

- **What it is:** a per-agent model-routing "config sidecar" for the Grok Bot
  desktop app. It does not host or emulate the app; it writes model bindings,
  wire-shape maps, and a health doctor next to an existing install, and
  injects a "hop" (local OpenAI-compatible proxy) between the app and whatever
  upstream provider (xAI, Zhipu/GLM, Anthropic, Google, DeepSeek, local
  llama.cpp) an agent is bound to.
- **URL:** https://github.com/OnlyTerp/opengrok
- **SHA:** `e69b00f70c8ab838699782ced9db58e12dd7f62d`
- **Licence:** MIT (`LICENSE`)
- **Size:** 78 tracked files (excluding `.git`), ~4,035 lines across
  `.rs/.ts/.js/.py/.go`-equivalent source (actual mix is `.py`/`.cjs`/`.js`);
  the two largest files are `box/openai-hop-session.cjs` (1,996 lines) and
  `box/hud/liquidglass.js` (1,451 lines).

## Architecture overview

```
Grok Bot agent (host app)
      │  modelId + parameters (thinking/effort/fast) from model-bindings.json
      ▼
tools/provider-maps.cjs / tools/provider-maps-hop.cjs   <- pure wire-shape maps
      │
      ▼
box/openai-hop-session.cjs  <- the streaming adapter (owns all IO)
      │  HTTP(S) to a local "hop" shim (localhost:187xx) which owns the real
      │  provider credential
      ▼
upstream provider API (xAI / Zhipu / Anthropic / Google / DeepSeek / local)
```

Key files:
- `tools/provider-maps.cjs` (452 lines) — Contract A, per-provider body
  rewriting, pure functions.
- `tools/provider-maps-hop.cjs` (312 lines) — Contract B, `applyHarnessControls()`,
  the variant that ships inside the box/hop process.
- `box/openai-hop-session.cjs` (1,996 lines) — the adapter that implements the
  host's `PromptExecutor`/`PromptSession` duck-typed interface, does the actual
  streaming HTTP, SSE parsing, retries, telemetry, and provenance logging.
- `tools/model-picker.py` (257 lines) — local HTTP server + single-page UI to
  edit `model-bindings.json`.
- `tools/doctor.py` (345 lines), `tools/qa.py` (122 lines) — health/leak audit.
- `tools/apply-box-patch.py` (225 lines) — anchored, idempotent host-bundle
  patcher.
- `docs/BYOK-DECISION.md`, `docs/MODEL-GUIDELINES.md`, `.contracts/validation-contract.md`
  — the design rationale and "laws".

## 1. What the routing decision keys on, and where relative to IO

The decision is keyed on **model id (regex match on the slug) and/or the hop
base URL (localhost port)** — never on task type, agent role, or any semantic
content. Each provider has an `is<Provider>Route(modelId, baseUrl)` pure
predicate:

- `tools/provider-maps.cjs:30-34` `isGrokRoute` — `GROK_MODEL_RE = /^grok[-.]/i`
  or base URL matches `127.0.0.1:18779`.
- `tools/provider-maps.cjs:207-211` `isClaudeRoute` — `/^claude[-.]/i` or
  `127.0.0.1:18776`.
- `tools/provider-maps.cjs:214-221` `isGeminiRoute` — `/^gemini/i` or
  `127.0.0.1:18778`.
- `tools/provider-maps.cjs:234-238` `isDeepSeekRoute` — `/deepseek/i` or
  `nano-gpt.com` / `127.0.0.1:8791`.
- `tools/provider-maps.cjs:199-203` `isGlmRoute` — `GLM_MODEL_RE` or
  `bigmodel.cn`/`friendli`/`127.0.0.1:18791`.
- `tools/provider-maps.cjs:247-252` `isQwenRoute` — `qwen` in id and
  `127.0.0.1:18787`.

The dispatcher `applyProviderReasoningControls(body, ctx)` at
`tools/provider-maps.cjs:118-146` is a **pure function**: it takes `body`
(mutated in place) and `ctx = {modelId, baseUrl, maxMode, parameters,
requestKind, localQwen}`, returns a route label string, and performs no IO,
no `fetch`, no file access — it is directly unit-testable with plain object
literals (see `tools/test-provider-maps.cjs`, 23/23 passing per README). This
is the cleanest pure-fold candidate in the whole repo (see §6).

IO only enters one layer up, in `box/openai-hop-session.cjs`, where
`applyProviderReasoningControls` is `require`d (`box/openai-hop-session.cjs:16-28`,
wrapped in three try/catch fallbacks for different install layouts) and called
just before the actual `http`/`https` request is built inside
`HopPromptExecutor.stream()` (`box/openai-hop-session.cjs:998` onward). So the
architecture already matches tinyhivemind's "pure decision, impure execution"
split: the routing/wire-shaping logic is pure and IO-free; only the class that
calls it (`HopPromptExecutor`) touches the network, the filesystem (provenance
log), and `crypto.randomUUID()`.

Config that ultimately supplies `modelId`/`baseUrl`/`parameters` comes from
`model-bindings.json`, read by the host at turn time (`docs/MODEL-GUIDELINES.md:9-16`)
— so "agent identity" only enters indirectly, as a foreign key into that JSON
mapping agent-UUID → binding; the routing predicates themselves never see the
agent id.

## 2. The provider port shape

There is no formal TypeScript/interface declaration (this is untyped CJS/JS),
but two duck-typed contracts are documented and enforced by usage:

**A. The "map" contract** (what a provider wire-shape module implements),
inferred from every `apply<Provider>` function, e.g.
`tools/provider-maps.cjs:57` `applyGrok(body, maxMode, parameters)`:

```
apply<Provider>(body: object, maxMode: boolean, parameters: Array<{id,value}>) -> void | string
  - mutates `body` in place (the outgoing JSON request body)
  - reads `parameters` via `param(parameters, id)` (tools/provider-maps.cjs:36-43)
  - returns a route label (string) or nothing, for the caller to audit
is<Provider>Route(modelId: string, baseUrl: string) -> boolean
```

`applyProviderReasoningControls(body, ctx)` (`tools/provider-maps.cjs:118`) is
the single entry point that dispatches to the right `apply<Provider>` based on
the route predicates, in a fixed if/else ladder — a deterministic, ordered
route table almost exactly analogous to tinyhivemind's responder ladder
(first match wins, `"none"` is the fallback / no-op case).

**B. The `PromptExecutor`/`PromptSession` port** the host expects
(`box/openai-hop-session.cjs:2-3`, `class HopPromptExecutor` at line 967):

```
class HopPromptExecutor {
  constructor(builder, opts: {
    baseUrl, modelId, onRequestId, agentId, provenanceAgentId,
    requestKind, allowTestVisibleRecovery, maxMode, parameters
  })
  appendMessages(messages) -> this
  getState() -> builder state
  getMessages() -> message[]
  clearMessages() -> void
  stream(ctx, invocationId, tools, options2) -> {
    fullStream: AsyncGenerator<Event>,   // text-delta | reasoning |
                                          // tool-call-streaming-start |
                                          // tool-call-delta | tool-call |
                                          // finish | error
    usagePromise, extendedUsagePromise, providerMetadataPromise,
    invocationIdPromise, responsePromise
  }
}
```

The event vocabulary the generator must emit is fixed and documented at
`box/openai-hop-session.cjs:7` ("Working/streaming contract ... must match
catalog models or the UI goes idle"). `createOpenAiHopSession(opts)`
(`box/openai-hop-session.cjs:1949`) is the factory the host actually calls.

## 3. The approval-gate model

There is no interactive human-approval prompt anywhere in the source
(no `input()` / confirmation dialog gating a request). What exists instead are
two **fail-closed pure predicates** that gate destructive or trust-sensitive
actions:

- **Anchor-count gate** in `tools/apply-box-patch.py:50-53`:
  `check_anchor(text, anchor, label)` counts literal occurrences of an anchor
  string in the target file and calls `die(...)` — refusing to write — unless
  the count is exactly 1 (or, for one multi-spread case, 1 or 2, line 104).
  This is a pure function of two strings (`text.count(anchor)`); it is called
  before every patch step in `patch_host()` (lines 67, 80, 104, 113, 127, 134,
  141, 146, 153) and gates whether `write()` (line 46) ever runs. `--dry-run`
  (line 163, checked at 183-184) short-circuits before any write regardless.
  Approval here is **structural, not human**: "does the upstream bundle still
  look like what I expect" rather than "did a person say yes."
- **Provenance/trace gate** in `box/openai-hop-session.cjs:125-142`
  `requireTraceContext(agentId, baseUrl)`: throws unless `agentId` matches a
  UUID regex (line 45) and `baseUrl` parses with no embedded credentials,
  query, or fragment (lines 135-137). This gates whether a
  `HopPromptExecutor` can even be constructed (called at
  `box/openai-hop-session.cjs:969`) — it is a pure validation of two strings,
  no IO, and it is a hard precondition, not a discretionary approval.
- **Recorded, not gated, decision:** every hop request appends an audit line
  to a local JSONL provenance log (`PROVENANCE_AUDIT_PATH`,
  `box/openai-hop-session.cjs:43-46`, written via `appendProvenanceRecord` at
  lines 168-186, schema `GROKBOT_LOCAL_PROVENANCE_V1`) with 0600 permissions
  (`ensureProvenanceAuditReady`, lines 157-166). This is **audit trail, not
  gate** — it happens after the fact and does not block the request.
- **Organizational-level gate:** `.contracts/validation-contract.md:60-61`
  states "irreversible actions require operator approval" and "nothing pushed
  without operator GO" — but this is a human process rule for the
  maintainers' own workflow (documented in `.contracts/validation-contract.md`),
  not a mechanism enforced in code.

So: opengrok's only *code-level* gates are pure fail-closed predicates
(anchor count, UUID/URL shape) guarding writes/execution; there is no
credential-access approval flow and no destructive-action confirmation UI.

## 4. Scoping and permission boundaries

- **Credentials never enter this repo's own files.** `tools/model-picker.py:8`
  states outright: "keys NEVER enter bindings; Test button probes from THIS
  machine only." `model-bindings.json` only ever stores `modelId`,
  `hopBaseUrl`, `provider` (audit-only label), `maxMode`, and `parameters`
  (`docs/MODEL-GUIDELINES.md:29-37`). The actual API key/OAuth session lives
  in a separate, out-of-repo "hop shim" process bound to `127.0.0.1:<port>`
  (`docs/MODEL-GUIDELINES.md:11-20`); opengrok's own code never reads or
  writes a provider secret.
- **URL scoping as a security boundary:** `hopBaseUrl` is required to be
  exactly `http://127.0.0.1:<port>/v1` with "no creds, no query strings"
  (`docs/MODEL-GUIDELINES.md:33`), and this is enforced in code by
  `requireTraceContext` rejecting any URL with `username`/`password`/`search`/
  `hash` (`box/openai-hop-session.cjs:135-137`) — i.e., a credential literally
  cannot be smuggled into the routing config, it can only live in the
  loopback-only shim.
- **File-level scoping:** the provenance audit log is created with `0o600`
  permissions and `chmodSync` re-asserted (`box/openai-hop-session.cjs:159-165`).
- **Leak scanning as a repo-level audit:** `tools/qa.py:44-49` regex-scans the
  whole tree for private IPv4 ranges and key-shaped strings
  (`sk-`, `xai-`, `Bearer `, etc.) and fails CI if any are found — this is a
  static, pre-merge audit rather than a runtime permission boundary.
- **No filesystem sandboxing/scoping of the app itself** — opengrok patches
  the Grok Bot bundle in place (`tools/apply-box-patch.py`) and writes to the
  bundle/host config directories directly; there is no capability or sandbox
  model beyond the anchor-count refusal described in §3.

## 5. Preserving the native conversation protocol across providers

The "native protocol" is the **Vercel-AI-SDK-shaped streaming event vocabulary**
the Grok Bot host expects from any `PromptExecutor.stream()`
(`box/openai-hop-session.cjs:7`: `text-delta, reasoning,
tool-call-streaming-start, tool-call-delta, tool-call, finish, error`).
opengrok's adapter is the single place that translates between that vocabulary
and OpenAI-compatible SSE chat-completions on the wire:

- `HopPromptExecutor.stream()` (`box/openai-hop-session.cjs:998`) builds the
  outgoing OpenAI-style JSON body (via `HopPromptBuilder`, class at line 782,
  and `toOpenAiMessages`/`convertOneMessage`, referenced in the `__test`
  export at lines 1985-1991), applies the provider wire map, POSTs to the
  local hop, and re-parses the provider's SSE stream (`iterateSse`, exported
  at line 1990) back into the host's own event shape.
- Provider differences in *reasoning/thinking representation* are normalized
  at the body-shaping layer (`tools/provider-maps.cjs`) rather than at the
  protocol layer — e.g., Claude's reasoning is left untouched because "the
  shim ALREADY pins thinking" (`tools/provider-maps.cjs:99-101`, comment above
  `applyProviderReasoningControls`), while GLM/DeepSeek/Qwen get explicit
  `thinking`/`reasoning_effort`/`chat_template_kwargs` fields synthesized.
  Regardless of provider, the adapter always re-emits a `reasoning` event type
  to the host (`box/openai-hop-session.cjs:11`), i.e., the wire-level
  heterogeneity is absorbed before it reaches the host's native format.
- Robustness features specific to preserving protocol continuity under
  degraded upstream behavior: `repairTruncatedJson` /
  `canonicalToolArguments` (`tools/provider-maps.cjs:255-345`) repair
  malformed/truncated tool-call argument JSON so a broken upstream stream
  still produces a valid `tool-call` event; `HEARTBEAT_MS`/`SSE_IDLE_MS`
  constants (`box/openai-hop-session.cjs:33-35`) keep the host's "working"
  indicator alive during slow upstream responses so the illusion of one
  continuous native conversation is not broken by network stalls.
- Context-window heterogeneity is also normalized centrally:
  `reportedContextWindow(modelId, baseUrl, parameters)`
  (`box/openai-hop-session.cjs:83-101`) is a pure lookup/fallback function
  (`KNOWN_CONTEXT_WINDOWS` table plus prefix heuristics) so the host's context
  guardian UI sees one honest number regardless of which provider is live.

## 6. Pure-fold candidates worth lifting

These are expressible today as `fn(args) -> value` with zero IO, matching
tinyhivemind-core's constraint set:

- `applyProviderReasoningControls(body, ctx)` and every `apply<Provider>` /
  `is<Provider>Route` function in `tools/provider-maps.cjs:30-252` — a pure,
  ordered "first matching route wins" ladder over `{modelId, baseUrl,
  parameters}`, directly analogous to `responder_plan`'s ladder in
  `docs/specs/responders.md`. This is the strongest lift candidate in the
  repo: same shape as a `Selector`-adjacent decision, but for provider wire
  format instead of turn ownership.
- `param(parameters, id)` (`tools/provider-maps.cjs:36-43`) — trivial pure
  lookup helper.
- `check_anchor(text, anchor, label)` (`tools/apply-box-patch.py:50-53`) — a
  pure precondition predicate gating a write, structurally identical to a
  tinyhivemind guard clause.
- `requireTraceContext(agentId, baseUrl)` (`box/openai-hop-session.cjs:125-142`)
  — pure validation (UUID regex + URL shape check), throws or returns a small
  value; no IO.
- `reportedContextWindow(modelId, baseUrl, parameters)`
  (`box/openai-hop-session.cjs:83-101`) — pure table lookup with prefix
  fallback.
- `repairTruncatedJson` / `canonicalToolArguments`
  (`tools/provider-maps.cjs:255-345`) — pure string/JSON repair, no IO,
  already unit-tested in isolation.

Everything else of substance (the actual HTTP calls, SSE parsing, file
writes, provenance logging, doctor's socket probes) is deliberately impure and
would map to a tinyhivemind **port**, not to `tinyhivemind-core`.

## README overclaims vs. code

- The README's "The LiquidGlass Observatory" section (live telemetry, "VERIFIED
  HOP" receipts, mid-chat model swap) describes UI/UX features implemented in
  `box/hud/liquidglass.js` (1,451 lines) and the telemetry/provenance code in
  `box/openai-hop-session.cjs` — these do exist in source, so this is not an
  overclaim, though it is UI, not the routing core.
- "Zero install into Grok. Injected into the app bundle by opengrok" undersells
  the actual mechanism: `tools/apply-box-patch.py` performs anchored
  literal-string patching of the host's bundled JS (`patch_host()`,
  `tools/apply-box-patch.py:59` onward) — this is a byte-surgical patch of
  vendor code, not a clean plugin/extension API. The README's "Quick start"
  section does self-correct this with an explicit "what this repo is — and is
  not" callout, which is honest about not shipping auth shims or controlling
  the host.
- The "23/23" and "6/6" test claims in the README's Testing section are
  directly verifiable against `tools/test-provider-maps.cjs` (162 lines) and
  `tools/test-provider-maps-hop.cjs` (51 lines) — plausible given file size,
  not verified line-by-line here.
- `.contracts/validation-contract.md` itself flags an unresolved gap the
  README does not mention: "Live-wire (on-box) patch validation is BLOCKED"
  (bottom of file) — i.e., the actual host-bundle patch path was, per the
  maintainers' own contract doc, not live-verified at the time of that
  document, only "port plan" verified. Worth noting as a caveat on how
  "verified" the patch mechanism really is end-to-end.

## Mechanism -> tinyhivemind has -> does not have

| Mechanism (from opengrok) | tinyhivemind equivalent today | Gap / what's missing |
|---|---|---|
| Pure `applyProviderReasoningControls(body, ctx)` route ladder over `{modelId, baseUrl, parameters}` (`tools/provider-maps.cjs:118`) | `responder_plan`'s deterministic ladder over mentions/desk/roster (`docs/specs/responders.md`) — same "first match wins, pure fold, fail-closed to a default" shape | tinyhivemind's ladder picks a *responder*, not a *model*; there is no analogous "per-agent model/provider selection" concern in scope today — arguably out of scope by design (model call is behind the `Selector` port, not a routing table) |
| `HopPromptExecutor` duck-typed port (`stream()` returns a fixed async event vocabulary) (`box/openai-hop-session.cjs:967`) | `Selector` port returning plain text (`docs/specs/responders.md`) | tinyhivemind's `Selector` is deliberately much narrower (one text response, no streaming, no tool events) — opengrok's port is a full conversational adapter, tinyhivemind's is a single selection call; no direct gap, different problem size |
| `requireTraceContext` fail-closed UUID/URL validation gating executor construction (`box/openai-hop-session.cjs:125`) | Fail-closed validation on selector output / ambiguous agent names (`docs/specs/responders.md`: "Selector absence/failure and invalid output deterministically choose the first candidate") | Same fail-closed philosophy exists in tinyhivemind for selection; no direct precedent yet for validating a *host-supplied identity/URL* since tinyhivemind takes no host callbacks by design (rule 2) |
| Local, keys-never-in-config credential boundary (loopback-only hop shim; `model-bindings.json` holds no secrets) (`docs/MODEL-GUIDELINES.md:29-37`) | The host owns storage/credentials entirely (charter rule 1); tinyhivemind never touches credentials at all | Not a gap — tinyhivemind's stricter rule (host owns *all* storage) already subsumes opengrok's narrower "keys stay in one shim" policy |
| JSONL provenance/audit log per request, 0600-permissioned (`box/openai-hop-session.cjs:43-46,157-186`) | No equivalent — tinyhivemind has no notion of an audit trail; a host could log outside the crate, but there is no port for it | Missing: no `AuditLog`-style port or documented convention for hosts wanting a per-turn provenance record of *which* selector/model decision fired and why |
| Anchor-count fail-closed patch gate (`tools/apply-box-patch.py:50`) protecting a destructive rewrite | None — tinyhivemind never mutates host bundles or files (rule 1: host owns storage) | Not applicable; tinyhivemind's storage abstinence makes this class of risk moot |
| `repairTruncatedJson`/`canonicalToolArguments` pure JSON repair for degraded upstream tool-call streams (`tools/provider-maps.cjs:255`) | No equivalent; tinyhivemind-core does not process model output at all | Not a gap for tinyhivemind's current scope (P4/P7 own message projection, not raw model-output repair) — worth flagging as a *pattern* (pure repair function, easy to test) if a future host-side adapter needs it |
