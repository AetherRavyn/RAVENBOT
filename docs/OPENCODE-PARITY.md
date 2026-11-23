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

**Not copied yet:** worker-based parsing/highlighting, block-level streaming
cache, and timeline virtualisation. Those are performance work, not visual
fidelity, and are listed below.

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
| **Goal loop for a 1:1 agent** | office rooms only; `ThreadView` has no plan/goal; `GoalQueries` supports `thread_id` but the scheduler only reads `office_id` | session goal/todo loop | **Missing (P0)** |
| **File snapshot / revert** | `file_changes` ledger stores diffs but there is no restore command/UI | `packages/core/src/snapshot.ts`, VCS | **Missing (P0)** |
| **Isolated worktrees** | not present | `packages/core/src/worktree.ts` | **Missing (P1)** |
| **LSP / project diagnostics** | no LSP client; code edits rely on compiler/test tools | project LSP settings in app | **Missing (P1)** |
| **Unified patch tool** | `code_edit`/`file_edit` string edits; no `apply_patch` | `tool/patch*` | **Missing (P1)** |
| **Timeline virtualisation** | history paginated, all loaded rows remain mounted | `session/timeline/virtualizer.tsx` | **Partial (P1)** |
| **Worker markdown/highlight** | marked + custom tokens on the UI thread | `markdown.worker.ts` | **Missing (P2)** |
| **MCP event-triggered jobs** | MCP client only; scheduler triggers are cron/webhook | automation from MCP events | **Missing (P2)** |
| **Public session share** | remote access/device pairing, no public share link | share UI | **Missing (P3)** |

## 5. The three gaps that actually block 24/7 “any task”

### P0 — a goal loop for every agent thread

The office already has planner → graph → verify → judge → continue. A 1:1
agent has only one turn. A user can leave a 1:1 chat and nothing continues.

Implementation sketch:

1. `goals` already stores `thread_id` and `office_id` as nullable. Add
   `GoalQueries::get_by_thread`, `ensure_thread_goal`, and a scheduler query
   for `status = 'active' AND office_id IS NULL AND thread_id IS NOT NULL`.
2. Add `agent_goal_loop` next to `office_autonomy_loop`:
   - read the goal and the thread's recent messages;
   - build a one-node `TaskGraph` with the goal prompt and the thread's bot;
   - execute with `chatroom_id = None` but stamp the existing thread id on the
     node (the executor already reuses `node.thread_id`);
   - run the same completion judge/gate rules as the office;
   - write `rounds_used`, `next_run_at`, `last_verdict`, and a visible system
     message into the 1:1 thread.
3. Add a “Goal” toggle/status card to `ThreadView` (objective, gates, rounds,
   pause/resume/stop).
4. Reuse `goal_gates` for build/test commands; deterministic completion stays
   the same as the office (`quality_gate.rs`).

This is the single highest-value parity item: it turns “one good turn” into
“works until the evidence says done”.

### P0 — snapshot and revert

`file_changes` proves what changed, but cannot put the workspace back. A 24/7
agent needs a rollback point per turn.

Implementation sketch:

1. Add `WorkspaceSnapshot` capture at run start when the workspace is a git
   repo: `git stash create` (or a content-addressed tree in
   `~/.local/share/ravenbot/snapshots/<account>/<hash>`), including untracked
   files up to a size cap.
2. Store `run_id → snapshot_id` in a new table.
3. Add `revert_run(run_id)` and `revert_file_change(change_id)` commands:
   restore selected paths from the snapshot; refuse if a later write touched
   the same path unless the user confirms.
4. Surface “Revert this turn” on the run/trace card and per-file diff viewer.
5. Non-git workspaces: capture changed files only (from `file_changes`), not a
   full tree; the ledger already names every path.

### P1 — worktrees, LSP, patch

- **Worktrees:** a task gets its own `git worktree` so a failed 24/7 run cannot
  dirty the main checkout. Reuse the existing project folder as the base and
  list/remove worktrees from the project settings.
- **LSP:** start the language server for the project root, surface diagnostics
  after writes and before “done”; this catches type errors that tests miss.
- **Patch tool:** accept a unified diff, validate paths against the workspace,
  apply with a context check, and record it as one `file_change`. This is the
  tool OpenCode’s edit loop is built around.

## 6. Renderer roadmap (after the typography change)

1. **Block streaming cache** — memoise completed blocks so an 8k-token answer
   re-renders only the last block. This is the biggest UI win for long runs.
2. **Worker parse/highlight** — move `marked` lexing and code tokenisation off
   the UI thread; keep the current security checks on the returned tokens.
3. **Timeline virtualisation** — recycle message rows above/below the viewport;
   the page API already provides stable message ids.
4. **Tool-part renderers** — show command output, diffs and file reads as
   first-class cards rather than one-line trace rows.

## 7. What not to copy

- Electron: RAVENBOT's Tauri shell is smaller and already ships signed
  installers; do not replace it.
- HTML injection: OpenCode's `renderHtml: false` is the right choice and
  RAVENBOT already does the equivalent.
- Worker complexity before block caching: the worker only pays off once blocks
  are stable and large; add the cache first.

## 8. Verification for this change

- `npm run check` — clean.
- `npm test` — full suite.
- The prose rules are in `src/lib/styles/components.css`; the token renderer
  and its DOM tests are unchanged.
