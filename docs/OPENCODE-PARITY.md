# OpenCode parity and 24/7 harness audit

**Date:** 2026-11-23
**Scope:** what the OpenCode desktop and session renderer actually do, what
RAVENBOT now shares, and what is still missing before a single agent can run
unattended for days like a DeepSeek/Grok-style harness.

Source inspected: `github.com/anomalyco/opencode` (shallow clone), primarily
`packages/desktop`, `packages/app`, `packages/session-ui`, `packages/ui`, and
`packages/core`.

## 1. What OpenCode's desktop is

| Layer | Implementation | Consequence |
|---|---|---|
| Desktop shell | Electron (`packages/desktop`, `electron-vite`) | Native window/tray/updater; the UI is a web app. |
| App UI | SolidJS (`packages/app`), router, virtualised session timeline | Long sessions stay responsive; message rows are recycled. |
| Shared session UI | `packages/session-ui` | Markdown, tool parts, reasoning, attachments, compaction. |
| Design system | `packages/ui` + `packages/theme` | Inter / IBM Plex Mono, 14px base, 150–160% line height, tokenised colour. |
| Agent runtime | `packages/core` | Durable session runner, tools, permissions, snapshots, worktrees, MCP, VCS. |

## 2. What OpenCode's text renderer actually does

OpenCode does **not** call `marked.parse()` and inject HTML into the message.
Its pipeline is block-oriented and worker-backed:

1. `@opencode/patchmark` (`markdown-stream.ts`) projects a growing answer into
   stable blocks, keeping code fences and live prose separate.
2. `markdown.worker.ts` runs the parser and `@pierre/highlights` in a Web
   Worker, returning only changed blocks/tokens to the main thread.
3. `markdown.tsx` caches completed blocks, streams only the current block,
   renders Mermaid/KaTeX through dedicated nodes, and adds copy/wrap/image/
   session-link controls.
4. `markdown.css` supplies the prose system:
   - 14px body at **160%** line height;
   - **17px h1 / 15px h2 / 14px h3** with 28/24/20px top margins;
   - 32px list indent, 8px between list items;
   - inline code as a quiet 8%-fill token (no border);
   - links un-underlined until hover;
   - `overflow-wrap: break-word`, not `anywhere`, for prose.
5. Streaming text is paced (`paced-text-machine`) so tokens arrive smoothly
   instead of re-laying out the whole message on every delta.

## 3. What RAVENBOT already has

RAVENBOT's renderer is a **token renderer**, not an HTML injector, which is the
important security property:

- `src/lib/markdown/MarkdownMessage.svelte` lexes `marked` blocks;
- `MarkdownBlock.svelte` / `MarkdownInline.svelte` build real elements for
  headings, lists, links, images, code, tables, Mermaid, KaTeX and charts;
- raw HTML is shown as escaped text and unsafe links are refused;
- `MarkdownCode.svelte` adds copy/artifact actions;
- `ChatRoomView` / `ThreadView` paginate message history.

**Adopted in this change:** `src/lib/styles/components.css` now carries an
OpenCode-parity prose rhythm: 14px/1.6, 17/15/14 headings with hierarchical
margins, 32px lists / 8px list items, 8% inline-code fill, hover-only link
underlines, and `break-word` prose wrapping. The token renderer and its safety
properties are unchanged. This is the correct half of OpenCode's renderer to
copy: typography and block semantics, not Electron or the worker cache
(RAVENBOT's stream is already bounded by the 24k tool-result cap).

**Not copied yet:** worker-based parsing/highlighting and the block-level
streaming cache. Timeline virtualisation now has its own measured row window;
these remaining items are performance work, not visual fidelity, and are
listed below.

## 4. 24/7 harness readiness matrix

Legend: **Present** = shipped and test-covered; **Partial** = one path exists
but not the general one; **Missing** = no implementation.

| Capability | RAVENBOT today | OpenCode reference | Verdict |
|---|---|---|---|
| Background daemon/service | `ravenbot daemon`, `src-tauri/src/service.rs`, systemd/launchd/task units | Desktop + CLI daemon | **Present** |
| Crash recovery | startup reconciliation, `resume_interrupted_runs`, `resume_interrupted_offices`, run checkpoints | session resume | **Present** |
| Durable queue | `office_queue`, work items, leases, orphan reconciliation | session queue | **Present** |
| Scheduler | `crates/scheduler`, routines, workflow schedules, cron, workflow webhooks | automation/jobs | **Present** |
| Provider retry/fallback | `governor.rs`, provider chain, 429 cooldown, fallback model | provider policy | **Present** |
| Context compaction | `Runtime` compaction + handoff summary | compaction | **Present** |
| Run watchdog | heartbeat + stalled-run watchdog in `executor.rs` | process monitor | **Present** |
| Permissions/approval | approval broker, sudo/destructive denylist, Ask/Auto/Full | permission modes | **Present** |
| MCP client | `crates/mcp`, server catalog, resources | MCP + event tools | **Present (client)** |
| Browser/computer | dock, `browser_dom`, `computer_control`, screenshot | browser tool | **Present** |
| Multi-agent office | plan graph, roster, review judge, goal autopilot | subagents/agents | **Present** |
| Goal loop for a 1:1 agent | durable thread goal, one-node graph pinned to the user thread, office review/gates, pause/resume/stop, restart-safe scheduling, same heartbeat in the headless daemon | session goal/todo loop | **Present** |
| File snapshot / revert | content-addressed before-images, run/file revert previews, conflict checks, Projects and Runs actions | `packages/core/src/snapshot.ts`, VCS | **Present** |
| Isolated worktrees | opt-in conversation-scoped `git worktree`, fail-closed on dirty/non-Git projects, review + checked apply/discard, goal/snapshot aware | `packages/core/src/worktree.ts` | **Present** |
| LSP / project diagnostics | real stdio LSP client, five default servers, agent tool, automatic goal-round checks, Projects diagnostics and a Settings JSON map | project LSP settings in app | **Present** |
| Unified patch tool | `file_edit` (also `apply_patch`) parses standard unified diffs and Codex patches in-process, validates every hunk, writes atomically, and journals each file | `tool/patch*` | **Present** |
| Timeline virtualisation | measured variable-height row window, top/bottom spacers, anchor-preserving prepends, rail offsets and mounted-target reveals; the 30/50 paging window remains the load budget | `session/timeline/virtualizer.tsx` | **Present** |
| **Worker markdown/highlight** | marked + custom tokens on the UI thread | `markdown.worker.ts` | **Missing (P2)** |
| **MCP event-triggered jobs** | MCP client only; scheduler triggers are cron/webhook | automation from MCP events | **Missing (P2)** |
| **Public session share** | remote access/device pairing, no public share link | share UI | **Missing (P3)** |

## 5. The 24/7 harness

### Shipped — a goal loop for every agent thread

The office already had planner → graph → verify → judge → continue. A 1:1
agent had one turn. That gap is now closed:

- `GoalQueries` owns a thread-scoped goal with round and token budgets,
  objective, outcome, constraints and an optional verification gate. A paused
  goal can raise either budget on resume without resetting its history.
- `thread_goal_loop` reads only active, due, office-less goals in SQL; it is
  serial across goals and yields to any live or paused run in the same thread.
- `run_thread_goal_turn` builds a one-node graph and pins the node to the
  user's existing thread, then runs the same `office_cycle_review`, goal gates
  and deterministic completion rules used by offices.
- Every round posts a visible verdict into the conversation; `record_verdict`
  schedules the next round, pauses at the budget, or makes done/blocked
  terminal.
- Startup closes an interrupted goal round instead of resuming it raw, then
  re-arms the goal. The headless daemon runs the same loop.
- `ThreadView` exposes start/pause/resume/stop, the round counter and the last
  verdict; a background round emits a refresh event.

That turns “one good turn” into “works until the evidence says done”.

### Shipped — snapshot and revert

The `file_changes` ledger proves what changed; it now also has a way back.

- The first mutating tool in a run walks the workspace roots and writes every
  regular file to a content-addressed blob store under
  `<RAVENBOT_HOME>/snapshots`, with a path manifest in
  `workspace_snapshots` / `workspace_snapshot_entries`. Generated trees
  (`.git`, `node_modules`, `target`, caches) are skipped; caps mark a snapshot
  partial instead of pretending it is complete.
- `preview_run_revert` / `revert_run_files` restore the paths a run wrote and
  delete the paths it created. `revert_file_change` restores one path to its
  pre-run version.
- A later write, a changed file hash, an unavailable before-image, a symlink, or
  a path that escaped its root blocks the action. `force` is a separate,
  explicit confirmation. A partial snapshot is never used to delete files it
  did not see.
- Blobs are private to the user and deduplicated. A shell command that wrote a
  file without emitting a `file_changes` row remains outside a run revert; the
  ledger is the contract for which paths may be touched.

### Shipped — in-process patch application

`file_edit`, reached through the model-facing `apply_patch` alias, now applies
a standard unified diff without shelling out to `patch(1)`.

- Hunk headers, context, additions, and deletions are parsed with the hunk
  counts as the boundary, so a removed line beginning with `--` is not mistaken
  for a file header.
- Every file and hunk is checked before the first write; a mismatch leaves the
  workspace unchanged. Adds, deletes, multi-file diffs, CRLF, and a missing
  final newline are supported; renames and binary diffs are refused with a
  clear message.
- Writes go through a temp file and rename, preserve the existing mode, and
  restore earlier files if a later write fails. The workspace snapshot and
  `file_changes` ledger receive each file's absolute path, diff, and hashes.
- `code_edit` remains for callers that explicitly want the system patch tool.

### Shipped — isolated Git worktrees

A conversation can opt into a real Git checkout so a long run cannot dirty the
project the user is still editing.

- The first run creates `git worktree add --detach` under the RAVENBOT home and
  records it per thread; later runs reuse the same checkout.
- Creation fails closed unless the project is a Git repository with a clean
  working tree. Isolation never silently degrades to the main folder.
- The conversation banner reviews the diff and applies it with a checked
  `git apply`, or discards it. A conflicting main checkout is reported and the
  worktree stays available.
- Goal review, workspace snapshots and gates use the checkout that actually
  produced the round. Turning isolation off leaves an active checkout intact
  for review.
- Small Git-ignored local files (for example `.env`) are copied into the
  checkout for reading. Ignored changes are not part of Apply.

### Shipped — language diagnostics

- `crates/runtime/src/lsp.rs` implements Content-Length framing, initialize,
  initialized, didOpen, publishDiagnostics, server-request replies, shutdown and
  exit over any reader/writer pair. The production path spawns the server in the
  workspace; tests use an in-memory duplex and a fake server.
- Defaults cover rust-analyzer, pyright-langserver, typescript-language-server,
  gopls, and clangd. A server is selected by the changed files' extensions and
  must be installed on PATH; nothing is auto-installed.
- The `diagnostics` tool is exposed only when a matching server is installed and
  the agent has the Shell capability. It checks named files or the run's
  changed files and returns typed, 1-based diagnostics.
- Every 1:1 goal round checks the files it changed. Error diagnostics become
  judge failures and count toward the round's retry backoff; warnings do not
  block. A missing server or timeout falls back to the existing deterministic
  gates, so an unavailable LSP cannot stall every goal.
- Projects has a per-file Diagnostics button; Settings → Diagnostics → Language
  servers edits the `lsp_servers` map and can restore defaults.
  `RAVENBOT_LSP_TIMEOUT_SECS` controls the per-check deadline.

### Shipped — virtualized timeline

The message window still bounds how much history is loaded; a measured
virtualizer now bounds how much of it is mounted.

- `src/lib/components/chat/VirtualTranscript.svelte` mounts the viewport plus
  overscan, replaces the rest with top/bottom spacers, and measures mounted
  rows with a `ResizeObserver` plus an `offsetHeight` fallback.
- Heights are variable. A row's real height wins once measured; unmounted rows
  use a line/code/attachment-aware estimate from
  `src/lib/chat/virtualRows.ts`.
- Paging changes follow the row at the top of the mounted range by key and
  shift the range to its new index, so "load earlier" prepends history without
  remounting a different set of messages under the reader.
- A full-window jump (search, reply, rail, global focus) mounts the target and
  scrolls it to the center. Long jumps are instant because animating across a
  changing spacer would fight the window; short moves still animate.
- The rail reads virtual offsets for every loaded user message, not only the
  mounted ones. `Alt+↑/↓` hands off to the view at the mounted edge so it can
  mount and reveal the adjacent message.
- The live streaming row and approvals stay outside the virtual list, so their
  entrance and height animation are unchanged. Rows inside the virtual list
  are mounted with `.entries-static`, so recycling cannot replay the history
  entrance animation.
- The row shell keeps `content-visibility: auto`; the virtualizer's mounted
  rows override it to `visible` so their measured height is exact.

## 6. Renderer roadmap (after the typography change)

1. **Block streaming cache** — memoise completed blocks so an 8k-token answer
   re-renders only the last block. This is the biggest UI win for long runs.
2. **Worker parse/highlight** — move `marked` lexing and code tokenisation off
   the UI thread; keep the current security checks on the returned tokens.
3. **Tool-part renderers** — show command output, diffs and file reads as
   first-class cards rather than one-line trace rows.

## 7. What not to copy

- Electron: RAVENBOT's Tauri shell is smaller and already ships signed
  installers; do not replace it.
- HTML injection: OpenCode's `renderHtml: false` is the right choice and
  RAVENBOT already does the equivalent.
- Worker complexity before block caching: the worker only pays off once blocks
  are stable and large; add the cache first.

## 8. Verification

- `npm run check` — clean; `npm test` — full suite.
- The prose rules are in `src/lib/styles/components.css`; the token renderer
  and its DOM tests are unchanged.
- Thread-goal end-to-end: `a_thread_goal_round_reuses_the_conversation_and_records_its_verdict`
  runs a persisted goal with a fake provider and proves the round reuses the
  1:1 thread, records the reviewed verdict and spends tokens in that thread.
- Restart: `restart_closes_a_goal_round_and_leaves_the_goal_runnable` proves an
  interrupted goal round is closed and the active goal is re-armed.
- Scheduling: `a_thread_goal_is_created_once_with_its_round_budget` and
  `a_thread_goal_yields_to_a_live_run` cover the due query, round budget and
  the live/paused-run guard. `raising_a_goal_budget_keeps_its_history` and
  `raising_the_token_budget_lets_the_next_turn_start` cover budget edits and
  the pause/resume path.
- Snapshot/revert: `a_file_write_is_journaled_with_a_revertable_before_image`
  exercises capture → journal → restore through the real tool path;
  `an_apply_patch_alias_applies_a_unified_diff_and_journals_it` does the same
  through `apply_patch`; `a_run_snapshot_restores_overwrites_and_deletes_created_files`
  and `a_later_write_blocks_the_revert_until_forced` cover run-level revert and
  the force gate. `RunsPanel` and `ProjectsView` DOM tests cover the preview
  and confirmation flows.
- Patch tool: `patch::tests` covers multi-file parsing, add/delete, CRLF,
  no-final-newline, mismatched context, and rename rejection; the file-edit
  tests cover the real atomic write path.
- Worktrees: `an_isolated_run_writes_to_a_worktree_not_the_project` proves a
  real run writes only to the checkout and journals worktree paths;
  `a_clean_repo_is_checkout_shared_then_applied` covers reuse, a commit inside
  the checkout, review and checked apply; `discard_leaves_the_project_untouched`
  and `isolation_refuses_a_dirty_or_non_git_folder` cover the safety edges.
  `a_thread_has_one_active_checkout_and_a_finished_history` covers the DB
  lifecycle.
- LSP: `lsp::tests` covers frame round-trips, workspace detection, diagnostic
  parsing, error-first truncation, capped failure summaries, and a full
  initialize → didOpen → publishDiagnostics session against an in-memory fake
  server. The ignored `clangd_reports_a_real_error` test validates a real
  installed server on demand. `thread_paths_are_scoped_to_the_turn` covers the
  changed-file scope used by the goal gate, and
  `language_diagnostics_force_a_continue_verdict` proves a diagnostic failure
  overrides a done verdict. The ProjectsView DOM test covers the diagnostics
  command and error list.
- Timeline virtualisation: `virtualRows.test.ts` covers prefix arithmetic,
  viewport/overscan slicing, tail rows, spacer clamping and height estimates.
  `VirtualTranscript.dom.test.ts` proves a 240-row transcript mounts a bounded
  tail, scrolls to a middle window, keeps the mounted keys through a 50-row
  prepend, reveals an off-screen target, and reports every rail offset.
  `TranscriptRail.dom.test.ts` covers virtual positions without DOM rows, and
  `navigation.dom.test.ts` covers the mounted-edge handoff for Alt+Arrow. The
  virtualizer was also exercised in real Chromium pages: once with 300–600
  plain variable-height rows and once with 120 real `ChatMessageRow` rows.
  A 100-row prepend kept the same eight mounted keys and added exactly the
  spacer delta to `scrollTop`, and a rail-style jump centered a 124px target at
  237px in a 598px viewport. The temporary check routes and browser profiles
  were removed.
