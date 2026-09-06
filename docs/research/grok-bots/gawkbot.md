# gawkbot (module: `github.com/nex-crm/wuphf`)

- **What it is**: an open-source "Grok Bot" clone — a local (or cloud-VM)
  runtime that turns a written request into a team of persistent Slack/Telegram
  "office" bots, each with its own tool allowlist, a scheduler, and a human
  approval gate on mutating external actions (email, CRM, calendar, etc. via
  Composio). Internally the project (and its Go module/import path, package
  names, binary `wuphf`, env var prefix `WUPHF_*`) is called **wuphf**; "gawkbot"
  is the outward product name in the README/CLI (`npx gawkbot`, `gawkbot share`).
- URL: https://github.com/najmuzzaman-mohammad/gawkbot
- SHA at research time: `bbb06884142760f4186390c804383142b248f304`
- Licence: **not pure open source.** `LICENSE` is a "Sustainable Use License
  v1.0" (verified — the badge is accurate): free to use/modify for internal or
  non-commercial use, redistribution only free-of-charge and non-commercial,
  no sublicensing. Not OSI-approved despite "open source" language throughout
  the README.
- Size: 1,403 `.go` files, ~397k lines of Go under the tracked dirs (`internal`
  15M, `web` 10M/TS frontend, `packages` 4.5M, `cmd` 1.7M, `agent` 340K,
  `runner` 72K). `internal/team` (the "broker") and `internal/teammcp` (the
  MCP server bots call into) are by far the largest packages — this is where
  almost all product logic (approval, scheduling, tasks, integrations) lives.

## Architecture overview

- `cmd/wuphf/` — the TUI/CLI entrypoint (`gawkbot`/`wuphf` binary), Slack-like
  chat UI (`channel*.go`), onboarding wizard.
- `internal/team/` — the **broker**: an in-process HTTP server (`Broker`
  struct) holding all runtime state (tasks/"Issues", channels, bots, the
  scheduler, requests/approvals, action grants) as plain Go structs, persisted
  as one JSON snapshot to disk (`internal/team/broker_persistence.go:280-355`,
  `os.WriteFile`/`json.Marshal` — no SQL DB). It also drives the headless coding
  agents (`headless_claude.go`, `headless_codex_runner.go`) and Slack/Telegram
  transports.
- `internal/teammcp/` — the MCP server the bot's own LLM session talks to
  (`team_action_execute`, `team_request`, etc.). This is where the approval
  **gate** actually lives, in front of `internal/action`'s provider calls.
- `internal/action/` — the pure classification/resolver layer
  (`resolver.go`) plus the Composio `Provider` implementation
  (`composio.go`) that actually calls out over HTTP.
- `internal/operations/` — the "Blueprint" IR: a written request gets
  synthesized into a `Blueprint` (bots, channels, tools, approval rules,
  workflow templates) that seeds a new team.
- `internal/bot/` — a **separate**, smaller local bot-loop (`BotLoop`) with
  its own tool registry (`read_file`, `grep_search`, `write_file`, `bash`,
  `send_message`). This loop has **no approval gate at all** (see §3).
- `internal/computer/`, `internal/openclaw/` — VM/"computer" lifecycle and a
  computer-use client, for the "its own screen" claim; not investigated in
  depth (out of scope for the approval/routing/scheduling questions).

## 1. Routing / tool-selection decision

There is **no scoring function that picks "which tool/model/agent runs."**
Tool selection is ordinary LLM function-calling: the bot's underlying coding
CLI (Claude Code / Codex / Opencode, wired in `internal/provider/*.go`) is
handed the full tool/MCP schema and the LLM decides which tool to call each
turn. `BotLoop.executeTool` (`internal/bot/loop.go:549-641`) just looks up the
tool by name the LLM chose (`l.tools.Get(tc.ToolName)`, line 561) and validates
params (`l.tools.Validate`, `internal/bot/tools.go:63-93`) — no policy, no IO
before the lookup, but also no *decision* to separate: it is a fold from
`(ToolRegistry, ToolCall)` (`internal/bot/tools.go:22-56`) to a validity
verdict, then unconditionally executes.

The one place gawkbot *does* make a real classification decision before
execution is the **external-action gate**, and there the decision is cleanly
pure and separated from IO:

- `action.Classify(ClassifyInput{ReadOnly, State, HasGrant}) Decision`
  (`internal/action/resolver.go:104-119`) — a `switch` with no IO, fully unit
  testable, returns `proceed | approve | connect | wait | fail_safe |
  fallback`.
- `action.Resolve(ResolveInput) (Decision, ConnectionState)`
  (`internal/action/resolver.go:150-162`) folds a fresh probe + cached
  registry state into the same pure `Classify`, so even the fail-safe/outage
  logic is pure and exhaustively testable.
- Model selection (which coding CLI backs a bot) is a static per-bot config
  field, not a computed decision — see `internal/provider/resolver.go` and
  `internal/team/broker_defaults.go` for the provider string on the bot record;
  it is chosen at bot-creation time, not scored per-turn.

**Verdict for tinyhivemind**: `action.Classify`/`action.Resolve` are the
strongest positive precedent for "pure decision function separable from
execution" in this codebase — exactly the shape `tinyhivemind-core` wants for a
mention-resolution or bid-classification fold. Tool *selection* itself,
however, is not decided by gawkbot's own code at all; it is delegated whole to
the LLM's function-calling, so there is nothing to lift there.

## 2. The tool interface (port shape)

Two distinct "tool" shapes exist, at two different layers:

**a) `internal/bot.BotTool`** (`internal/bot/types.go:51-56`) — the local
in-process tool a `BotLoop` executes directly:

```go
type BotTool struct {
    Name        string
    Description string
    Schema      map[string]any // JSON Schema, hand-built map[string]any
    Execute     func(params map[string]any, ctx context.Context, onUpdate func(string)) (string, error)
}
```

Registered/looked-up via `ToolRegistry` (`internal/bot/tools.go:20-56`,
`Register`/`Unregister`/`Get`/`List`/`Has`/`Validate`). `Execute` takes the
params, a `context.Context`, and a progress callback (`onUpdate`), and returns
`(resultJSON string, error)`. Built-ins: `read_file`, `grep_search`, `glob`,
`write_file`, `bash`, `send_message` (`internal/bot/tools.go:135-459`).

**b) `internal/action.Provider`** (`internal/action/types.go:59-77`) — the
external-integration surface (what a bot's `team_action_execute` MCP call
ultimately reaches through Composio):

```go
type Provider interface {
    Name() string
    Configured() bool
    Supports(Capability) bool
    Guide(ctx, topic) (GuideResult, error)
    ListConnections(ctx, opts) (ConnectionsResult, error)
    SearchActions(ctx, platform, query, mode) (ActionSearchResult, error)
    ActionKnowledge(ctx, platform, actionID) (KnowledgeResult, error)
    ExecuteAction(ctx, req ExecuteRequest) (ExecuteResult, error)
    CreateWorkflow / ExecuteWorkflow / ListWorkflowRuns(...)
    ListRelays / RelayEventTypes / CreateRelay / ActivateRelay / ListRelayEvents / GetRelayEvent(...)
}
```

`IntegrationProvider` (`internal/action/types.go:81-87`) extends it with
connect/disconnect/status for OAuth-style account management. This is a much
richer, provider-agnostic surface than `BotTool` — every method takes typed
request/result structs (e.g. `ExecuteRequest`/`ExecuteResult`,
`internal/action/types.go:229+`) rather than a bare `map[string]any`.

**Takeaway**: there is no single unifying "Tool" trait in gawkbot; local
tools and external actions are two unrelated interfaces. tinyhivemind's own
`Selector`/`Port` shapes should not try to unify these either — they are
answering different questions (bounded local capability vs. external side
effect needing a trust boundary).

## 3. The approval-gate model

### What is actually gated

The **only** hard-gated, human-in-the-loop mutating surface is
`team_action_execute` in `internal/teammcp/actions.go`, guarded by
`requireTeamActionApproval` (`internal/teammcp/actions.go:83-267`). It gates
**any external action** whose `action_id` is not read-only per
`action.ActionIsReadOnly` (verb-based classification,
`internal/action/resolver.go:159-236`) — this covers Gmail sends, Slack
posts, CRM writes, calendar mutations, etc. Additionally:

- `requireHumanCreateApproval` (`internal/teammcp/create_approval.go:44-58`)
  gates creating a new bot or channel (`member_approval.go`,
  `channel_approval.go`) — a persistent structural change, not a "send."

**What is NOT gated**, despite the README's blanket claim:

- **`internal/bot.BotLoop.executeTool`** (`internal/bot/loop.go:549-641`) runs
  `bash`, `write_file`, `read_file`, etc. with **zero approval check** — no
  call to any gate function anywhere in `internal/bot/loop.go` or
  `internal/bot/tools.go`. A bot on this loop can run arbitrary shell commands
  (including things that "send" data over the network via `curl`, or delete
  files) with no human click.
- **Git commits** made by the headless coding agents
  (`internal/team/headless_claude.go`, `internal/team/headless_codex_runner.go`,
  via `internal/gitexec/gitexec.go:68-84`, `Run`/`RunOK`) are plain
  `exec.Command("git", ...)` calls with **no approval gate anywhere in
  `internal/gitexec`** — grep for `approv` in that package returns nothing.
  Since a coding-agent bot's own `bash` tool use is what actually runs `git
  commit`/`git push`, a "commit" from gawkbot's own coding bots never reaches
  `requireTeamActionApproval` at all; that gate only fires for Composio-style
  platform actions (`platform`/`action_id` pairs), not for git operations.
- **Purchases/deletes** are gated only insofar as their action_id contains a
  mutating verb classified by `mutatingActionVerbs`
  (`internal/action/resolver.go:190-211`, includes `charge`, `pay`, `refund`,
  `delete`) — this is a heuristic string-token classifier, not a semantic
  understanding of "this is money" or "this is destructive." A vendor whose
  action IDs don't include one of the ~45 listed verbs (e.g. a platform verb
  the authors didn't anticipate) would slip through as non-mutating.

### Explicit bypass paths (grep-verified)

1. `args.DryRun` — dry-run calls never gate (`actions.go:85-86`).
2. `os.Getenv("WUPHF_UNSAFE") == "1"` — global bypass, set by the `--unsafe`
   CLI launch flag (`cmd/wuphf/main.go:647,727`); bypasses the action gate
   (`actions.go:87-88`), the create-approval gate
   (`create_approval.go:55-56`), and is referenced in `server_channel_tools.go:197`
   as bypassing specialist routing too.
3. `actionIsReadOnly(args.ActionID)` — read-only actions skip the modal
   entirely (`actions.go:90-91`); classification is a verb-token heuristic
   (§ above), not a hard allowlist.
4. **Standing grants**: `preApproved` short-circuits the human prompt
   (`actions.go:130-139`). A grant is minted per exact `(bot, platform,
   action_id)` tuple when a human clicks "Approve & always allow"
   (`internal/team/broker_action_grants.go:32-40`), capped at 30 days
   (`maxGrantTTL`, `broker_action_grants.go:53`), and checked by
   `hasActiveActionGrant`/`actionGrantActive`
   (`broker_action_grants.go:75-98`) — both pure predicates over
   `(actionGrant, time.Time)`. The code's own comment
   (`broker_action_grants.go:18-30`) flags that grant CRUD relies entirely on
   the broker token as the trust boundary — a bot with shell access could in
   principle mint or reach `/integrations/grants` itself, which the authors
   call out as flagged-for-review, not fixed.

### Who approves / how it's recorded

- A gate creates a `humanInterview` record (`internal/team/broker_types.go:115-155`,
  fields: `ID, Kind, Status, Question, Context, Options, RecommendedID,
  Blocking, Required, DedupeKey, IssueID, ...`) via `POST /requests`
  (`actions.go:157-184`), then polls `GET /interview/answer?id=...`
  (`actions.go:224-229`) every 1.5s (`actionApprovalPollInterval`,
  `actions.go:27`) for up to 30 minutes (`actionApprovalTimeout`,
  `actions.go:23`).
- The human answers through the Slack card (`internal/team/slack_task_cards.go`)
  or the web/TUI "Requests" panel (`handleRequests`,
  `internal/team/broker_requests_interviews.go:560`). Choices are
  `approve`/`approve_with_note`/`confirm_proceed` (proceed),
  or reject/hold/cancel/timeout (blocked; `actions.go:255-267`).
- A durable audit trail is a separate struct, `ApprovalAuditEntry`
  (`internal/team/broker_approval_audit.go:20-35`: request id, task id,
  platform, action id, connection key, requested/answered/executed
  timestamps, outcome, actor, channel), appended to `b.approvalAudit` in
  memory and flushed into the same host-owned JSON snapshot file as the rest
  of broker state (`internal/team/broker_persistence.go:280-355`) — there is
  no SQL schema; it's a plain Go slice serialized to disk.

### Pure vs. entangled

The **read/write classification and grant-liveness checks are pure**
(`ActionIsReadOnly`, `Classify`, `actionGrantActive` — all take plain values,
no IO, exhaustively unit-tested per their doc comments). But the actual
*gate function*, `requireTeamActionApproval`, is **not** separable from
execution: it is one long function that (a) classifies, (b) auto-resolves an
"Issue" over HTTP (`resolveActionIssue`, line 96), (c) POSTs the approval
card, (d) polls, (e) writes audit entries — all inline. The pure classify
step could be pulled out and tested alone (and is, via
`action.ActionIsReadOnly`/`Classify`), but the orchestration around it is a
single IO-heavy function, not a decision-then-effect pipeline.

### README claim vs. code — verdict

**Overclaim, confirmed.** "Every send, commit, purchase, and delete waits for
your click" is false as a universal claim:
- **commit** is never gated (no approval call anywhere in `internal/gitexec`
  or the headless coding-agent runners).
- Local shell/file tools on `internal/bot.BotLoop` (a real, wired-up code path,
  not dead code) have no gate at all — a `bash` tool call can send data over
  the network or delete files without ever reaching
  `requireTeamActionApproval`.
- The gate that does exist is bypassable via `--unsafe`/`WUPHF_UNSAFE=1`,
  dry-run, read-only classification (heuristic), and 30-day standing grants —
  all legitimate design choices, but each is a hole in "every ... waits for
  your click," and the grant path is explicitly flagged by the authors'
  own comments as resting on a single shared broker-token trust boundary
  rather than a per-action human decision each time.

## 4. Scheduling

No cron-expression parser exists. A scheduled job is a plain struct,
`schedulerJob` (`internal/team/broker_types.go:787-817`): `Slug, Kind, Label,
TargetType, TargetID, Channel, Bot, Provider, ScheduleExpr, WorkflowKey,
IntervalMinutes, DueAt, NextRun, LastRun, Status, Payload, Enabled,
IntervalOverride`. `ScheduleExpr` is a free-text field carried through but not
parsed by a cron engine in the code paths inspected — the actual "due" check
is interval/timestamp based:

```go
func schedulerJobDue(job schedulerJob, now time.Time) bool { // internal/team/broker_scheduler.go:186-199
    if status is done/canceled { return false }
    if DueAt set and !DueAt.After(now) { return true }
    if NextRun set and !NextRun.After(now) { return true }
    return false
}
```

This is a pure predicate over `(schedulerJob, time.Time)` — no IO — and is
the canonical "is this due" check (`dueSchedulerJobsLocked`,
`broker_scheduler.go:14-26`, explicitly calls it out as "the canonical due
predicate" so DueAt-only and NextRun-only jobs are both covered). A separate
`systemCronSpec` registry (`broker_scheduler.go:386-466`) self-registers
fixed internal maintenance jobs (`request_follow_up`, `review-expiry`,
`task_recheck`, etc.) with a human-overridable `IntervalMinutes` — these are
plain repeating intervals, not calendar cron syntax.

**Trigger**: an internal poller (invoked from `handleScheduler`,
`broker_scheduler.go:357-384`, and the run-loop referenced in
`registerSystemCrons`) periodically calls `dueSchedulerJobsLocked(now)` and
runs each due job (`handleRunSchedulerJob`, `broker_scheduler.go:719-810`),
then reschedules or marks it done.

## 5. Recorded-demonstration → reusable agent

The README's "demo it once on a call" claim does **not** correspond to any
screen/video recording ingestion pipeline — there is no `.mp4`/`.webm`/HAR
capture, no meeting-transcription integration, and no "trace replay" IR file
format in the codebase (verified by grep across `internal/` and `cmd/` for
video/screencast/HAR/meeting terms — only unrelated hits in youtube/image-gen
tooling). What actually exists is **workflow mining over the office's own live
tool-call telemetry**:

- `internal/team.DetectWorkflows(manifests []TurnManifest, opts DetectOptions)
  []DetectionCandidate` (`internal/team/workflow_detect.go:338-451`) is a
  **pure fold**: it takes the recorded tool-call manifests of past agent
  turns (`TurnManifest`, already in the caller's hands — read from a JSONL
  sink by the IO wrapper `DetectWorkflowsFromSink`,
  `workflow_detect.go:454-460`), reduces each task to its "shape" (the
  ordered distinct non-plumbing tools it used —
  `taskShape`, `workflow_detect.go:261-280`, dropping `internal/team`'s own
  MCP calls via `isOrchestrationTool`, `workflow_detect.go:248-259`), clusters
  tasks with matching (or near-matching, or order-insensitive) shapes into
  `detectCluster`s, and surfaces a `DetectionCandidate{Fingerprint, Shape,
  Bot, TaskIDs, Count, Outcome}` when a shape recurs enough times or a single
  run reached a recognized terminal "outcome" verb (send/post/publish/...).
- This candidate is turned into a proposal via an LLM call
  (`buildWorkflowDetectPrompt`, `internal/team/broker_workflow_detect.go:547-604`,
  and `parseWorkflowDetectDecision`, line 605), raised as a human-approved
  card (`raiseDetectedAppProposal`, `broker_workflow_detect.go:462-546`), and
  on approval materialized into a "microapp" (`internal/team/broker_apps_scaffold.go`)
  or a `WorkflowTemplate` on an `operations.Blueprint`
  (`internal/operations/types.go:263-282`: `ID, Name, Trigger, Mode, Schedule,
  Integrations, Checklist, Definition map[string]any, SmokeTest`).

So the **intermediate representation** is two-layered:
1. `TurnManifest`/`DetectionCandidate` — the mined shape of what the bot
   *already did once or twice*, a plain Go struct with no schema file (just
   the Go type definitions above).
2. `operations.Blueprint` — the durable, schedulable IR a written request OR
   a detected workflow both compile down into (see `internal/operations/types.go:3-49`):
   name/objective, `StarterPlan` (bots + channels + tasks), `ApprovalRules`,
   `Connections`, and `[]WorkflowTemplate`. `synthesizeGenericBlueprint`
   (`internal/operations/generic_synthesis.go:11-49`) is itself a large,
   deterministic (no LLM call, no IO) fold from a `SynthesisInput` (free-text
   directive + company profile) to a `Blueprint` — string/keyword-driven
   template filling, not ML synthesis.

**Verdict**: "recorded demonstration" in gawkbot means "the bot did the task
once live via chat and its tool-call trace got mined," not screen recording of
a human. This is a materially narrower mechanism than the README phrase
suggests, but it is real, working code with a genuinely pure detection core.

## 6. Candidates for a pure fold (tinyhivemind-core signal)

| Function | File:line | Signature shape | Note |
|---|---|---|---|
| `action.Classify` | `internal/action/resolver.go:104-119` | `(ClassifyInput) Decision` | switch over 3 plain fields, exhaustive |
| `action.Resolve` | `internal/action/resolver.go:150-162` | `(ResolveInput) (Decision, ConnectionState)` | folds live probe + cached state, no IO |
| `action.ActionIsReadOnly` | `internal/action/resolver.go:222-236` | `(string) bool` | token-based verb classifier |
| `actionGrantActive` | `internal/team/broker_action_grants.go:75-84` | `(actionGrant, time.Time) bool` | fail-closed liveness predicate |
| `schedulerJobDue` | `internal/team/broker_scheduler.go:186-199` | `(schedulerJob, time.Time) bool` | canonical "is this due" fold |
| `DetectWorkflows` | `internal/team/workflow_detect.go:338-451` | `([]TurnManifest, DetectOptions) []DetectionCandidate` | clustering/mining fold, no IO |
| `taskShape` | `internal/team/workflow_detect.go:261-280` | `([]TurnManifest) []string` | reduces turns to ordered tool shape |
| `synthesizeGenericBlueprint` | `internal/operations/generic_synthesis.go:11-49` | `(SynthesisInput) Blueprint` | deterministic text→struct template fill |
| `actionApprovalDedupeKey` | `internal/teammcp/actions.go:270-278` | `(string, TeamActionExecuteArgs) string` | explicitly documented "pure for testability" |

All of these take plain, caller-held values and return plain values with no
`context.Context`, no channel/DB/HTTP client in the signature — exactly the
shape `tinyhivemind-core` requires.

## Mechanism → tinyhivemind has → does not have

| Mechanism (from gawkbot) | tinyhivemind equivalent today | Gap / what's missing |
|---|---|---|
| `action.Classify`/`Resolve`: pure decision (read-only? connection state? grant?) → `proceed/approve/connect/wait/fail_safe/fallback` | Expert-delegation's `BidReason`/`!defer` classification is structurally similar (a pure fold to a small enum) | tinyhivemind has no equivalent for "should this side-effecting action require human approval" — no `ApprovalDecision` fold at all; would need a port for "pending approval" storage plus a pure `Classify`-shaped function |
| `humanInterview` + `ApprovalAuditEntry`: typed pending-approval record and a separate durable audit trail | None — tinyhivemind has no concept of an approval request or its outcome | Would need a snapshot/view type (never a host callback, per rule 2) representing "this pending decision," and a host-implemented port for creating/polling it |
| Standing "grant" (per exact bot+platform+action, TTL-capped, pure liveness check) | None; expert-delegation's directory tracks who-knows-what, not standing pre-authorizations | A grant-like mechanism would be a natural pure-predicate addition once approval exists, but nothing today models "skip the check because it was pre-authorized" |
| `schedulerJobDue`: pure "is this job due" predicate over `(job, now)` | None — tinyhivemind has no scheduling concept; the hive/mention-dispatch model is reactive (message-triggered), not time-triggered | Out of current scope per the charter (turn-triggering, not scheduling), but the predicate shape (`(state, now) -> bool`) is a clean template if scheduled hive episodes are ever wanted |
| `DetectWorkflows`: mining recurring tool-call shapes out of past turns into a reusable "workflow" | Nothing — tinyhivemind's projection fold produces turn history for one viewer, not cross-task pattern mining | A hive episode's outcome or an expert-delegation record could feed a similar pure clustering fold to promote a recurring pattern into a durable artifact, but no such fold exists yet |
| `operations.Blueprint`: durable IR compiling a written request (or a mined workflow) into bots+channels+approval-rules+schedules | Nothing; tinyhivemind produces ephemeral session/turn state, not a durable "spec for a reusable agent" | This is the biggest structural gap relative to gawkbot's core pitch — tinyhivemind is deliberately scoped to *live* dispatch/deliberation mechanics (per the charter), not to authoring durable agent definitions, so this may be correctly out of scope rather than a gap to close |
| `BotTool`/`ToolRegistry`: local in-process tool execution with **no** approval gate at all | N/A — tinyhivemind defines no tool-execution port | Confirms tinyhivemind's "host owns storage/IO" boundary is stricter than gawkbot's own internal design: gawkbot itself has an ungated local-tool path that undercuts its own approval story, a trap tinyhivemind avoids by never executing side effects itself |
