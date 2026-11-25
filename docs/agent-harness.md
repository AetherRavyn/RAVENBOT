# RAVENBOT agent harness

This is the operating contract for a RAVENBOT run: how a model gets tools, how
the loop stops, and what happens when the model does not stop on its own. It is
informed by the public harness designs from Anthropic, OpenCode, DeepSeek
Harness, and Grok Build (sources at the end).

## The loop

```text
system prompt + project roots + tool discipline
        │
        ▼
  model call ──► tool calls? ──► execute in parallel ──► tool results
        │                │
        │                └──────────────► next round
        └── no tool calls ──► final answer
```

The loop is bounded in three places:

1. **Two rounds left** — a user-role notice tells the model to stop exploring,
   finish the most important incomplete step, and report what remains.
2. **Tool budget exhausted** — the runtime makes one final call with **tools
   disabled**, asking for a factual report: completed work, artifact paths,
   verification, remaining work, blockers. The answer gets a visible note:
   `Tool-round limit reached (N). This is a progress report; some work may remain.`
3. **No answer even then** — a deterministic report is built from the tool
   traces (`tool_rounds_report`). A run never ends with an empty assistant
   message and a pile of useful tool calls.

The final answer is persisted before the run is marked complete. The executor
also refuses an empty result, so a downstream office node cannot run on a file
that was never produced.

## Tool discipline

The baseline prompt is intentionally short; the discipline block is separate
and explicit:

- **Discover with `file_tree`, search with `code_search`, read with
  `file_read`.** Shell `find`, `ls -R`, and `grep -r` are not discovery tools.
- **`shell_exec` is for builds, tests, git, and processes.** Process commands get
  a timeout; sleep/poll loops are discouraged.
- **Read before overwrite.** Prefer `code_edit` for targeted changes. Treat
  unfamiliar files as someone else's work and inspect before deleting.
- **Batch independent work.** The runtime executes all tool calls in one model
  round in parallel; serial reading one file at a time wastes the budget.
- **Check `success`/`exit_code`.** A failed call changes the approach; it is not
  repeated. A sandbox denial is policy, not a command bug.
- **Missing file, not a missing machine.** Inspect the workspace tree once,
  adapt, or write the blocker to `Shared/STATUS.md`. Whole-filesystem discovery
  is refused by the shell guard with a pointer to `file_tree`.
- **Finish with evidence.** Name the artifact paths and the tests/commands that
  produced them. Never claim a test that was not run.

The same rules are carried by the tool descriptions themselves, so a model that
only reads the schema still gets the intended usage.

## Run liveness

A run records `heartbeat_at` and a `phase` at model, tool, approval, and
checkpoint boundaries. Long model rounds, tool calls, and approval waits hold a
lease that refreshes the heartbeat every 20 seconds, so a quiet provider call
does not look like a dead run. Terminal transitions append to `work_events`
(`run_started`, then `run_completed` / `run_failed` / `run_cancelled`).

Office nodes are bounded by **stall**, not wall clock:

| Setting | Default | Meaning |
|---|---|---|
| `RAVENBOT_NODE_STALL_SECS` | 1800 | Stop a node whose heartbeat is older than this. |
| `RAVENBOT_NODE_MAX_SECS` | 8 × stall | Final hard cap even if the heartbeat keeps moving. |
| `RAVENBOT_NODE_TIMEOUT_SECS` | — | Backward-compatible alias for the stall threshold. |

A slow node that is still streaming, running tools, or waiting on a person is
left alone; a wedged node stops and reports its phase and last tool.

## Quality gates

A model's `complete` verdict is a claim until the workspace's own commands
agree. Each office has one persisted completion contract (`goals` +
`goal_gates`) derived from its mission. The contract names the outcome, the
stop condition, the reviewed-cycle budget, and up to four custom verification
commands. Custom commands are checked by the shell-safety policy when saved and
again when run; a destructive command is refused.

Before the office ends a reviewed turn, RAVENBOT runs the contract's custom
gates, then detects known project manifests and runs up to two additional
gates:

| Manifest | Gate |
|---|---|
| `Cargo.toml` | `cargo test --quiet` |
| `package.json` | `npm test`, then `npm run build` (or `npm run check`) |
| `go.mod` | `go test ./...` |

Node gates run with `CI=1` and `NO_COLOR=1`, so a test runner does not enter
watch mode. The default per-gate timeout is 600 seconds
(`RAVENBOT_GATE_TIMEOUT_SECS`).

A red required gate vetoes completion even when the lead said the product was
done. Its output tail becomes the next cycle's task, which is how "the tests
fail" turns into work instead of a false finish. Results are persisted in
`office_gate_runs` with the goal id and posted to the office conversation. A
missing or unrecognized manifest is not a failure; it is reported as no gate
detected. Optional custom gates are reported but cannot veto completion.

The contract's `max_turns`, `max_tokens`, and `max_wall_seconds` stop the office
cycle loop. Tokens are measured from the runs linked to the office (the office
thread and its node threads), and usage is monotonic. The office re-reads the
goal before every cycle, so a pause, block, or budget exhausted mid-turn stops
the next cycle instead of spending past it.

Each reviewed cycle records a strict `done | continue | blocked | wait` verdict,
its reason, and its turn count. `blocked` and `wait` stop the turn without
claiming completion; `wait` pauses the goal because there is no process-wait
support yet. A done goal is not reopened by a late review. Office settings can
pause, resume, and reopen the goal (a done/failed/cancelled goal can start a new
mission); a paused goal is refused at the start of the next turn.

## Background tasks

`task_runner` can start a long build, test, or server with `background: true`.
It returns a `process_id` immediately. The process output is captured to a
bounded 200k-character tail, and the registry keeps finished entries for 30
minutes.

- `action: "wait"` with `process_id`, optional `pattern`, and `timeout` releases
  on process exit, a matching output line, or the timeout.
- `action: "status"` inspects one process.
- `action: "processes"` lists this conversation's background processes.
- `action: "stop"` terminates one.

A background child is owned by a supervisor task with `kill_on_drop`, so closing
the app does not leave an orphan. The task runs through the same sandbox and
hard safety policy as a foreground task. Host-tier commands apply CPU, memory,
and file-size limits but not a user-wide process cap, which would otherwise make
ordinary `fork` fail on a busy desktop.

## Office handoff

An office node is not a chat turn. No user is watching it, so `ask_user` is
skipped (and the prompt says not to ask). Before finishing, the node updates
`Shared/STATUS.md` with the artifact path, a one-line result, and what remains.
The next teammate and the lead read that file, not the node's private context.
`STATUS.md` is seeded once at office creation with the mission, milestone,
blockers, and roster; after that it belongs to the agents and re-seeding never
overwrites it.

The planner is told to name only files that exist or that the task explicitly
asks to create, to keep each task within roughly twenty tool rounds, and to split
large work by artifact. `Shared/STATUS.md` is the durable progress file that
bridges context windows.

A node can also hand work to a teammate through `delegate`. The child inherits
the office, project roots, autonomy, and live stream emitter, so delegated work
is visible in the room instead of disappearing behind a tool result.

## Work ledger

Every office plan node is also a durable `work_item` (migration 066), keyed by
the graph node id. The graph remains the scheduler; the ledger is the readable
record:

- planned nodes and dependency edges are seeded before execution;
- a node start atomically claims the item, increments its attempt count, and
  takes a 15-minute lease; an expired lease is reclaimable by a new run;
- done, failed, and skipped transitions also write the result/error and append
  a `work_events` entry;
- a loop iteration or loop-group reset reopens the item without spending an
  attempt;
- comments and artifact paths are appended, never used to rewrite the task.

Changed files from a node's run are attached to its work item as artifacts
from the `file_changes` journal, so the drawer lists what the run actually
produced without trusting prose. The office board opens a task-detail drawer. It reads the item with
`get_work_item`, appends notes with `add_work_comment`, and exposes
`retry_work_item`, `block_work_item`, `unblock_work_item`,
`request_work_review`, `request_work_changes`, and `approve_work_review`.

Retry, unblock, and request-changes reopen the item's saved graph: the target
node and every descendant are reset to pending, their blackboard outputs are
cleared, and the office resumes. Completed upstream work is kept. The reviewer
comment is written into the implementer's thread as a `[Rework requested]`
message, so the next run sees it.

After a graph finishes, its items are projected into the workspace's
`Shared/STATUS.md` between `<!-- ravenbot:work-ledger:begin -->` and
`<!-- ravenbot:work-ledger:end -->`. Everything the agents wrote outside those
markers is preserved. The SQLite ledger is authoritative; the file is a
projection for the next agent and the human reading the folder.

## Visual output and skills

The chat renderer understands four visual fences without a third-party chart
runtime:

| Fence | What it renders |
|---|---|
| `chart` | Bar, line, area, pie, donut, and scatter SVG charts from a JSON spec (labels + series, or row data with `x`/`y`/`seriesKey`). |
| `slides` | A deck: Markdown slides separated by a line containing only `---`, with previous/next navigation and an internal scroll stage. |
| `mermaid` | Flowcharts, sequence diagrams, gantt charts, timelines, and architecture diagrams. Wide diagrams scroll inside their own box. |
| `visual` | An interactive HTML page with its own scripts, served on a separate `ravenvisual://` origin and sandboxed without same-origin access. It reports its height, opens web links through the app, and can request fullscreen. |
| `html` | A sandboxed, script-free static preview artifact. |

The chart renderer is deliberately local: it works offline, inherits the app
theme, renders during streaming, and never executes model-authored script. A
malformed chart shows its error and source instead of taking the message down.

Markdown images and image attachments open in a focused viewer (download,
caption, Escape, backdrop close). The `visuals` workflow skill documents the schemas and design rules; it is
auto-injected when the user asks for a chart, graph, plot, dashboard, slide,
deck, or presentation. The `fframes` workflow skill covers programmatic motion
graphics (Rust + SVG + ffmpeg): install, scaffold, design, inspect frames,
review contact sheets, check loudness, and render an MP4. It is auto-injected
for video, animation, motion-graphic, promo, explainer, reel, and clip requests.

Skill and connector plumbing:

- The full VoltAgent awesome-agent-skills catalog is resolved live by
  `awesome_fetch`. A catalog page is followed to its GitHub source, then the
  real `SKILL.md` is fetched (not an HTML preview). Instructions are returned in
  full, capped at 20,000 characters with an explicit truncation marker.
- MCP tools with the same name from different servers are renamed
  `<server>__<tool>` instead of one silently replacing the other. The runtime
  keeps up to 64 tools per run and 48 MCP tools per bot; the old caps dropped
  whole connectors.
- A workflow skill is injected into the system prompt only when it is enabled
  or the request matches its intent. Tool skills are offered as callable tools;
  the two are separated before the tool cap is applied, so a prose workflow
  cannot crowd out `file_read`.

## Long-run offices

An autonomous office is a sequence of bounded turns, not one endless run. The
heartbeat starts a turn only when the office is idle and its previous turn is
old enough, so a crash or a long provider call cannot stack turns on top of each
other.

- **Multi-cycle turn.** A vague "continue" request, an autonomous heartbeat, or
  a cycle with failed tasks runs up to four plan→execute→synthesize cycles
  (`RAVENBOT_OFFICE_CYCLES`, 1–8). The lead reviews product completion after
  each cycle; a software goal gets a forced verification/continuation cycle
  after its first milestone even if every task says done. The turn stops on
  verified completion, the cycle cap, two cycles without a workspace change,
  budget exhaustion, a queued user message, or a cancel.
- **Graph resume.** Each cycle's `TaskGraph` is snapshotted to
  `office_graphs` while it runs. On startup, the saved graph is restored,
  completed nodes keep their outputs, and only nodes that were interrupted are
  rescheduled. A node whose run has a checkpoint resumes that exact run and
  replays its in-flight tool results; a node without a checkpoint falls back to
  its thread with a resume note. This avoids replanning finished work and avoids
  starting the interrupted task from scratch.
- **Budget ceiling.** `ChatRoom.budget` is enforced against the per-office,
  per-member spend since the first of the month. Each member's prior lifetime
  usage is its baseline; a shared bot's spend in another office does not count.
  When the ceiling is reached, autonomy pauses and the room gets one message
  naming the spent amount and the limit.
- **No-progress guard.** The heartbeat fingerprints the workspace (relative
  paths, sizes, mtimes; VCS metadata, virtualenvs, and build output excluded).
  Three accepted cycles without a changed fingerprint pause autonomy with a
  pointer to `Shared/STATUS.md`, instead of repeating the same briefing.
- **Bootstrap guard.** A workspace with no code root or manifest gets a
  deterministic creation-first plan. Three empty bootstraps pause autonomy.
- **Concurrent offices.** Each graph node streams on its own thread, so two
  offices (or an office and a chat) can run at the same time. The only
  serialization is one turn per office, and the graph's three-node batch limit.
- **Queued user messages.** A message sent while an office is working is
  persisted in `office_queue` and starts as soon as the active turn releases
  the office. FIFO order, pause/resume, cancel, and restart survival are
  enforced by the queue worker; autonomy yields while a user queue is pending.
- **Context handoff.** Compaction keeps the system prompt plus a token-budgeted
  tail and replaces the dropped middle with a structured summary: task/goal,
  current request, file paths, tools/commands, errors, and earlier context. Tool
  call/result pairs are never split. The handoff is persisted with the last
  core message it covers (`context_summaries`, `threads.context_floor_message_id`),
  and a later run replays `summary + messages after the floor`. The core-message
  mapping travels with the model messages, so the floor is the last message the
  summary actually covers. This keeps a long project's state usable without an
  extra model call and without re-reading the whole transcript.
- **Micro-compaction.** Before dropping any message, old tool results outside
  the newest twelve rows are replaced in place with a one-line recovery
  pointer. A tool call keeps a result, the conversation structure survives, and
  a build log from six rounds ago stops consuming the window. If stubbing is
  enough, no summary is created; if it is not, the full handoff runs on the
  original messages so tool detail is not lost from the summary.
- **Session search.** The durable transcript is indexed with FTS5. The
  read-only `session_search` tool returns ranked snippets with
  `>>>match<<<` markers (and can be scoped to one thread), so an agent can
  recover a decision, path, command, or error that compaction summarized away.
- **Context meter.** Every model round emits an estimated context-window usage.
  The composer warns at 80% and 95%; a click requests a one-shot manual
  compaction on the next run.
- **Tool budget.** Office-style work defaults to forty model↔tool rounds
  (configurable per bot or via `RAVENBOT_MAX_TOOL_ROUNDS`). When the budget is
  exhausted the node returns a progress report, is marked partial, and its
  dependents are skipped rather than built on an unfinished artifact.

## Guards

| Guard | Behavior |
|---|---|
| Hard denylist (`safety.rs`) | Refuses disk/partition/power commands, destructive git, privileged Docker, recursive deletes of a workspace root or outside every root, `cd` escapes, and symlink escapes. |
| Whole-machine scan guard | Inside a workspace, refuses `find /`, `grep -r /`, `ls -R /`, and home/ancestor scans with an actionable error. |
| Native repeat guard | The same tool + arguments is not executed indefinitely; the second identical call is skipped and a repeated loop is forced to a final answer. |
| Office autonomy | Runs are spawned outside the IPC call, the board is persisted by the backend, and a heartbeat starts the next turn on idle. Unattended runs use Review: reads and low-stakes writes proceed, high-risk tools park on the attention card, and destructive commands are refused by the hard safety policy before a card exists. One turn per office is enforced atomically. The header toggle, kill switch, pending question, budget ceiling, or repeated no-progress cycles pauses it. |
| Encrypted secret fallback | The OS keychain is used when available. Without it (and when the config directory is writable), secrets are sealed with XChaCha20-Poly1305 under `config/sealed-secrets` (directory 0700, key and entries 0600) and the database keeps only `__RAVENBOT_KEYCHAIN__`. Startup migrates legacy plaintext provider, vault, plugin, MCP, webhook, signing and app-setting secrets. A copied `ravenbot.db` alone cannot reveal them. |
| Provider circuit breaker | Four consecutive final failures open a provider-wide circuit for 30s (or the provider's longer retry window). New requests skip that provider and fall back immediately; one success resets it, and a half-open probe after the window decides whether to close or reopen it. |
| Memory retrieval | FTS5 (migration 75) supplies a BM25-ranked candidate set, which the configured embedding reranks. The local lexical backend narrows to those candidates and falls back to a full vector pass only when the query has no indexed term; a semantic Ollama/OpenAI backend keeps the full vector pass. A fact with no embedding is still findable, and deleting a fact removes its FTS row. |
| Human-card waits | Approval, question and budget-confirmation polls check the kill switch, cancellation and pause every 400ms. Pause parks a resumable run with a checkpoint, denies the approval or expires the question, and the UI reloads the card lists so a stale card cannot authorize later work. |
| Per-round budget | The bot's budget is checked before the first model call and before every later model/tool round, not only at run start. `HardStop` fails before the overspending call; `AskUser` asks once per run and a Continue override covers the remaining rounds. |
| Empty node result | The executor marks a node failed and skips its dependents instead of showing a misleading `Done · No response`. |

## Verification

The runtime is tested at the harness boundary, not just the happy path:

- `a_tool_loop_ends_with_a_report_not_an_empty_run` — a provider that calls one
  tool forever must end with a final report, a single tools-disabled wrap-up
  call, and a completed run.
- `whole_filesystem_discovery_is_refused_inside_a_workspace` and
  `workspace_scoped_discovery_is_allowed` — the scan guard is workspace-aware.
- `editor_*` / `viewer_*` — workspace file safety and conflict handling.
- `office_board_projection_survives_the_window` — the board is persisted from
  events even when no view is open.
- `office_budget_counts_only_this_offices_spend` — a shared bot's earlier spend
  is a baseline, and the monthly ceiling is enforced on the delta.
- `a_mid_run_budget_crossing_stops_before_the_next_model_call` — crossing a hard
  limit after the run started fails before the next model request.
- `legacy_plaintext_provider_keys_move_to_the_encrypted_vault` — a legacy
  provider key is sealed, its row becomes a marker, and reads still return it.
- `secrets_round_trip_and_delete`,
  `the_vault_file_and_key_are_private_and_do_not_contain_plaintext`, and
  `a_tampered_entry_is_unavailable_not_wrong_plaintext` — vault round-trip,
  0600 permissions, ciphertext-only files and tamper rejection.
- `a_provider_circuit_opens_and_does_not_affect_other_providers`,
  `a_success_closes_the_circuit_and_clears_the_streak`, and
  `an_open_provider_circuit_is_skipped_before_any_call` — the breaker opens,
  resets, isolates providers, and skips the failing provider in `call_model`.
- `lexical_candidates_carry_retrieval_when_vectors_are_uninformative` and
  `a_fact_without_an_embedding_is_still_retrievable_lexically` — FTS5 supplies
  the candidate set, an embedding can be absent, and deletion removes the FTS
  row. The office store has the same regression test.
- `the_default_local_backend_meets_the_hybrid_recall_target` — the local hybrid
  path clears the 0.8 Recall@5 target on the fixed 50-fact fixture.
- `pause_while_waiting_for_approval_parks_the_run_without_running_the_tool`,
  `pause_while_waiting_for_a_question_parks_the_run_and_expires_the_card`,
  `pause_while_waiting_for_a_budget_decision_parks_the_run`, and
  `cancel_while_waiting_for_approval_ends_the_run_promptly` — a human-card wait
  is interruptible, the tool does not run, and the card cannot be answered
  after the run has paused or cancelled.
- `workspace_fingerprint_changes_only_with_real_edits` — edits and new files
  change the progress fingerprint; virtualenv and build churn do not.
- `an_office_rejects_a_second_turn_while_one_is_active` — the office claim is
  atomic and releases on drop.
- `Visuals.dom.test.ts` — a chart renders as SVG, an invalid chart shows its
  error, and a slide deck advances on a real button click.
- `fetch_tests` — catalog URLs resolve to raw `SKILL.md` paths, the README
  resolves live ids, and oversized instructions are truncated honestly.
- `colliding_mcp_tool_names_are_server_prefixed` — two connectors exposing the
  same tool keep both callable names.

## What the public harnesses do that we adopted

- **Anthropic, *Effective harnesses for long-running agents* / *Writing
  effective tools for agents***: incremental progress, a progress/feature file,
  verifying with real execution, actionable tool errors, token-efficient
  responses, and explicit tool descriptions. Adopted as `Shared/STATUS.md`,
  the office handoff, and the tool discipline block.
- **DeepSeek Harness**: dedicated `read`/`glob`/`grep` tools over shell
  commands, read-before-write, exit-code checks, and an advisory repeat-call
  reminder with escalating thresholds. Adopted as tool-specific guidance, the
  native repeat guard, and the two-round warning.
- **OpenCode**: a short base prompt plus dynamic tool guidance, explicit
  tool-over-shell preference, and parallel independent calls. Adopted as the
  split between the baseline prompt and the per-run discipline block.
- **Grok Build (xAI)**: bounded turns, goal verification, and progress-aware
  long-running sessions. Adopted as the bounded loop, the final-answer
  fallback, and the office heartbeat (the heartbeat is deliberately more
  forgiving than a hard turn cancellation).

### Sources

- <https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents>
- <https://www.anthropic.com/engineering/writing-tools-for-agents>
- <https://github.com/deepseek-ai/deepseek-harness>
- <https://github.com/anomalyco/opencode>
- <https://github.com/xai-org/grok-build>

## Beyond this harness

This document describes the current bounded-turn office. The next layer —
durable per-task execution, structured context handoff, completion contracts,
background delegation, and curated memory/skills — is planned in
[`LONG-AGENTIC-WORK-PLAN.md`](LONG-AGENTIC-WORK-PLAN.md). The plan is explicit
about what exists, what is missing, and which phase must land before the next.
