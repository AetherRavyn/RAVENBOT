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
- **Context meter.** Every model round emits an estimated context-window usage.
  The composer warns at 80% and 95%; a click requests a one-shot manual
  compaction on the next run.
- **Tool budget.** Office-style work defaults to twenty model↔tool rounds
  (configurable per bot or via `RAVENBOT_MAX_TOOL_ROUNDS`). When the budget is
  exhausted the node returns a progress report, is marked partial, and its
  dependents are skipped rather than built on an unfinished artifact.

## Guards

| Guard | Behavior |
|---|---|
| Hard denylist (`safety.rs`) | Refuses disk/partition/power commands, destructive git, privileged Docker, recursive deletes of a workspace root or outside every root, `cd` escapes, and symlink escapes. |
| Whole-machine scan guard | Inside a workspace, refuses `find /`, `grep -r /`, `ls -R /`, and home/ancestor scans with an actionable error. |
| Native repeat guard | The same tool + arguments is not executed indefinitely; the second identical call is skipped and a repeated loop is forced to a final answer. |
| Office autonomy | Runs are spawned outside the IPC call, the board is persisted by the backend, and a heartbeat starts the next turn on idle. One turn per office is enforced atomically, so a user prompt cannot queue behind a heartbeat and mis-attribute its completion. The header toggle, kill switch, pending question, budget ceiling, or repeated no-progress cycles pauses it. |
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
