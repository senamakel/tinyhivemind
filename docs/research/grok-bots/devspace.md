# devspace

- **What it is**: a self-hosted MCP server (plus an optional local-agent daemon)
  that lets remote AI clients (ChatGPT, Claude, Codex, etc.) open a real local
  project as an "MCP workspace" and read/write files and run shell commands on
  the host machine, or delegate the work to a locally-installed coding-agent
  CLI (Claude Code, Codex, Grok, Pi, opencode, GitHub Copilot) over ACP.
- **URL**: https://github.com/Waishnav/devspace
- **SHA researched**: `321d09e1134de85af406bde9985bde6af5dd82bf`
- **Licence**: MIT (`LICENSE:1-3`, copyright Waishnav, 2026)
- **Size**: TypeScript/Node project, `pnpm` workspace; `src/*.ts` alone is
  ~26,600 lines across ~100 files (`wc -l` over `src/**/*.ts`). No Rust.

## Architecture overview

- `src/server.ts` — Express + `@modelcontextprotocol/sdk` HTTP MCP server.
  Registers the shared `open_workspace` / `read` tools and OAuth middleware.
- `src/tool-surfaces/{claude,codex,shared,types}.ts` — per-client MCP tool
  registration. `types.ts` is the actual "port" a tool surface implements
  (`ToolSurface { register, instructions }`).
- `src/workspaces.ts` — the `WorkspaceRegistry`: opens a workspace (checkout or
  managed git worktree) under an allowlisted root and resolves every
  file/command path against it.
- `src/roots.ts` — the pure path-containment primitives used everywhere above.
- `src/oauth-provider.ts`, `src/oauth-store.ts` — a single-user OAuth
  authorization-code server gating *client connection* (not per-command
  execution) behind an "Owner password".
- `src/local-agent-*.ts` + `bin/devspace-agentd.js` — a separate subsystem: a
  background daemon that launches real coding-agent CLIs (Claude Code, Codex,
  Grok/opencode/Pi via ACP, Copilot) against a workspace, each session tagged
  with a `writeMode` (`read_only` | `allowed` | `full_access`).
- `src/logger.ts` — structured JSON/pretty logging, no persistent audit store.
- `src/review-checkpoints.ts` — git-ref-based "what changed since X" diff
  summaries for a human to review after the fact (`show_changes`), not a
  pre-execution gate.

## 1. The client/tool port shape

The actual extension point for "a client must implement X" is
`ToolSurface` in `src/tool-surfaces/types.ts:76-91`:

```ts
export interface ToolRegistrationContext {
  server: Pick<McpServer, "registerTool" | "registerResource">;
  config: ServerConfig;
  workspaces: WorkspaceRegistry;
  processSessions: ProcessSessionManager;
}
export interface ToolInstructionContext { agents: string; skills: string; }
export interface ToolSurface {
  register(context: ToolRegistrationContext): void;
  instructions(context: ToolInstructionContext): string;
}
```
(`src/tool-surfaces/types.ts:76-91`)

Concrete tool schemas (Zod, MCP `registerTool`) live per client, e.g. the
shell tool for Claude-shaped clients:

```ts
server.registerTool(toolNames.shell, {
  title: "Bash",
  description: CLAUDE_SHELL_DESCRIPTION,
  inputSchema: {
    workspaceId: z.string(),
    command: z.string(),
    workingDirectory: z.string().optional(),
    timeout: z.number().positive().max(300).optional(),
  },
  outputSchema: resultOutputSchema(),
  annotations: SHELL_TOOL_ANNOTATIONS,
}, async ({ workspaceId, workingDirectory, ...input }) => { ... })
```
(`src/tool-surfaces/claude.ts:171-215`)

Every tool call resolves a `workspaceId` (from `open_workspace`) to a
`Workspace`, resolves the path/cwd through `WorkspaceRegistry`, runs the
underlying op (`pi-tools.ts`'s `writeFileTool`/`editFileTool`/`runShellTool`),
and returns `{ content, structuredContent }`. Tool names live in
`toolNames` (`src/tool-surfaces/types.ts:8-14`): `open_workspace`, `read`,
`write`, `edit`, `bash` (Claude surface) / `apply_patch`, `exec_command`,
`write_stdin` (Codex surface, `src/tool-surfaces/codex.ts:20`).

The second, unrelated "client" shape is the ACP local-agent adapter: any
locally installed coding-agent binary just needs to speak the Agent Client
Protocol (`@agentclientprotocol/sdk`) — DevSpace's client wraps
`methods.client.session.requestPermission` and
`methods.client.session.update` (`src/local-agent-acp.ts:493-503`).

## 2. Approval-gate model

There are **two separate, non-interchangeable "approval" mechanisms** —
neither is a per-command human-in-the-loop prompt for the direct MCP tool
surface:

**(a) Connection-level approval (OAuth "Owner password"), for the MCP endpoint
itself.** Any MCP client (e.g. ChatGPT) doing OAuth discovery hits
`SingleUserOAuthProvider.authorize` (`src/oauth-provider.ts:117-131`), which
renders an HTML form (`formHtml`, `src/oauth-provider.ts:50-98`) asking a human
to type the Owner password. This happens **once per client registration**, not
per command — see `docs/security.md:34-40` ("When an MCP client connects,
DevSpace shows an approval page"). Once authorized, the client holds an
OAuth token and can call `bash`/`write`/`edit` with **no further human
confirmation** — confirmed directly in the tool description string: *"Shell
commands run with the local user's authority and are not sandboxed; workspace
validation only selects their initial working directory."*
(`src/tool-surfaces/claude.ts:26`, repeated in `codex.ts:20`). This is stored
as SQLite rows (`oauth_clients`, `oauth_access_tokens`,
`oauth_refresh_tokens` — `src/db/schema.ts:58-91`), not an audit log of
actions.

**(b) Session-tier approval (`writeMode`), for delegated local-agent runs.**
When DevSpace launches a real coding-agent CLI as a sub-agent
(`src/local-agent-manager.ts`), the caller sets one of
`LocalAgentWriteMode = "read_only" | "allowed" | "full_access"`
(`src/local-agent-runtime.ts:5`), defaulting to `"allowed"`
(`src/local-agent-manager.ts:425`). This tier is chosen **ahead of time**
(by whoever starts the run), not decided interactively per command. Each
provider adapter turns the tier into that provider's own native
permission/sandbox config: Claude Code gets `deny: allowed ? [] : ["Bash",
"Edit"]` plus a `denyWrite` sandbox rule (`src/local-agent-claude.ts:330-341`);
opencode gets a `PermissionConfig` object (`src/local-agent-opencode.ts:189-201`);
Codex gets `approvalPolicy: "never"` plus a sandbox policy
(`src/local-agent-codex.ts:458-469` — note Codex's own approval prompt is
explicitly disabled: `"approvalPolicy": "never"`); Copilot/Grok/other
ACP-speaking agents have their *native* permission-request RPC intercepted
and auto-answered by DevSpace, never surfaced to a human:

```ts
.onRequest(methods.client.session.requestPermission, (context) => {
  const writeMode = sessionWriteModes.get(context.params.sessionId);
  const selected = selectAcpPermissionOption(context.params.options, writeMode, this.provider);
  return selected
    ? { outcome: { outcome: "selected", optionId: selected.optionId } }
    : { outcome: { outcome: "cancelled" } };
})
```
(`src/local-agent-acp.ts:493-500`)

**Is the gate a pure predicate separable from execution?** Yes, for (b):
`selectAcpPermissionOption` (`src/local-agent-acp.ts:762-778`) is a pure
function `(options, writeMode, provider) -> { optionId } | undefined` with no
IO — it purely picks `allow_once`/`allow_always` vs `reject_once`/
`reject_always` from the option list handed to it. The IO (actually replying
to the ACP `requestPermission` RPC) is a separate, thin wrapper around the
call site. For (a), the gate is **entangled**: `authorize()` in
`oauth-provider.ts` combines validation (resource/scope checks) with
side-effecting response rendering (`res` is an Express `Response` passed
straight into the method) and DB writes on the token-exchange path
(`oauth-store.ts`) — there is no standalone "would this be approved" function.

## 3. Filesystem scoping

Boundary check, entirely pure, in `src/roots.ts`:

```ts
export function isPathInsideRoot(path: string, root: string): boolean {
  const resolvedPath = resolve(expandHomePath(path));
  const resolvedRoot = resolve(expandHomePath(root));
  const relationship = relative(resolvedRoot, resolvedPath);
  return (
    relationship === "" ||
    (!isAbsolute(relationship) &&
      !relationship.startsWith("..") &&
      relationship !== ".." &&
      !relationship.includes(`..${sep}`))
  );
}

export function assertAllowedPath(path: string, allowedRoots: string[]): string {
  const resolvedPath = resolve(expandHomePath(path));
  if (allowedRoots.some((root) => isPathInsideRoot(resolvedPath, root))) return resolvedPath;
  throw new AccessDeniedError(`Path is outside allowed roots: ${path}`);
}
```
(`src/roots.ts:20-41`)

The project boundary itself is a config-level allowlist,
`ServerConfig.allowedRoots: string[]` (`src/config.ts:16`, default
`[process.cwd()]`, `src/config.ts:64`). `WorkspaceRegistry.openCheckoutWorkspace`
enforces it at workspace-open time (`assertAllowedPath(path,
this.config.allowedRoots)`, `src/workspaces.ts:322`), and every subsequent
file/command path is re-checked against the *workspace root specifically*
(`resolvePath`, `src/workspaces.ts:289-295`; `resolveWorkingDirectory`,
`src/workspaces.ts:319-321`) — so a workspace, once opened under an allowed
root, cannot be used to `../../`-escape to a sibling directory even if that
sibling is itself under an allowed root. This is a pure string/path
computation with `node:path`'s `resolve`/`relative` — no OS-level sandbox,
no chroot, no seccomp: it is a logical boundary, not a kernel-enforced one
(confirmed explicitly for shell commands, see §2 and `docs/security.md:80-84`:
*"Shell commands run as local commands and can do what your user account can
do."*). The one exception is Pi's local-agent sandbox mode, which does wrap
`sandbox-runtime`'s OS-level `denyRead`/`denyWrite`/network allowlist
(`src/local-agent-pi-sandbox.ts:118-135`) — but that only applies to
delegated Pi sub-agent sessions, not the direct MCP `bash` tool.

## 4. Command-approval mechanics — the actual flow

For the direct MCP surface (what a remote client like ChatGPT actually uses),
there is **no per-command approval step**. The flow is:

1. Client completes OAuth once (human types Owner password in the browser
   form — `src/oauth-provider.ts:117-131`, `docs/security.md:34-40`).
2. Client calls `open_workspace` with a path; `WorkspaceRegistry` checks it
   against `config.allowedRoots` (`src/workspaces.ts:322`) and returns a
   `workspaceId`.
3. Client calls `bash`/`exec_command` with `{ workspaceId, command,
   workingDirectory?, timeout? }` (`src/tool-surfaces/claude.ts:171-183`).
4. `workspaces.resolveWorkingDirectory` re-validates the cwd
   (`src/tool-surfaces/claude.ts:196-199` → `src/workspaces.ts:319-321`).
5. `runShellTool` (in `src/pi-tools.ts`) actually spawns the process — no
   human is consulted, no allow/deny decision beyond the path check.
6. Result is logged (`logToolCall`, see §5) and returned to the client.

There is **no hop where a human decision is inserted** in this path — the
"approval" the README/docs describe is entirely step 1 (once per client), and
`docs/security.md:78-84` says so directly under "Shell Access": *"The shell
tool is powerful by design... Shell commands run as local commands and can do
what your user account can do."*

The only place a live human decision is inserted mid-flow is the local-agent
daemon's own coding-agent sessions when the *provider's own* CLI would
otherwise prompt a human (e.g. running Claude Code interactively outside
DevSpace) — DevSpace intercepts and auto-answers that prompt from the
pre-chosen `writeMode` tier (§2b), so even there no human is asked live;
`writeMode` is the human's one decision, made when the session/profile is
configured, not per command.

## 5. Audit logging

No dedicated audit-log table or file — logging is `console.log`/`console.error`
structured JSON (or pretty text), not persisted (`src/logger.ts:30-53`):

```ts
export function logEvent(config, level, event, fields = {}): void {
  if (!shouldLog(config, level)) return;
  const entry = { ts: new Date().toISOString(), level, event, ...fields };
  const line = config.format === "pretty" ? formatPretty(entry) : JSON.stringify(entry);
  ...
}
```

Tool-call logging fields (`ToolLogFields`, `src/tool-surfaces/types.ts:44-54`):
`tool, workspaceId?, path?, workingDirectory?, command?, commandLength?,
success, durationMs, error?`. The shell command text itself (`commandPreview`)
is redacted by default and only included when `logging.shellCommands` is
explicitly turned on (`src/tool-surfaces/shared.ts:36-46`,
`src/config-schema.ts:49-50`; docs warn: *"Do not enable shell command logging
if commands may contain secrets"*, `docs/security.md:100`). Artifact downloads
get their own event (`artifact_tool_call`, `src/artifact-tools.ts:311-333`)
with hostname/path/size/sha256 but never the raw file value or credentials.

**Coupling to the approval check**: logging is coupled to *execution*, not to
the OAuth approval step. `runLoggedToolOperation`/`logToolCall`
(`src/tool-surfaces/shared.ts:36-72`) wrap the operation and log
success/failure **after** the command has already run — there is no log entry
for "would have run but was denied" in the direct MCP path, because nothing is
denied there; the only denial path is `AccessDeniedError` from `roots.ts`,
which surfaces as a normal tool error and is logged via
`logFailedToolResponse` (`src/tool-surfaces/shared.ts:89-101`). OAuth
authorization failures are logged separately as `auth_denied`
(`src/server.ts:910`). So: two independent log call-sites, no shared
"decision" record joins them.

## 6. Pure-fold candidates worth lifting

- **`isPathInsideRoot` / `assertAllowedPath` / `resolveAllowedPath`**
  (`src/roots.ts:20-46`) — a total fold over `(path, root[s])` with no IO,
  returning a definite in/out-of-bounds answer. Directly analogous to a
  `Allow | Deny` boundary predicate tinyhivemind's host boundary could reuse
  as a pattern for "is this path/handle within the session's declared scope."
- **`selectAcpPermissionOption(options, writeMode, provider)`**
  (`src/local-agent-acp.ts:762-778`) — takes an *action's* offered options, a
  policy tier, and static provider identity, and returns
  `{ optionId } | undefined` (i.e. Allow-once/Allow-always vs Reject vs "no
  opinion, cancel it") with zero IO. This is the closest concrete precedent in
  devspace to the requested "`(action, policy, history) -> Allow/Deny/Ask`"
  shape — it is missing the `history` argument (no rate-limiting/backoff by
  past decisions) but is otherwise exactly that kind of pure gate function,
  cleanly separated from the RPC reply plumbing around it.
- **`requestedScopesAllowed(requested, supported)`**
  (`src/oauth-provider.ts:100-102`) — trivial pure fold, `requested.every(s
  => supported.includes(s))`.
- By contrast, `WorkspaceRegistry.openCheckoutWorkspace`,
  `SingleUserOAuthProvider.authorize`, and `runLoggedToolOperation` are all
  IO-entangled — the pure/impure split in devspace is real but partial: the
  boundary *checks* are pure, the *decisions to act on client identity/OAuth
  state* are not extracted into anything as clean as `selectAcpPermissionOption`.

## README overclaim check

None found that materially overstate the code. `README.md:21` says "approve
the connection with a password only you have" (singular "connection," matches
code: one-time OAuth, not per-command). `docs/security.md` is unusually candid
about the shell tool being unsandboxed and about `writeMode`/logging limits —
if anything the docs undersell how little gating exists rather than
oversell it; the task brief's framing ("command approval... who approves")
is a reasonable question to ask of devspace, but devspace itself never claims
a human approves individual commands — only the one-time client connection.

## Mechanism -> tinyhivemind has -> does not have

| Mechanism (from devspace) | tinyhivemind equivalent today | Gap / what's missing |
|---|---|---|
| `assertAllowedPath`/`isPathInsideRoot` pure path-containment fold (`src/roots.ts:20-41`) | No filesystem concept at all — tinyhivemind never opens files; storage is entirely host-owned per the charter | Not a gap by design (rule 1), but if a future host-boundary spec needs a "scope containment" predicate, `roots.ts` is a clean, minimal precedent to imitate as a pure fold, not to port directly |
| `selectAcpPermissionOption(options, policy, provider)` pure allow/deny/cancel predicate (`src/local-agent-acp.ts:762-778`) | Nothing that decides "should this external action proceed" — the mention-dispatch spec governs *whose turn it is*, not *whether an action is approved* | tinyhivemind has no analogous `Action -> Allow/Deny/Ask` port at all; devspace shows the shape works well as a pure function taking policy + offered options, worth sketching as a future host port (e.g. for a `run_command`-style hive action) rather than folding approval logic into the host callback |
| One-time OAuth "Owner password" gate on client connection (`src/oauth-provider.ts:117-131`) | No connection/identity gate concept — a host implements its own transport auth before ever calling into tinyhivemind's ports | Consistent with rule 1 (host owns storage/transport); nothing to add here, devspace's split (connection auth vs execution auth) is a useful cautionary example of how *not* per-action gating can look deceptively like a security boundary |
| Structured JSON tool-call logging coupled to execution, not to any decision record (`src/tool-surfaces/shared.ts:36-72`) | No logging/audit concept in the pure crate; a host is expected to instrument its own port implementations | Gap: tinyhivemind's specs (mention-dispatch, expert-delegation) do not yet describe what a host *should* log about a dispatch decision (chosen desk, mention resolution, referral) for auditability — devspace shows logging is easy to bolt on after the fact but does not compose with an actual approval decision unless deliberately joined by a shared record, which is a lesson for any future audit-log guidance tinyhivemind gives to hosts |
| `writeMode` tiering (`read_only`/`allowed`/`full_access`) set once per delegated session (`src/local-agent-runtime.ts:5`) | `expert-delegation.md`'s specialist-tier seating (see `docs/specs/expert-delegation.md`) already has a comparable "decide capability tier once, up front, not per turn" pattern | No gap in kind; devspace's tiers are coarser (3 buckets, no per-command re-evaluation) — tinyhivemind's tier-seating is a stronger design already, nothing to borrow |
