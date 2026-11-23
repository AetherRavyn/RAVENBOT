# RAVENBOT — long agentic work plan

> Status: proposed, no code changes in this document.
>
> Scope: what RAVENBOT must add to run real work for hours or days without
> losing the task, the context, or the plot. This is based on a code audit of
> RAVENBOT at the current working tree and a read of Nous Research's
> [Hermes Agent](https://github.com/NousResearch/hermes-agent) architecture and
> feature docs.
>
> Rule inherited from the existing roadmap: a capability counts only when the
> repository contains the code path **and** a test.

**First slices implemented in this working tree.** An office turn is no longer
one plan. It runs up to four bounded plan→execute→synthesize cycles
(`RAVENBOT_OFFICE_CYCLES`, 1–8), asks the lead to review product completion
after each cycle, and forces a verification/continuation cycle after the first
software milestone even when every planned task says done. It stops on verified
completion, the cycle cap, two cycles with no workspace change, budget
exhaustion, a pending user queue/cancel, or a question. The planner now plans
one 3–5 task milestone per cycle, and the default tool-round budget is 40.

A second slice now persists each office graph to `office_graphs` while it runs.
Startup restores the saved graph and keeps completed node outputs. An
interrupted node whose run has a checkpoint resumes that exact run, replaying
its in-flight tool results; a node without a checkpoint falls back to its thread
with a resume note. The durable ledger now mirrors those transitions: each
planned node is a `work_item` with attempts, lease, result, and event history.

A third slice replaces the bare compaction marker with a structured,
deterministic context handoff: task/goal, current request, file paths,
tools/commands, errors, and earlier context. The tail is token-budgeted so a few
large tool results cannot consume the entire free window, and tool call/result
pairs stay intact. This costs no extra model call and works offline; later
Phase 2 slices added durable replay, micro-compaction, and an optional
auxiliary-model summary.

A fourth slice adds FTS5 over the durable transcript (migration 064) and a
read-only `session_search` tool, so an agent can recover a decision, path,
command, or error that was summarized out of its live context.

A fifth slice instruments liveness. Migration 065 adds heartbeat/phase columns
to `runs` and an append-only `work_events` ledger. The runtime touches a
heartbeat at model, tool, approval, and checkpoint boundaries; office nodes are
now bounded by *stall* (`RAVENBOT_NODE_STALL_SECS`) with a separate hard cap
(`RAVENBOT_NODE_MAX_SECS`), so a slow-but-progressing node is no longer killed
at 30 minutes. The old `RAVENBOT_NODE_TIMEOUT_SECS` still works as the stall
threshold alias.

A sixth slice adds the durable work ledger. Migration 066 creates `work_items`,
`work_links`, `work_comments`, and `work_artifacts`. Each office and
office-scoped workflow plan is seeded before execution; `WorkLedger` claims
items atomically, records attempts and leases, and mirrors done/failed/skipped
nodes. Retry, unblock, and request-changes reset the node and its descendants in
the saved graph and resume it; request-review and approve move the ledger
through a real review state. The office board opens a task-detail drawer with
the instruction, result/error, attempts, comments, artifacts, and event
timeline. Changed files from each node's run are attached as artifacts
automatically. After each graph, the ledger is projected into `Shared/STATUS.md`
between markers, preserving agent notes.

A seventh slice makes completion checkable. Migration 067 persists every quality
gate run. Before the office may accept a lead's `complete` verdict, RAVENBOT
detects the workspace's own test/build commands (`cargo test`, `npm test`,
`npm run build`/`check`, `go test`) and runs them with a timeout. A red gate
deterministically vetoes completion and its output tail becomes the next
cycle's task. The office conversation shows the gate report.

An eighth slice persists the completion contract. Migration 068 adds `goals` and
`goal_gates`. Each office gets one contract derived from its mission, with an
outcome, stop condition, reviewed-cycle budget, and up to four custom
verification commands. The contract's turn budget caps the office's cycle loop.
Custom gates run through the shell-safety policy before they can execute; a
red required gate vetoes completion exactly like a detected `cargo`/`npm` gate.
Every reviewed cycle records its verdict, reason, and turn count on the goal.

A ninth slice enforces the contract instead of only displaying it. The office
re-reads the goal before every cycle and stops when the token budget, wall-clock
budget, pause flag, block flag, or done state says to. Tokens are measured from
the runs linked to the office, and usage is monotonic. The completion review now
returns a strict `done | continue | blocked | wait` verdict; `blocked` and
`wait` stop the turn without claiming completion, and `wait` pauses the goal.
Office settings can pause and resume the goal.

A tenth slice makes context compaction durable. Migration 069 adds
`context_summaries` with the core-message floor it covers, and threads point at
the latest summary. The runtime records the full deterministic handoff and, on
a later run, replays `summary + messages after the floor` instead of reloading
and recompacting the whole transcript. The core-message mapping is tracked
alongside the model messages, so the floor is the last message actually covered
by the summary — not the last message overall. The full transcript remains
searchable.

Micro-compaction is the middle layer: before dropping any message, the runtime
replaces old tool results outside the protected twelve-row tail with a one-line
recovery pointer. If that brings the prompt under the window, no message is
dropped and no summary is made. If it is not enough, the full structured
handoff still runs on the original messages, so the summary keeps the tool
detail.

An eleventh slice adds the optional auxiliary summary. When a full handoff is
created, a bounded digest of the dropped messages and the deterministic draft
go to the bot's provider (or `RAVENBOT_CONTEXT_SUMMARY_MODEL=provider/model`).
The returned structured handoff replaces the draft in the live context and in
`context_summaries`; the call is recorded in the utility ledger. Any failure,
timeout, or empty answer keeps the deterministic handoff. `RAVENBOT_CONTEXT_SUMMARY=off`
disables the call.

A twelfth slice makes background processes real. `task_runner` now honors
`background: true`: it returns a process id immediately, captures a bounded
output tail, and supports `wait`, `status`, `processes`, and `stop`. A wait can
release on exit, a matching output pattern, or a timeout. The child is owned by
a supervisor task with `kill_on_drop`, so a shutdown does not orphan it. Host
commands no longer apply a user-wide `ulimit -u` that made ordinary forks fail
on a busy machine.

---

## 1. Verdict

RAVENBOT already has the strongest part of the problem solved: a local-first
Rust runtime, a real office planner and DAG, parallel node execution, budgets,
a kill switch, audit logs, a scheduler/webhook receiver, and a headless daemon
that can own the fleet on Linux.

It is not yet reliable for **long** agentic work. The weak points are:

1. **Office work is not durably checkpointed per task.** A restart restarts the
   whole office turn instead of resuming the unfinished node.
2. **Context compaction is a sliding window with a marker.** It drops the middle
   of the conversation and keeps no structured summary, no searchable archive,
   and no contract continuity.
3. **There is no completion contract or verification gate.** "The turn ended" is
   treated as "the work is done"; autonomy relies on a workspace fingerprint and
   a no-progress counter instead of a definition of done.
4. **Delegation is synchronous and invisible to restart recovery.** A child run
   blocks the parent, cannot be steered/stopped/continued as a background task,
   and is failed on startup rather than resumed.
5. **Coordination is mostly a file (`Shared/STATUS.md`) plus a JSON board blob.**
   There are no durable task comments, review states, artifacts, or file claims.
6. **Memory and learning are heuristic, unbounded, and mostly not injected.**
   Self-review stores 500-character chunks of assistant prose as "facts", while
   `agent_learnings` is a UI statistic that never changes agent behaviour.

The strategic recommendation is therefore:

> **Do not add more agent features first. Add the durable work ledger, the
> context spine, and the completion contract. Then add background delegation
> and the learning loop on top of them.**

That order matters. Background agents, autonomy, and self-improvement all become
unsafe or useless if a restart can duplicate work, if compaction forgets the
task, or if "done" is not checkable.

---

## 2. What Hermes does that matters here

This is not a feature checklist; these are the mechanisms that make long work
survive contact with reality.

### 2.1 Durable work is a first-class object

Hermes Kanban stores every task as a row with an owner, status
(`triage | todo | ready | running | blocked | review | done | archived`),
dependencies, comments, workspace, attempts, artifacts, and lease metadata. A
dispatcher claims ready work, spawns a worker, and reclaims stale claims. The
task outlives any one process. A human or another agent can read the whole
history and unblock/retry it.

**Contrast:** RAVENBOT's `TaskGraph` is an in-memory `HashMap`; only the board
projection is persisted as `office_board:<thread_id>` JSON in `app_settings`.
The graph's node state, attempts, thread ids, and outputs are not authoritative
after a crash.

### 2.2 Background delegation is durable and controllable

Hermes `delegate_task` starts child agents with isolated context, supports
parallel batches and output schemas, returns a handle immediately for background
work, stores completion events durably, detects stalled children by progress
(not wall clock), and lets the parent `list`, `steer`, `stop`, and `continue`
children. A child that did not finish can be resumed with an explicit
`resume_mode` (`AUTO_AT`, `AUTO_ON_BOOT`, `ON_FIX`, `REPAIR_THEN_CONTINUE`,
`HANDOFF`).

**Contrast:** RAVENBOT's `delegate` is a synchronous call inside
`execute_run_inner`; the parent blocks until the child finishes. Startup
`fail_orphaned_runs` marks delegated children failed, so there is no background
completion or continue-after-restart path.

### 2.3 Context is compressed with a structured handoff, and history stays searchable

Hermes compresses at a configurable threshold, protects the head and a
token-budgeted tail, keeps tool call/result pairs intact, and asks an auxiliary
model for a structured summary: goal, constraints, progress, decisions, files,
next steps, critical context. On later compactions it updates the previous
summary rather than starting over. It archives removed rows and exposes
`session_search` over SQLite FTS5, so summarized-away detail is recoverable.

**Contrast:** `compact_messages` in `crates/runtime/src/lib.rs` was deterministic
and lossy: it dropped the middle and inserted a bare
`[Earlier conversation compacted: N message(s) omitted...]` marker. It now writes
a structured, deterministic handoff instead: task/goal, current request, file
paths, tools/commands, errors, and earlier context. The tail is token-budgeted
and tool pairs stay intact. The handoff is now persisted with the message it
covers through (`context_summaries`, `threads.context_floor_message_id`), and a
later run replays `summary + messages after the floor`. The full transcript
remains searchable with `session_search`. Micro-compaction now shortens old
tool results before dropping messages, and an optional auxiliary model can
rewrite the handoff; any failure keeps the deterministic version.

### 2.4 Goals are judge-driven and gates are deterministic

Hermes `/goal` is a Ralph-style loop: keep a standing objective, run a cheap
judge after each turn (`done | continue | blocked | wait`), and continue in the
same session until the goal is met, paused, blocked, or out of budget. A
completion contract names outcome, verification, constraints, boundaries, and
stop conditions. Quality gates are shell commands that must exit 0 before the
judge can say done; a red gate becomes the next continuation prompt. A goal can
park on a background process until it exits or a pattern matches.

**Contrast:** RAVENBOT has office autonomy and a no-progress guard, but no
per-session goal loop, no judge, no contract, and no deterministic verification
gate. The office heartbeat can run forever or stop because the file tree did not
change, neither of which proves the deliverable is correct.

### 2.5 Memory is bounded and curated; skills are procedural memory

Hermes has small, bounded `MEMORY.md` / `USER.md` stores injected as a frozen
snapshot, a `memory` tool with `add | patch | remove`, capacity errors that force
consolidation, duplicate prevention, injection scanning, and optional write
approval. A background review uses an auxiliary model to propose memory and
skill changes; skill writes can be staged for human approval. Skills are
progressive-disclosure documents, not prompt bloat.

**Contrast:** RAVENBOT's `SelfReviewer::review_run` is heuristic, not a review:
it stores assistant text over 50 characters that contains a period, up to 500
characters, as a memory fact. `agent_learnings` is recorded but never read by
the runtime. `create_skill` / `revise_skill` exist, but no learning loop calls
them from completed work.

### 2.6 Liveness is progress-based, not a stopwatch

Hermes watches running children and workers for **progress**: API-call count,
current tool, streamed tokens, tool transitions, and last-activity timestamps. A
child that is slow but progressing is left alone; a child that is silent past
the stale threshold is interrupted, given a grace window, then finalised as
`stalled` with structured metadata. Cron uses an execution ledger with
`claimed | running | completed | failed | unknown`, and recovery never
blindly reruns an `unknown` attempt.

**Contrast:** RAVENBOT now records a run heartbeat and phase at model, tool,
approval, and checkpoint boundaries, exposes stalled runs through
`RunProgressQueries::stalled`, and bounds office nodes by stall rather than wall
clock. The append-only `work_events` ledger records start, tool, checkpoint, and
terminal transitions. What is still missing is the claim/lease state machine and
an `unknown` verdict for interrupted destructive side effects.

### 2.7 Prompt stability and caching

Hermes keeps the system prompt stable during a session and applies provider
cache breakpoints; dynamic state goes into messages. Memory is a frozen snapshot
at session start.

**Contrast:** RAVENBOT rebuilds the system prompt and tool list per run and
injects office memory/status/roster into it. That is correct for freshness but
defeats provider prefix caching and makes long sessions more expensive than they
need to be.

---

## 3. RAVENBOT today — the real system map

### 3.1 The run loop

`Runtime::execute_run_inner` in `crates/runtime/src/lib.rs`:

- loads the bot, budget, thread, messages, and working directories;
- builds a provider chain (primary + optional fallback);
- assembles skills, MCP tools, plugins, and intent-aware tools;
- prepares a model message list with a system prompt, memory context, office
  context, channel context, project roots, and tool discipline;
- compacts before the first call and after every tool round;
- calls the model with retries (`MAX_ATTEMPTS = 3`), provider fallback, and the
  shared `ProviderGovernor` concurrency/cooldown;
- executes tool calls in parallel;
- injects steering messages at tool boundaries;
- writes a checkpoint (`model_messages`, `rounds`) after each tool round;
- on tool-round exhaustion, makes one final tools-disabled report call, then a
  deterministic `tool_rounds_report` if the model still says nothing;
- persists the assistant message, usage, trace spans, and run outcome;
- runs `SelfReviewer` and applies heuristic memory updates.

Defaults: **40 model↔tool rounds** per run (`max_tool_rounds`, per bot or
`RAVENBOT_MAX_TOOL_ROUNDS`), 12-message compaction tail, 30-minute office node
timeout, 3 parallel office nodes, 3 provider requests in flight.

### 3.2 Office coordination

`office_turn` in `src-tauri/src/lib.rs`:

1. loads the room, roster, goal, policy, and workspace snapshot;
2. asks the lead to produce a JSON DAG (`plan_office` in
   `crates/runtime/src/orchestrator.rs`) or uses a deterministic fallback /
   bootstrap plan;
3. builds an in-memory `TaskGraph`;
4. seeds the persisted board JSON and emits `plan_ready`;
5. runs `GraphExecutor` with up to 3 ready nodes in parallel;
6. each node gets its own thread, is linked to the office, inherits project
   folders, and writes a `Run` row;
7. node outputs flow to dependents through the blackboard and as an "Input from
   previous task" user message; agents also coordinate through
   `Shared/STATUS.md`;
8. the lead synthesizes the final answer;
9. the board is persisted from node events by a backend task, not by the view.

Office state lives in `app_settings` keys: `office_active:<id>`,
`office_phase:<id>`, `office_board:<thread_id>`, `office_queue`, autonomy keys,
workspace fingerprint, bootstrap attempts, and budget baseline.

### 3.3 Durable pieces that already exist

- `runs` with `checkpoint`, `resume_on_start`, and startup parking of interrupted
  1:1 runs (`resume_interrupted_runs`).
- `office_queue` for user messages sent while a turn runs, with pause/cancel and
  restart survival.
- `workflow_runs` and `workflow_schedules`; startup re-queues interrupted
  workflows.
- `channel_tasks` reset to pending on startup.
- `office_memories` with embeddings and provenance.
- `memory_facts` with embeddings, importance, access counts, and provenance.
- The headless daemon (`src-tauri/src/daemon.rs`, `docs/SERVICE.md`) owns the
  scheduler, office loops, workflow triggers, audit maintenance, and remote
  listener on Linux, with a fleet lock so desktop and daemon do not both write.

### 3.4 Coordination primitives that exist but are not durable enough

- `delegate` tool: permission-checked, office-aware, child thread, transcript
  snapshot, depth cap 3, synchronous.
- steering: a message can join a run that is inside a tool phase and is injected
  at the next model call.
- pause/cancel: cooperative flags checked between steps.
- approvals and questions: persisted rows; office nodes run unattended under the
  per-run Review policy; hard safety blocks destructive calls.
- `Shared/STATUS.md`: a seeded, agent-owned file; `OFFICE.md` and `POLICY.md`
  are the office charter.
- `office_board`: a JSON projection for the UI, not the execution source of
  truth.

### 3.5 Current shape

```text
User message
   │
   ▼
office_turn (src-tauri/src/lib.rs)
   │
   ├─ lead planner (crates/runtime/src/orchestrator.rs)
   │      └─ JSON plan or fallback/bootstrap plan
   │
   ├─ TaskGraph (in memory, crates/runtime/src/graph.rs)
   │      └─ nodes, edges, blackboard, loop groups
   │
   ├─ GraphExecutor (crates/runtime/src/executor.rs)
   │      ├─ node thread → Runtime::execute_run_inner
   │      │      ├─ model call + retry/fallback
   │      │      ├─ tool calls (parallel)
   │      │      ├─ checkpoint (model_messages, rounds)
   │      │      └─ sliding-window compaction
   │      └─ node events → office_board JSON (app_settings)
   │
   └─ lead synthesis → room transcript
```

The turn is the durability boundary: if the process stops, the graph and its
node outputs are gone, even though the board and the transcript survive.

### 3.6 What this means

RAVENBOT has a **good execution engine** and a **good office metaphor**, but the
unit of durability is still the *turn*, not the *task*. Hermes makes the task the
unit. That single difference explains most of the gaps below.

---

## 4. Gap matrix

| Capability | Hermes | RAVENBOT today | Risk | Priority |
|---|---|---|---|---|
| Durable task record | Kanban rows + dispatcher + claims | `work_items` ledger + graph snapshot + atomic claims | Multi-worker dispatcher still missing | **P0** |
| Per-task resume | Worker/lease/retry/review | Whole office turn restarts | Duplicate side effects, lost node outputs | **P0** |
| Context compaction | Structured summary + archive + search | auxiliary summary + durable replay + micro-compaction + FTS | Core path complete | **Done** |
| Session search | FTS5 `session_search` | FTS5 + read-only `session_search` tool | Summarized detail is recoverable | **Done** |
| completion contract | `/goal` contract + gates | persisted contract + strict verdict + custom/detected gates + token/time budgets + pause | Wait-on-process and a non-office goal worker still missing | **P0** |
| Background delegation | Async, durable, steer/stop/continue | Synchronous, restart-fails child | Parent blocked; no long fan-out | **P1** |
| Task comments/handoffs | Durable board comments | `work_comments` + `work_artifacts` + events + task drawer | Shared file claims still missing | **P1** |
| Review/changes loop | `review` / `request_changes` | review state + change request reruns the graph branch | Per-bot review assignment still missing | **P1** |
| File/worktree isolation | Per-task worktrees/workspaces | Shared workspace | Concurrent edits conflict | **P1** |
| Memory curation | Bounded, model-reviewed, approval | Heuristic transcript chunks | Memory pollution, stale advice | **P1** |
| Skill learning | Background review + skill writes | Manual create/revise only | No compounding improvement | **P2** |
| Progress watchdog | Heartbeat/stall detection | Heartbeat + phase + stall watchdog + hard cap | Slow work survives; wedged work stops | **Done** |
| Per-task cost/budget | Per-task/worker budgets | Bot + office monthly only | One task can burn the office budget | **P2** |
| Prompt caching | Cache breakpoints + stable prefix | Prompt rebuilt per run | Long work is expensive and slow | **P2** |
| Project context | AGENTS.md/CLAUDE.md chain | OFFICE.md/POLICY.md only | Coding work misses repo conventions | **P2** |
| Failure incidents | Dedupe, streaks, doctor | Logs + no-progress pause | Alert fatigue, silent repeated failure | **P2** |
| Execution ledger | claimed/running/terminal/unknown | Run rows + `work_events` + startup reconciliation | Claim/lease and `unknown` verdict still missing | **P0/P1** |

---

## 5. Target architecture

Four concepts, added incrementally.

```text
User goal / office request
   │
   ▼
Goal + completion contract
   │
   ▼
WorkLedger (SQLite: work_items, links, comments, artifacts, events)
   │
   ▼
Dispatcher / lease
   │
   ├─ ready work item ──► Run + checkpoint ──► tools/artifacts
   │                              │
   │                              └─ heartbeat / progress events
   │
   ├─ blocked / review / unknown ──► human or parent action
   │
   ▼
Context spine (summary + archived transcript + session_search)
   │
   ▼
Verification gates ──► Judge (done | continue | blocked | wait)
   │
   └─ continue ──► next work item or continuation turn
```

### 5.1 Work item — the durable unit

A `work_item` is one accountable piece of work. It can be an office graph node, a
workflow node, a delegated child, a channel task, or a 1:1 goal step.

Minimum fields:

```text
id, kind, parent_id, office_id, thread_id, run_id
title, instruction, contract_json
status: pending | ready | running | blocked | review | done | failed | unknown | archived
owner_bot_id, reviewer_bot_id
attempts, max_attempts
priority, created_at, started_at, updated_at, completed_at
lease_owner, lease_expires_at, heartbeat_at
workspace, result, error, metadata_json
```

Supporting rows:

- `work_links(parent_id, child_id)` for dependencies;
- `work_comments(id, work_item_id, author, body, created_at)`;
- `work_artifacts(id, work_item_id, path, sha256, mime, size, created_at)`;
- `work_events(id, work_item_id, kind, payload_json, created_at)` — append-only
  claim/start/heartbeat/tool/complete/fail/unknown/review events.

The graph becomes a **projection** of these rows. The current `TaskGraph` can
remain the in-memory scheduler, but every transition must be written through the
ledger.

### 5.2 Execution lease — who is allowed to run it

A worker claims a ready item atomically and gets a lease. A heartbeat extends it.
A crashed worker's lease expires and the item becomes reclaimable. If the item
was running when the owner disappeared, mark it `unknown` until the side-effect
question is answered; do not auto-rerun a destructive task.

This is the RAVENBOT equivalent of Hermes' Kanban claim/lease and cron execution
ledger, without importing the Python dispatcher.

### 5.3 Context spine — summary, archive, search

For each thread:

- `context_summaries(id, thread_id, through_message_id, summary, token_estimate,
  model, created_at)`;
- `threads.context_summary_id`, `threads.context_floor_message_id`;
- all original messages stay in `messages` and remain searchable;
- runtime loads `summary + messages after floor`, not the whole transcript;
- `session_search` and `transcript_read` tools let an agent recover detail.

Compaction becomes a real handoff, not a deletion:

```text
## Goal
## Completion contract
## Constraints and boundaries
## Progress (done / in progress)
## Decisions and why
## Files and artifacts
## Commands and verification results
## Open tasks and owners
## Blockers and exact next step
## Critical identifiers (paths, SHAs, error strings, URLs)
```

### 5.4 Completion contract — when is it done

A contract attached to a goal, office task, or workflow run:

```text
objective
outcome
verification: [shell command | file exists | test suite | artifact check]
constraints
boundaries
stop_when
max_turns, max_tokens, max_wall_seconds
```

The judge sees the contract, the last answer, the task ledger, and the gate
results. It returns `done | continue | blocked | wait`. A red gate blocks
`done` deterministically; a broken judge fails open to `continue` and the turn
budget is the backstop.

### 5.5 Agent operating protocol

This is the behavioural half of the architecture. Infrastructure without a
protocol just gives agents more ways to duplicate work.

Every agent turn receives a compact **work packet**:

- office charter: goal, policy, roster, workspace contract;
- the assigned work item: id, title, instruction, acceptance criteria, owner;
- dependency handoffs: outputs, artifact paths, changed files, verification;
- recent ledger comments and events for that item;
- relevant shared memory, capped by tokens;
- project context (`AGENTS.md`/`CLAUDE.md`/`.cursorrules` when trusted);
- workspace path, file claims or worktree, and the budget remaining.

**Worker protocol**

1. Read the work item, its comments, and its dependency handoffs first.
2. Claim the files/worktree before editing; a conflicting claim is an explicit
   block, not a race.
3. Work in the assigned workspace; never wander outside it.
4. Heartbeat on long operations and record meaningful progress, not every
   token.
5. Persist artifacts in the workspace and attach their paths/hashes to the work
   item.
6. Run the verification the task names; record the exact command and result.
7. Finish with the handoff contract: what changed, how it was verified, what
   remains, residual risk, and any blocker.
8. End with exactly one terminal action: `complete`, `request_review`, or
   `block`. Prose alone is not a completion.

**Lead protocol**

1. Plan and replan from the ledger and the workspace, not from a private memory
   of the conversation.
2. Assign distinct work; never give two agents the same file or command.
3. Keep specialist work with specialists; do not do the implementation itself.
4. Review the contract, the evidence, and the open risks; synthesize the final
   answer from the ledger, not from a paraphrase.
5. If the plan is blocked, write the blocker and the exact next step; do not
   repeat the same plan.

**Reviewer protocol**

1. Inspect the artifact, diff, test output, and acceptance criteria directly.
2. Approve only when the verification passes and the residual risk is stated.
3. On rejection, write a specific change request tied to a file/line/test, and
   route the item back to its implementer.
4. Cap review cycles; a repeated rejection escalates to the lead or a human.

**System protocol**

1. Claim and lease ready work; one owner at a time.
2. Record every transition in the event ledger.
3. Enforce budgets, gates, and safety before the next step.
4. Detect stalls by progress, not by a stopwatch.
5. Mark interrupted work `unknown` when side effects cannot be proven; never
   blindly rerun a destructive task.
6. Resume unfinished work from its own checkpoint, not by replaying the whole
   office turn.

This protocol is what makes “agents understand the project” concrete: the
ledger is the shared source of truth, the work packet is the brief, and the
handoff contract is the durable message. `Shared/STATUS.md` remains the
human-readable projection for the office and the user.

### 5.6 Security and trust invariants

- Work-item comments, dependency outputs, artifacts, and memory are **data, not
  instructions**. The work packet labels their origin; the runtime never
  promotes text from them into the system prompt as a directive.
- Gates are configured by the owner, never by the agent being judged. An agent
  cannot add, remove, or waive its own gate, and cannot approve its own review.
- A delegated child inherits the parent's toolsets and permissions; it cannot
  escalate. Delegation permission is checked at spawn, not only at the first
  tool call.
- A worker may update only the work item it has claimed. Completing or
  reviewing another owner's item is refused.
- Memory and skill writes use the same discipline as tools: scan, propose or
  approve, and never let an autonomous run silently rewrite the agent's
  long-term behaviour.
- An interrupted item whose side effects cannot be proven is `unknown`, not
  `ready`. Retrying it is an explicit human or parent decision.

---

## 6. Phased plan

Estimates are relative: **S** = 1–3 days, **M** = 1–2 weeks, **L** = 2–4 weeks.
Each phase ends with tests and a usable surface. Do not start a later phase
before the earlier phase's exit criteria are green.

### Phase 0 — Instrumentation and invariants (S) — landed

**Why:** every later phase needs a trustworthy event stream and heartbeat.

**Changes**

- Migration `065_run_progress.sql`: append-only `work_events` with
  `work_item_id`, `run_id`, `thread_id`, `kind`, `payload_json`, `created_at`,
  plus `heartbeat_at`, `phase`, `last_tool`, `last_error`, `lease_owner`, and
  `lease_expires_at` on `runs`.
- `RunProgressQueries::touch` is called at model request start/finish, each
  tool start/finish, checkpoint writes, approval waits, and finalization. Long
  model/tool/approval waits hold a lease that beats every 20 seconds.
- Existing `Status` events remain the live UI signal; terminal transitions are
  also appended to `work_events` (`run_started` … `run_completed` /
  `run_failed` / `run_cancelled`).
- Startup trims the ledger to 30 days or 100k rows.
- Office nodes are bounded by stall (`RAVENBOT_NODE_STALL_SECS`, default 1800)
  and a hard cap (`RAVENBOT_NODE_MAX_SECS`, default 8× the stall threshold).
  `RAVENBOT_NODE_TIMEOUT_SECS` remains a stall-threshold alias.

**Exit criteria**

- A synthetic run that sleeps between tools emits heartbeats and is not
  considered stalled. **Covered by DB + boundary tests.**
- A run that stops touching progress for the configured threshold is detectable
  in a test. **Covered by `stalled` and `progress_stalled` tests.**
- `cargo test --workspace` green; no UI change required.

### Phase 1A — Persist and resume the office graph (M) — landed

**Why:** this is the highest-value fix with the smallest conceptual change.

**Changes**

- Migration `063_office_graphs.sql`:
  `office_graphs(id, chatroom_id, thread_id, cycle, request, visible, goal,
  labels, graph, status, created_at, updated_at)`.
- Status: graph snapshots are written while a graph runs and marked terminal at
  the end. Startup restores the saved graph, keeps completed outputs, and
  reschedules interrupted nodes. A node with a checkpointed `Run` resumes that
  exact run (same run id, token/cost totals, and in-flight tool results); a node
  without a checkpoint falls back to its thread with a resume note. **Still
  next:** the durable work ledger/comments/review in Phase 1B.
- Serialize the whole `TaskGraph` (nodes, edges, blackboard, loop groups,
  attempts, thread ids, outputs) after every transition. Do it through a
  debounced writer or a single mpsc task, like the existing board writer.
- On `resume_interrupted_offices`, load the graph instead of re-planning:
  - `Done` nodes stay done and keep their output;
  - `Running` nodes are looked up in `runs`; if a checkpoint exists, resume the
    run from it; if not, mark `unknown`;
  - `Pending`/`Ready` nodes are scheduled normally;
  - `Failed` nodes follow their retry policy;
  - never insert the user message again;
  - never re-run a `Done` node.
- The executor's `mark_running` / `mark_done` / `mark_failed` / `retry_failed_nodes`
  paths write the snapshot.
- Add `office_graph_id` to the office turn's resume key so two offices cannot
  collide.

**Exit criteria**

- Test: build a 3-node graph; complete node 1; kill the process before node 2;
  restart; node 1 is not re-run, node 2 resumes, node 3 runs once, and the
  user message appears exactly once.
- Test: a `Running` node with no checkpoint becomes `unknown`, not silently
  retried.
- The board projection after resume matches the graph snapshot.

### Phase 1B — Work ledger, comments, artifacts, review (L) — core landed

**Why:** the graph snapshot fixes restart; the ledger fixes coordination.

**Changes**

- Status: migration 066, `WorkItemQueries`, the runtime `WorkLedger`, office and
  office-scoped workflow seeding, graph claim/sync/reconcile, review/block/
  retry transitions, graph-branch reruns, the task detail drawer, and the
  marker-based `Shared/STATUS.md` projection are implemented. Still next:
  bounded replanning, shared file claims/worktrees, and per-bot review
  assignment.
- Migration `066_work_items.sql`:
  `work_items`, `work_links`, `work_comments`, `work_artifacts`. The event
  ledger already exists from Phase 0 (migration 065).
- `WorkLedger` in `crates/db` (queries) and `crates/runtime` (claim/lease).
- Office planner writes planned tasks as `work_items`; the graph nodes reference
  `work_item_id`.
- `GraphExecutor` claims each node through the ledger before running it and
  records terminal state there.
- Dependencies, retries, and review states are ledger transitions, not just
  in-memory graph fields. Office plan nodes default to two attempts for
  transient provider/tool failures; configuration/permanent errors and
  destructive side effects do not auto-retry.
- `Shared/STATUS.md` becomes a generated projection plus an agent-notes section
  with a clear marker. Agents may append, but the ledger is authoritative.
- Replanning: when a node fails or blocks, the lead can read the ledger and
  create a replacement task or a revised dependency edge. Replans are bounded
  (default 3 per turn), require a stated reason, preserve completed work, and
  cannot recreate a task that is already `done`. A repeated replan with no new
  artifact is a blocker, not another plan.
- UI: task detail drawer with status, owner, dependencies, comments, artifacts,
  attempts, last heartbeat, and `Retry` / `Unblock` / `Request review` actions.
- Add commands: `list_work_items`, `get_work_item`, `add_work_comment`,
  `list_work_events`, `request_work_review`, `request_work_changes`,
  `block_work_item`, `unblock_work_item`, `retry_work_item`, `replan_work`.

**Exit criteria**

- Two workers cannot claim the same ready item.
- A worker with an expired lease is reclaimed; a completed item is never
  reclaimed.
- A review rejection routes the item back to the implementer with the comment
  preserved.
- Artifacts survive scratch cleanup and are listed on the task.
- `Shared/STATUS.md` can be regenerated from the ledger without losing agent
  notes.

### Phase 2 — Context spine (L) — landed

**Why:** long work dies when the transcript is compacted into a generic marker.

**Changes**

- Status: compaction writes a structured deterministic handoff (task/goal,
  current request, files, tools/commands, errors, earlier context) and keeps a
  token-budgeted tail without splitting tool pairs. Migration 069 persists the
  handoff with the last core message it covers; a later run loads
  `summary + messages after the floor`, and the summary message is replayed
  before the surviving transcript. FTS5 + `session_search` landed in migration
  064. Micro-compaction shortens old tool results before dropping messages. An
  optional auxiliary-model summary rewrites the deterministic handoff, with a
  deterministic fallback and utility-cost accounting. **Remaining Phase 2
  polish:** `transcript_read`, prompt stability/cache breakpoints, and a
  per-goal summary model setting.
- Migration `069_context_summaries.sql` plus `threads.context_summary_id` and
  `threads.context_floor_message_id`.
- Add `MessageQueries::list_context_window(thread_id, floor)` used by the runtime;
  the UI keeps `list_by_thread` for the full transcript.
- Implement `ContextCompactor`:
  - trigger at `min(model_window * threshold, provider error pressure)`;
    default threshold 0.70–0.75; keep the existing manual half-window request;
  - preserve system, the first user turn, the active contract, the last N
    messages (default 12), the last real user message, and complete tool
    call/result groups;
  - never summarize an unanswered tool round;
  - summarize the middle with a structured prompt; on later passes, update the
    previous summary;
  - if the summary call fails, write a deterministic summary from tool traces,
    artifacts, and the ledger, and keep the tail; never silently drop without a
    marker;
  - archive nothing destructively: original rows stay in `messages`; the floor
    only changes what the runtime replays;
  - emit a context event with `summary_tokens`, `tail_tokens`, and
    `archived_through`.
- Add FTS5:
  - FTS5 landed as migration `064_messages_fts.sql` with INSERT/UPDATE/DELETE triggers and a
    backfill;
  - `SearchQueries::search_fts(query, filters, limit)` with ranked snippets;
  - keep the existing LIKE search for compatibility.
- Add agent tools:
  - `session_search(query, thread_id?, limit?)` — returns thread title, date,
    role, and a snippet with `>>>match<<<`;
  - `transcript_read(thread_id, before_id?, after_id?, limit?)` — bounded
    context around a hit.
- Micro-compaction: outside the protected tail, replace old tool results with a
  one-line stub plus a recovery pointer to the archived row/file.
- Prompt stability: put dynamic office/roster/status text in a user-role context
  message, keep the system prompt stable within a run, and add provider cache
  breakpoints where supported.

**Exit criteria**

- A 200-message synthetic thread compacts and the next model call still sees:
  the goal, contract, artifact paths, open tasks, blockers, and the last user
  request.
- No tool result is orphaned from its call.
- A summarized-away fact is findable via `session_search`.
- The context meter still reports accurate buckets and the summary delta.
- A test proves the old marker-only path is no longer used for normal
  compaction.

### Phase 3 — Goals, gates, and liveness (M) — contract and gates landed

**Why:** “done” must be checkable, and a long task must not be killed for taking
long.

**Changes**

- Status: migration 067 adds `office_gate_runs`; migration 068 adds `goals` and
  `goal_gates`. The office completion review runs the contract's custom gates
  through the shell-safety policy, then detected `cargo test` / `npm test` /
  `npm run build|check` / `go test` gates, before accepting `complete`. A red
  required gate becomes the next continuation task and is posted to the room.
  The contract's turn, token, and wall-clock budgets stop the cycle loop;
  pause/resume is a persisted goal state; and each cycle records a strict
  `done | continue | blocked | wait` verdict with its reason and turn count.
  Wait-on-process now has a real background registry and `task_runner`
  wait/status/list/stop actions; judge-level wait (parking an office cycle on a
  process id) remains. A goal worker for non-office threads also remains.
- Migration `068_goals.sql`: `goals` and `goal_gates`, scoped to a thread, office,
  or work item. Store the contract JSON, status, turns used, token budget, and
  last verdict.
- `GoalContract` type in `crates/core`; `GoalQueries` in `crates/db`.
- `GoalJudge` auxiliary call:
  - prompt includes objective, contract, last answer, open work items, gate
    results, and the last few tool outcomes;
  - strict JSON verdict;
  - cheap model by default; optional per-goal model;
  - fail-open to `continue`.
- Goal worker:
  - after a run completes, if a goal is active and not done, enqueue a synthetic
    continuation user message and start the next run when the thread is idle;
  - a real user message always preempts;
  - pause/resume/clear; turn/token/time budgets; no overlap with office autonomy.
- Quality gates:
  - shell command(s) with cwd = workspace, timeout, output tail, and the same
    safety policy as `shell_exec`;
  - all gates must exit 0 before the judge can return `done`;
  - failed gate output becomes the next continuation prompt;
  - bounded retries (default 3) and a per-gate timeout;
  - persist gate results for the UI and the judge.
- Wait-on-process:
  - a small background-process registry (pid/session id, command, started,
    last output, exit code);
  - judge may return `wait` with a pid/session/deadline;
  - release on exit, matching output pattern, or deadline.
- Heartbeat/loop (optional, same phase):
  - one idle-only recurring instruction per session, persisted;
  - coalesce missed ticks;
  - cache-safe: inject as a normal user message;
  - never overlap a running turn.

**Exit criteria**

- A goal with a red gate cannot be marked done; the gate output drives the next
  turn; after the gate passes, the judge can complete it.
- A goal with a broken judge keeps going until the turn budget, then pauses with
  a reason.
- A user message preempts a continuation.
- A goal parked on a background process does not burn turns while waiting.
- Office autonomy uses the goal contract to stop on verified completion instead
  of relying only on the workspace fingerprint.

### Phase 4 — Background delegation and coordination (L)

**Why:** the office needs fan-out that outlives the parent turn and survives a
restart.

**Changes**

- Migration `070_delegations.sql`: child delegation rows with parent work item,
  child thread/run, route, status, attempts, heartbeat, `resume_mode`, result,
  error, and artifact list.
- Extend the `delegate` schema:
  - `background?: bool`
  - `tasks?: [{ goal, context, output_schema?, images? }]`
  - per-task `max_iterations`/`timeout` where allowed.
- Runtime:
  - background delegation inserts the child row and returns a handle
    immediately;
  - child completion writes a durable completion event and a synthetic message
    into the parent thread;
  - if the parent is idle, start a parent turn; if busy, leave it queued;
  - a completion is delivered once (claim/ack in DB), and is recoverable after
    restart;
  - `delegate_control` actions: `list`, `steer`, `stop`, `continue`;
  - child stall detection uses the Phase 0 heartbeat; slow-but-progressing
    children are not killed;
  - `continue` rebuilds the child from its saved thread and route, and creates a
    new run.
- Workspace isolation:
  - coding children get a `git worktree` (or a task subdirectory for non-git
    workspaces);
  - the parent records the worktree/branch and reviews/merges on completion;
  - file claims (`work_claims`) for cooperative tools in shared workspaces.
- Coordination:
  - office handoffs become work-item comments/links, not only `Shared/STATUS.md`;
  - review state machine: `request_review` → `review` → `request_changes` back
    to the implementer or `complete`;
  - cap review cycles (e.g., 3) to avoid ping-pong.

**Exit criteria**

- A parent turn can finish while two children run; both completions arrive as
  separate durable messages/turns.
- Killing the app after a child completes but before delivery still delivers the
  completion once on restart.
- `steer` changes a running child at its next boundary; `stop` preserves partial
  output; `continue` works after restart.
- Two coding children cannot edit the same worktree; a conflicting file claim is
  refused with an actionable message.
- A review rejection routes back and preserves the reviewer comment.

### Phase 5 — Memory and skill curation (M)

**Why:** long work should compound, not pollute.

**Changes**

- Replace `SelfReviewer`'s heuristic with a real review:
  - a cheap auxiliary model, not the main chat model, by default;
  - input is a digest (recent turns verbatim + older summary), the run outcome,
    artifacts, and the task contract;
  - output is a small JSON list of durable facts and skill proposals, each with
    evidence and confidence;
  - strict criteria: durable, non-obvious, reusable, not already in memory;
  - no transcript dumps; max ~5 candidates per run.
- Memory schema:
  - add `scope` (`agent | office | user`), `status`
    (`proposed | active | archived`), `confidence`, `last_verified`,
    `expires_at`, `token_count`;
  - enforce a per-agent token budget (e.g., 1.5k–2k tokens) and a per-office
    budget; capacity errors force consolidation;
  - retrieval returns top-k within the budget, with recency/importance/usage;
  - `memory` tool: `add | patch | remove | forget`; similarity dedupe;
  - optional write approval; autonomous writes default to `proposed` unless the
    owner opts in.
- Office memory:
  - cap count and total tokens; consolidate related facts;
  - expire low-importance, unaccessed facts;
  - keep provenance (thread/run).
- Skills:
  - post-task `skill_propose` when a non-trivial procedure succeeded or a
    correction was learned;
  - `skill_manage` create/patch/write references; security scan; approval gate;
    lock;
  - progressive disclosure: metadata in the prompt, body on demand;
  - skill usage stats and a curator pass for stale/duplicate/broken skills.
- Project context:
  - load trusted `AGENTS.md`, `CLAUDE.md`, `.cursorrules`, and
    `.hermes/skills` equivalents from the workspace, bounded and with a trust
    decision; inject into the prompt for coding tasks.

**Exit criteria**

- Memory stops growing linearly with assistant messages.
- A repeated stale solution is not retrieved forever; expiry/consolidation
  removes it.
- A proposed skill is not active until approved; an unsafe skill is blocked.
- A repo with `AGENTS.md` gets its conventions in the coding prompt without
  exceeding the context budget.
- The intelligence/learning UI reflects real, injected learnings rather than
  dead counters.

### Phase 6 — Operations and always-on hardening (M)

**Why:** long work must be observable and recoverable, not just durable.

**Changes**

- Metrics per work item: tokens, cost, wall time, attempts, retries, stalls,
  queue wait, success rate.
- Failure incidents: dedupe by normalized error signature; alert once; remind on
  a cooldown; acknowledge/resolve; failure streak; pause after repeated failures.
- Execution ledger: `claimed | running | completed | failed | unknown`; recovery
  never auto-reruns `unknown`; human can retry with a reason.
- Per-task budgets: token/cost/wall-time caps, warning at 80%, pause with a
  reason; cost attributed to the work item and office.
- State DB maintenance: WAL checkpoints, backup/restore, migration checks,
  orphan cleanup, bounded event retention.
- Daemon/desktop ownership: keep the fleet lock; ensure the daemon resumes the
  same work ledger; make the UI a client/view of the ledger rather than the
  owner of background work.
- Dashboard: work queue depth, running/stalled/blocked counts, last heartbeat,
  failure incidents, goal status, gate results.

**Exit criteria**

- A stalled work item is detected, paused, and visible; it is not silently
  retried forever.
- A repeated failure produces one incident, not one alert per cycle.
- Cost and token totals per work item are accurate within the existing estimate.
- Killing the daemon mid-work and restarting preserves the ledger and resumes
  only what is safe.
- The UI can explain, for any work item, what happened, who owns it, what
  artifact it produced, and what is blocking it.

---

## 7. Recommended first sprint (five concrete tasks)

Do these in order; they are deliberately small and unlock the rest.

1. **Add `work_events` + run heartbeat fields** (Phase 0). **Done:** migration
   065, runtime heartbeats at model/tool/approval/checkpoint boundaries,
   stall-detection query and watchdog, retention, and tests.
2. **Persist the office `TaskGraph` and resume unfinished nodes** (Phase 1A).
   **Done:** graph snapshots, graph-level resume, and exact checkpointed-run
   resume for interrupted nodes.
3. **Add `context_summaries` and a structured compactor with a deterministic
   fallback** (Phase 2, first half). **Done:** structured handoff, durable
   summary rows, core-message floor, `summary + messages after the floor`
   replay, micro-compaction, and an optional auxiliary-model summary.
4. **Add FTS5 + `session_search`** (Phase 2, second half). **Done:** migration
   064, ranked snippets, thread scoping, read-only runtime tool.
5. **Add `GoalContract` + a quality-gate runner to office autonomy** (Phase 3,
   first slice). **Gates done:** detected workspace test/build commands veto a
   model's `complete`; results are persisted and fed into the next cycle. A
   persisted goal contract and custom per-goal gates remain.

Why this order: 1 makes 2 observable; 2 makes offices restart-safe; 3 and 4
make context loss survivable; 5 gives “done” a meaning. Background delegation
(Phase 4) should wait until these are in place, otherwise it multiplies the
failure modes.

---

## 8. What not to copy

- **Do not import Hermes' Python gateway and 25k-test surface.** RAVENBOT's
  Rust runtime, local-first model breadth, audit trail, and headless daemon are
  the differentiators.
- **Do not make the office an unbounded swarm.** Keep the 3-node parallel cap,
  the provider governor, the delegation depth cap, and the budget ceiling.
  Add background work behind the same limits.
- **Do not let the judge be the only definition of done.** Deterministic gates
  first; the judge handles ambiguity.
- **Do not auto-write memory or skills from autonomous runs without a review
  path.** Pollution compounds faster than capability.
- **Do not use a hard wall-clock timeout as the primary liveness check.** Use
  progress heartbeats; keep a generous hard cap only as a backstop.
- **Do not replace `Shared/STATUS.md` with a database only.** Humans and agents
  both read it. Make it a projection of the ledger, with an explicit agent-notes
  section.

---

## 9. Success metrics

Track these before and after:

- **Restart safety:** percentage of interrupted office/workflow tasks that
  resume without duplicating a completed node or user message.
- **Context survival:** percentage of compactions after which the next turn
  still references the correct goal, artifact paths, and open tasks.
- **Completion quality:** percentage of “done” tasks with a passing
  verification gate or explicit human acceptance.
- **Coordination:** duplicate-work rate (same file/command repeated by two
  agents), review rejection cycle time, blocked-task age.
- **Liveness:** median time to detect a stalled task; false-stall rate on slow
  but progressing tasks.
- **Cost:** tokens/cost per completed work item; percentage of runs that hit the
  tool-round limit.
- **Learning:** percentage of repeated tasks that reuse a skill or memory
  instead of rediscovering the procedure.

---

## 10. References

Hermes Agent (read for this plan):

- <https://github.com/NousResearch/hermes-agent>
- <https://hermes-agent.nousresearch.com/docs/developer-guide/architecture>
- <https://hermes-agent.nousresearch.com/docs/developer-guide/agent-loop>
- <https://hermes-agent.nousresearch.com/docs/developer-guide/context-compression-and-caching>
- <https://hermes-agent.nousresearch.com/docs/developer-guide/session-storage>
- <https://hermes-agent.nousresearch.com/docs/user-guide/features/delegation>
- <https://hermes-agent.nousresearch.com/docs/user-guide/features/kanban>
- <https://hermes-agent.nousresearch.com/docs/user-guide/features/goals>
- <https://hermes-agent.nousresearch.com/docs/user-guide/features/loops>
- <https://hermes-agent.nousresearch.com/docs/user-guide/features/heartbeat>
- <https://hermes-agent.nousresearch.com/docs/user-guide/features/memory>
- <https://hermes-agent.nousresearch.com/docs/user-guide/features/skills>
- <https://hermes-agent.nousresearch.com/docs/user-guide/features/cron>

RAVENBOT files that define the current behaviour:

- `crates/runtime/src/lib.rs` — run loop, retries, compaction, delegation,
  steering, self-review, context events.
- `crates/runtime/src/executor.rs` — office graph execution and node threads.
- `crates/runtime/src/graph.rs` — in-memory task graph, blackboard, loop groups.
- `crates/runtime/src/orchestrator.rs` — planner, fallback plan, bootstrap plan.
- `src-tauri/src/lib.rs` — office turn, board persistence, autonomy, queue,
  startup reconciliation, resume.
- `src-tauri/src/daemon.rs` — headless fleet owner.
- `crates/db/src/queries.rs` — runs, messages, search, workflows, approvals.
- `crates/memory/src/self_review.rs` — heuristic review to replace.
- `crates/memory/src/office_memory.rs` — office memory and retrieval.
- `crates/skills/src/builtin/delegate.rs` and `local_skills.rs` — delegation
  schema and manual skill creation.
- `docs/agent-harness.md` — the existing long-run office contract.
- `docs/roadmap-p0-p3.md` — existing queue/context/completed work.
