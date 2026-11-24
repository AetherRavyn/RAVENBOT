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

**Renderer work:** prose typography, timeline virtualisation, the block
streaming cache, off-thread markdown/highlighting and the tool-part cards are
all shipped.

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
| Worker markdown/highlight | module Web Worker lexes settled long bodies and colours long code blocks; streaming and short bodies stay synchronous, with a timed fallback and bounded caches on both sides | `markdown.worker.ts` | **Present** |
| Streaming block memoisation | completed block token objects survive each streaming re-lex; only the growing block and fade offsets invalidate | block cache in renderer | **Present** |
| MCP event-triggered jobs | long-lived stdio notification listeners, persistent routine triggers with method/resource/filter matching, cooldowns, supervisor in desktop and daemon; HTTP connectors are honestly reported as unsupported | automation from MCP events | **Present (stdio)** |
| Public session share | owner-created read-only links served by Remote access, hashed tokens, expiry/revocation, snapshot cutoff and sanitized text-only page | share UI | **Present (local listener)** |
| Tool-part renderers | shell commands as terminal cards, edits as file/diff cards, reads with line-number windows, and search/file-tree result cards | tool part UI | **Present** |

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

### Shipped — off-thread markdown and highlighting

The token renderer stayed the same; the work that produces tokens moved.

- `src/lib/markdown/worker.ts` is a real module Web Worker built by Vite. It
  imports the same `lexBlocks`, `hl` and `hlLanguage` functions as the UI
  thread, so there is no second parser or highlighter to drift.
- `src/lib/markdown/offThread.ts` owns the request ids, a 15s timeout, worker
  crash policy, request coalescing, prewarming and a bounded main-thread result
  cache. A crashed or timed-out worker is terminated, cools down for 30s, and
  can start fresh; every caller falls back synchronously meanwhile.
- Settled bodies under 4,000 characters and all streaming bodies stay on the UI
  thread. A streaming answer produces a new prefix on every reveal tick, so a
  worker round-trip there would be lag, not relief.
- Settled bodies over the threshold render through the worker. A body already
  in the cache paints on the first frame; a cold one shows its escaped raw
  source with `aria-busy` until the real token tree arrives. The fallback is
  the original synchronous lexer, so an unavailable worker cannot blank a
  message.
- Code blocks under 1,200 characters highlight synchronously. Longer settled
  blocks keep their plain text for the one frame the worker needs and then
  swap in the same colour tokens `hl()` would have produced.
- The thread and office views prewarm the long bodies in the loaded window, and
  prewarm results fill the main cache too, so scrolling back into a long answer
  is a cache hit rather than a placeholder.
- Caches are bounded on both sides; bodies over 200,000 characters and code
  over 120,000 characters are parsed but not retained.

### Shipped — streaming block memoisation

`marked` still re-lexes the body as it grows, but the component tree no longer
re-renders it.

- `src/lib/markdown/lex.ts` exposes `stableStreamTokens`: for each index, a
  block whose type and raw source are unchanged reuses the previous token
  object. Only the growing block and any newly appended blocks are new.
- Link reference definitions are compared separately. A definition added at
  the end can change the resolved href of a link above it, so if the definition
  list changes at all, the fresh tree is used.
- `MarkdownTokens.svelte` runs that stabiliser before building its keyed list.
  Because Svelte compares object props by identity, completed `MarkdownBlock`s
  receive the same token, the same `streaming=false`, and (away from the fade
  tail) the same `undefined` offset. The active block and the one or two blocks
  inside the fade window update; the rest of an 8k-token answer does not.
- The same stabiliser applies to nested blockquotes and list items, because
  those render through `MarkdownTokens` too. A stable container is reused whole;
  a growing list stabilises its own completed items.

### Shipped — MCP event triggers

The MCP client could call tools but could not receive server pushes. That gap is
closed for stdio connectors:

- Migration 73 adds `mcp_event_triggers`: one row per routine/server/event,
  with an optional resource URI for `resources/subscribe`, an optional
  case-insensitive params filter, a cooldown and last-fired/last-error state.
- `McpClient::listen_notifications` keeps a real stdio session alive after
  initialize, subscribes to requested resource URIs, answers server requests
  (`ping`, `roots/list`), and dispatches every notification. Notification lines
  are capped at 1 MiB and params are bounded before they reach a routine.
- `src-tauri/src/mcp_events.rs` groups enabled triggers by server, starts one
  listener per server, restarts when the trigger set or config changes, retries
  with backoff, and runs in both the desktop process and the headless daemon.
- A matched trigger atomically claims a cooldown slot in SQL before it starts
  the routine. Notifications that arrive during the cooldown are dropped, not
  queued; a per-trigger single-flight guard stops a slow run from overlapping
  itself, and a global semaphore bounds concurrent routine starts.
- The routine's first user turn carries a bounded `[MCP event trigger]` context
  with the server, method, resource URI and params. Routine execution remains
  under the normal unattended Review policy.
- Routines gain an MCP event panel: choose an enabled stdio connector, method
  (default `notifications/message`), optional resource URI, optional filter, and
  see trigger enable/delete, last-fired and listener state.
- HTTP connectors are rejected when creating a trigger and reported as
  `unsupported` by the supervisor. The existing HTTP transport is one-shot
  request/response; claiming it could receive pushes would be false.

### Shipped — public conversation shares

A conversation can now have a read-only public link:

- Migration 74 stores `thread_shares` (thread, token hash, expiry, revocation,
  view count). The token is returned once and never stored in plaintext.
- Creation is an owner-only local command and requires the Remote access
  listener to be running. The link is `<remote-url>/share/<token>`; if Remote
  access is loopback-only, the link is reachable there or through a tunnel.
- The public page is server-rendered, has no JavaScript, is `noindex`, and
  shows only user/assistant text. Tool calls, reasoning, attachments, approvals
  and questions are omitted; checklist labels are included without results.
- All content is HTML-escaped. A 404 is used for unknown, expired, revoked and
  malformed tokens alike.
- `created_at` is a snapshot boundary: messages added after the link was made
  are never exposed. Edits/deletions in the shared range still apply. A share
  renders at most the most recent 300 messages and says so when it truncates.
- The Thread header opens a Share dialog for owners: lifetime (24h/7d/30d),
  one-time link, copy, existing links, view counts, expiry and revoke.
- Deleting a thread deletes its share rows.

### Shipped — tool-part cards

Tool traces now show the work, not just its name:

- `src/lib/chat/toolCards.ts` normalises the persisted trace shapes (envelope,
  bare output, clipped string) into command, diff, read and search models.
- A `shell_exec` trace renders a terminal card with the command, working
  directory, exit status, stdout and stderr. The command is copyable.
- `file_edit`, `apply_patch`, `code_edit` and `file_write` render file counts,
  additions/deletions and the retained unified diff through `DiffView`.
- `file_read` renders a line-numbered window with offset/limit/next-offset
  context and a truncation badge.
- `code_search` and `file_tree` render result listings with the pattern/root,
  count and truncation state.
- Cards show a preview while the trace is collapsed and the full retained
  output when it is expanded. A trace with no card data keeps the generic
  JSON body, so a failed call never loses its error.

## 6. Renderer roadmap (after the typography change)

All planned renderer work is shipped: typography, block caching, worker
parsing/highlighting, timeline virtualisation and tool-part cards.

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
- Public shares: DB tests cover active/expired/revoked tokens, view counts,
  scoping and cleanup. Remote server tests prove the public page is 200 for an
  active token, 404 for unknown/expired/revoked tokens, escapes HTML, omits
  tool content and future messages, and sets `noindex`/`no-store` headers.
  The share dialog is owner-only and uses the same remote URL as the listener.
- MCP event triggers: DB tests cover create/list, cooldown claims, disabled
  triggers/routines and duplicate rejection. `crates/mcp` tests cover
  notification parsing/matching/context bounds, server-request answers, a fake
  child process that pushes a notification, and built-in rejection. The routine
  panel DOM test opens the event panel and creates a stdio trigger. A unit test
  covers the single-flight trigger guard.
  A full `cargo test --workspace`, release build and daemon smoke run cover the
  supervisor wiring; the daemon migration is now v73.
- Tool cards: `toolCards.test.ts` covers trace-shape normalisation, command
  output/exit codes, diff file summaries and bare diffs, line-window reads and
  search/file-tree results. `ToolTraceList.dom.test.ts` covers the terminal
  card and its expansion, a diff card with `DiffView`, read line numbers,
  search listings, and the generic-body fallback for a failed edit with no diff.
- Streaming block memoisation: `stableStreamTokens` tests prove an unchanged
  block keeps its token identity, a growing block and new blocks get fresh
  tokens, and a definition change falls back to the fresh tree. The
  `MarkdownTokens` keyed renderer then receives stable props for completed
  blocks; the active block remains the only one that changes on each reveal
  tick.
- Worker markdown/highlight: `offThread.test.ts` covers lexing, language-keyed
  highlighting, duplicate-request coalescing, prewarming, unavailable-worker
  fallback, timeout termination, crash and malformed-response fallback, and
  reset. `MarkdownMessage.dom.test.ts` proves a 4k+ settled body shows the
  escaped pending source and then the worker's real tokens;
  `MarkdownCode.dom.test.ts` does the same for a 1.2k+ code block. A real
  Chromium page reported `available: true`, produced an 8,811-character body's
  token list identical to the synchronous lexer, coloured a long TypeScript
  block, and logged zero fallback warnings. The production Vite build emitted
  a self-contained 53 kB `workers/worker-*.js`; loading that asset directly
  from a static preview also returned the heading token and keyword colours,
  so the worker path is not a dev-server artifact. The temporary routes and
  preview page were removed.
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
