# RAVENBOT roadmap: P0 → P3

This is the execution plan for closing the OpenBot gap without copying its
PolyForm Noncommercial source. Each track has a data model, surfaces, acceptance
tests, and a status. Work lands in dependency order; later tracks reuse the
contracts from earlier ones.

Legend: **done** means implemented in this repository and covered by tests;
**next** means designed and ready to implement; **planned** means scoped but not
started.

## P0 — Per-office message queue — done

### Problem
A second send while an office turn ran was rejected. The instruction was lost,
and the composer was disabled for the entire plan/execute/synthesize cycle.

### Data model
`crates/db/src/migrations/027_office_queue.sql`

```sql
office_queue(
  id, chatroom_id, content, visible, attachments,
  state, -- pending | running | done | failed | cancelled
  error, created_at, started_at, finished_at
)
```

A queued message is **not** written to `messages` until its turn starts. That
keeps transcript order equal to execution order and means a cancelled item
leaves no orphan prompt.

### Behaviour
- `send_to_chatroom` starts immediately only when the office is idle, the queue
  is empty, and the queue is not paused.
- Otherwise it enqueues and returns `{ queued: true, queue_id, position, started }`.
- The office queue worker (`office_queue_loop`, every 2 s) claims the oldest
  `pending` row transactionally and starts it.
- A turn's completion marks its queue row `done` or `failed`; the worker starts
  the next row.
- Heartbeat autonomy yields while a user queue is pending, so an autonomous
  cycle cannot jump ahead of a person's instruction.
- Pause/resume is persisted in `office_queue_paused:<room>`; unpausing drains
  immediately.
- Failed rows are dismissible; pending rows are cancellable.

### Surfaces
- Composer queue panel: count, pause/resume, per-item text, attachments count,
  running/failed state, remove.
- `get_office_queue`, `get_office_queue_paused`, `set_office_queue_paused`,
  `cancel_office_queue_item`, `dismiss_office_queue_item`.
- i18n in all six locales.

### Acceptance
- `office_queue_is_fifo_and_pausable` (Rust): ordering, claim, finish, cancel,
  pause round-trip.
- Composer remains usable during a run; a queued message appears in the panel
  and starts after the active turn.
- No orphan user message after a cancel.
- Queue survives app restart.

## P1 — Context usage meter and compact control — done (first version)

### Problem
Compaction was silent. A long office could approach the model window with no
visible signal, and there was no way to request compaction.

### Data flow
- `StreamEvent::Context { bot_id, thread_id, used_tokens, window_tokens, compacted }`
  is emitted after the first and every subsequent compaction in `execute_run`.
- Tokens are an estimate (`estimate_messages_tokens`), not a billing figure;
  the event is labelled as such.
- `Runtime::request_compaction(thread_id)` records a one-shot request. The next
  run on that thread compacts against half the window, then removes the request.
- `compact_thread_context` exposes the request to the UI.

### Surfaces
- `ContextMeter.svelte`: percentage bar, warning at 80%, danger at 95%,
  compacted badge, click to request compaction on the next turn.
- 1:1 threads use the per-thread store. Offices show the latest node's context
  because office nodes run on their own threads.
- `contextUsage.svelte.ts` keeps the newest value per thread across view changes.

### Acceptance
- `manual_compaction_request_is_scoped_to_a_thread` (Rust).
- `ContextMeter.dom.test.ts`: 42% normal, 85% warning, 96% danger, no-event empty.
- A compact request is consumed exactly once by the next run.

### Next for this track
- Persist the last context value on the assistant message so a reload shows the
  meter before the next run.
- Show categories (system, tools, memory, messages) once the runtime exposes a
  breakdown.
- Add a "Compacted 84k → 31k" confirmation event from the runtime.

## P1 — Image gallery and audio/video attachments — done (first version)

### Problem
The lightbox handles one image at a time. OpenBot has a justified gallery,
generated-image cards, and audio/video attachment cards.

### Data model
- Extend `AttachmentInput`/`Attachment` with `preview_kind` and optional
  `poster`/`duration_ms`.
- Add attachment MIME allowlists for `audio/*` and `video/*`; keep the existing
  32 MB preview cap and workspace confinement.
- Generated images from a tool result become an `Attachment` with
  `presentation: "generated"`.

### Surfaces
1. `ImageGallery.svelte`: justified rows from natural aspect ratios, 2+ images,
   click opens the existing lightbox at that index.
2. `MediaCard.svelte`: video with poster/controls, audio with waveform or title,
   download/open, duration and size.
3. `ImageGeneration.svelte`: generating / completed / failed / interrupted
   states, prompt, aspect ratio, download.
4. Composer attachment chips for audio/video with duration/size.

### Status
- `ImageGallery.svelte` lays out one tile per image and opens the lightbox at
  the clicked index.
- The lightbox steps through a gallery with buttons and arrow keys.
- `ThreadView` and `ChatRoomView` render audio and video attachments with
  controls; the composer caps inline attachments at 24 MB.
- Generated-image cards with generating / failed states remain **next**.

### Acceptance
- Multiple images keep their aspect ratios and never overflow at 620 px.
- A failed image tile offers download, not a blank box.
- Audio/video cards do not autoplay; controls are keyboard reachable.
- `Visuals.dom.test.ts` covers gallery dispatch and lightbox stepping.

## P2 — Agent step list and trace — done (first version)

### Problem
A turn is a reasoning blob plus tool pills. There is no compact turn summary,
memory-change trail, duration, or trace.

### Data model
- `StreamEvent::Step { thread_id, bot_id, step_id, kind, text, at }` for
  `thinking`, `progress`, `memory`, `subagent`, and `failure`.
- Persist the step list on the assistant message (like `tools`) so reload keeps
  the trace.
- Add `started_at`/`finished_at` facts to the message or run projection.

### Surfaces
- `AgentSteps.svelte`: collapsed one-line summary (`9 steps · 2m 14s`), expandable
  live list capped at ~220 px with glide-to-newest.
- `TraceWaterfall.svelte`: dialog with spans (model, tool, delegation), duration
  bars, failure highlight, and a jump-to-step action.
- Memory steps render saved/updated/forgotten with the previous text struck
  through.

### Status
- `TurnTrace.svelte` replaces the raw tool stack with one summary:
  `N steps · duration`, failure count, expandable timeline, and the existing
  per-tool details.
- The runtime persists `MessageContent.spans`: model rounds, tool calls,
  memory saves, and context compactions, each with a turn-relative start and
  duration. The timeline is proportional to those spans; failures are red.
- Reasoning appears as a step row, with the full ReasoningPanel still above.
- Reload preserves it because the spans are stored on the assistant message.

- `TraceWaterfall.svelte` adds a full-screen dialog with the same spans for
  long turns.
- Delegation spans name the target and child thread; memory spans carry the
  saved fact (or the failure reason) and render as a second detail line in both
  the inline timeline and the dialog.

### Acceptance
- A long turn shows one summary row, not a wall of reasoning.
- The list scrolls internally and never widens the message.
- A failed tool is visible in both the summary and the trace.
- Reload preserves the step list.
- `TurnTrace.dom.test.ts` covers summary, expansion, tool timeline,
  model/tool/memory span rendering, and the full trace dialog.
- `trace_span_tests` and `trace_spans_are_capped_and_mark_failures` cover
  persistence shape, legacy decoding, failure flags, and the cap.
- `tool_span_description` tests cover delegation target/thread details,
  failed-delegation reasons, saved memory facts, and plain tool spans.

## P2 — Persistent browser dock — done

### Problem
Browser/computer tools run invisibly. There is no watchable, take-over-able
surface.

### Architecture
- The dock is a **native child webview**, not an iframe. Remote sites set
  `X-Frame-Options`/`frame-ancestors`, and the app's CSP is not theirs to
  relax.
- `BrowserDock.svelte` renders the chrome and a measured placeholder. The
  backend creates the webview at that rectangle; a `ResizeObserver` keeps the
  native bounds in sync with the grid.
- Closing the panel **hides** the webview. History, cookies, scroll position,
  and logins survive, and reopening restores the page without reloading it.
- Back/forward/Reload, a URL bar, the page title, and a close action live in
  the chrome. The URL bar accepts bare hosts and turns other input into a
  DuckDuckGo search.
- `browser_open` is a separate skill from `browser_navigate`. It asks the
  shell to show a real page (`ravenbot_core::request_browser_dock`), while
  `browser_navigate` remains the headless fetch/readable-text tool.
- `browser_dom` acts inside the dock: snapshot interactive elements with
  selectors, click, fill, highlight, press, scroll, read text/HTML, navigate
  back/forward/reload, wait, or evaluate JavaScript. On Linux it uses
  WebKitGTK's `run_javascript` with a callback; the command is dispatched on a
  blocking task so `with_webview` never deadlocks the GTK main thread. Other
  platforms return an honest "not available" instead of a fake success.
- Every browser action is published as a small activity event. The dock shows
  the agent name, the action, and its result. Clicked elements flash an
  outline in the page.
- **Take over** pauses agent DOM actions. The hand button focuses the page and
  sets a process-wide flag; both the command and the skill-facing evaluator
  refuse while it is set, so the next agent action fails rather than clicking
  underneath the user. Releasing clears the flag; hiding the dock also releases
  it.
- On Linux, Tauri's `Window::add_child` packs the child into the window's
  vertical box and shrinks the app. The dock instead wraps the window content
  in a `GtkOverlay` and adds the child as an end-aligned overlay child below
  the chrome. Windows and macOS use `add_child` directly.
- Only `http(s)` is loadable; `file:`, `javascript:`, and custom schemes are
  rejected in the backend.

### Surfaces
- Rail globe button and `mod+shift+b`.
- `browser-dock:request` (agent), `browser-dock:page-load`,
  `browser-dock:title`, and `browser-dock:closed` events.
- Commands: `browser_dock_open`, `browser_dock_set_bounds`,
  `browser_dock_navigate`, `browser_dock_back`, `browser_dock_forward`,
  `browser_dock_reload`, `browser_dock_hide`, `browser_dock_destroy`,
  `browser_dock_state`, `browser_dock_eval`.

### Acceptance
- A browser task shows a live page, not only a tool trace.
- Reopening the dock restores the same page without a navigation event.
- A failed navigation shows the webview's error page, not a blank panel.
- `BrowserDock.dom.test.ts` covers closed state, desktop fallback, host
  normalization, and search normalization.
- `browser_dock_tests` cover scheme rejection and bounds clamping.
- `browser_dom::tests` cover selector/text escaping, required fields, and the
  no-evaluator failure.
- `browser_dock_takeover_tests` covers the process-wide flag round trip.
- Live verification: the dock's page-load title read round-tripped through
  `browser_dock_eval` (`DuckDuckGo...`, then `Dock DOM Test`), and navigating
  while taken over produced page-load events but **no** eval request. Releasing
  and reloading resumed the eval (`Example Domain`).

### Remaining
- DOM actions are Linux-only for now. Windows/macOS need their own
  result-bearing evaluation bridge before `browser_dom` can work there.
- The downloads list is in the dock; a workspace-wide download manager remains
  separate.
- Take-over is an explicit release/acquire flag, not a per-action lease; an
  agent action already in flight when the user takes over is not cancelled.
- Tabs and named profiles are still out of scope.
- A page-only screenshot cropped to the dock is not implemented; `computer_control`
  captures the full screen.

See [browser-and-computer.md](browser-and-computer.md) for the full tool
contract, safety gates, and recommended agent flows.

## P3 — Versioned local skills and marketplace — done (library + install)

### Data model
`~/RAVENBOT/skills`:

```text
index.json                         versions, hashes, installed copies
library/<id>/<version>/SKILL.md    immutable version snapshots
installed/<id>/SKILL.md            the active copy agents read
backups/<id>/<timestamp>/          preserved modified copies
```

`SKILL.md` may carry `scripts/`, `references/`, and `assets/`; the tools accept
supporting files as relative-path text. Paths, ids, and versions are validated
against traversal, and symlinks are never followed.

### Tools
- `create_skill`, `revise_skill`, `list_local_skills`, `read_local_skill`,
  `install_local_skill`.
- Revision requires the version the agent read; a stale `base_version` is
  refused with the current version in the message.
- Updating a library skill does not update installed copies; install an exact
  revision.
- Modified installed files are protected. `force=true` backs them up before
  replacing them.
- Registration never executes scripts.

### Surfaces
- Settings → Skills → Local skills: list with installed/latest/modified chips,
  version history, preview, install/rollback, and a replacement confirmation
  when the installed copy has local edits.
- "Try skill" fills the composer with `example-prompt` or a generated
  read-and-follow prompt; it does not send.
- Marketplace install: search the live VoltAgent catalog, fetch the complete
  `SKILL.md` (not the 20k prompt-clipped copy), and import it as a versioned
  local skill. A collision is refused unless **Update** is chosen; an update
  publishes a patch and installs it, and a modified installed copy still
  refuses the overwrite.
- Marketplace publish/submit to a remote registry remains a separate,
  explicit flow.

### Acceptance
- A skill can be created, revised, installed, and rolled back.
- A modified installed copy refuses an overwrite; forcing it leaves a backup.
- The preview shows `example-prompt` from YAML frontmatter and strips the
  header from the rendered body.
- No script runs during registration.
- `local::tests` cover create/revise/install/rollback, stale revision bases,
  modified-copy protection and backups, traversal rejection, supporting files,
  and frontmatter.
- `builtin::local_skills::tests` cover the agent tool path end to end.
- `marketplace_import_creates_then_updates_with_explicit_permission` covers
  first install, collision refusal, update, and unchanged-content detection.
- `catalog_readme_search_filters_and_dedups` covers live catalog parsing,
  sponsor/badge filtering, category labels, and deduplication.
- `LocalSkillsPanel.dom.test.ts` covers installed preview/install and the
  marketplace search/install flow.

## P3 — Team channels with task ownership — done (first version)

### Data model
Migration `028_team_channels.sql`:
- `channel_messages` — the shared timeline, with `human`, `agent`, and
  `system` authors.
- `channel_tasks` — one row per task and exactly one `owner_bot_id`.
- `channel_task_events` — immutable history for created/start/pause/resume/
  reassign/archive/restore/finish/fail.

### Behaviour
- `ChannelTaskQueries::claim` is one conditional `UPDATE ... WHERE state IN
  ('pending','paused')`; two workers cannot both claim a task.
- Reassign updates the owner on the same row, and only while pending/paused.
  Running tasks must be stopped first, so two agents never own one run.
- Archive is refused while running. Archived tasks are readable, are excluded
  from the default board, and can only run after **Restore** (which returns
  them to paused).
- Starting a task creates the owner's thread, links it to the channel, runs
  the agent with the channel instructions, posts the result into the shared
  timeline, and marks the task done/failed.
- Stop sets the row to paused **and** cancels the active run; a late model
  answer is not posted over the Stop.
- A failure before the run is spawned marks the task failed rather than
  leaving a phantom `running` row.

### Surfaces
- Rail → **Channels**: channel list, shared timeline with Markdown, composer,
  and a task board with Start / Stop / Reassign / Archive / Restore.
- Task cards show owner, state, result, and history-driven state transitions.
- `ChannelsPanel` remains the roster/instructions editor; Save now accepts the
  partial channel payload the list endpoint returns.

### Acceptance
- Only one owner can run a channel task at a time.
- Reassign transfers ownership without duplicating the task.
- Archived tasks remain readable and cannot be resumed without restore.
- Channel history and task events survive restart.
- `channel_task_tests` cover message ordering, the atomic claim, reassignment,
  archive/restore, and the running-task guards.
- `TeamChannelsView.dom.test.ts` covers the timeline, task board, start,
  archive, and human composer.
- Live verification: a task run completed and its answer appeared in the
  channel timeline.

### Remaining
- Agent-to-agent replies/reactions and shared file cards.
- A channel-wide "ask the lead" path that fans a human message out under the
  responder rules; tasks are the current agent entry point.

## Rendering and design — done

The chat renderers and the export path are now one contract, documented in
`docs/rendering-and-design-stack.md`.

- `chart` — native responsive SVG (bar, line, area, pie, donut, scatter),
  negative values, stacking, themes, accessible summary, and SVG download.
- `slides` — keyboard navigation (arrows/Home/End), fullscreen present mode,
  progress, and a designed stage. The `slides` workflow covers the argument,
  palette, typography, and no-slop rules.
- Math — KaTeX for `$...$`, `$$...$$`, `\(...\)`, and `\[...\]`, with a raw
  fallback for invalid LaTeX.
- `make_document` — real DOCX/PDF/PPTX/HTML through Pandoc; PDF uses XeLaTeX,
  PPTX converts `---` slide separators, and exports are verified non-empty.
  Exported files appear as an Open card in the tool trace.
- `make_slides` — a structured deck becomes 1920×1080 PNGs from code-defined
  vector layouts (type, icons, shapes, stats, timelines, charts), then a
  `.pptx` with each image full-bleed on a blank 16:9 slide. The tool trace
  shows thumbnails and an Open PPTX button; Pandoc reads the package back.
- `fframes` — current `cargo fframes new --template/--backend` workflow,
  inspect/strip/frame/onion/audio review, plus an ffmpeg slideshow/captions/
  loudness fallback when cargo-fframes is unavailable.
- Tests: `MathBlock`, `mathDelimiters`, `Visuals` (chart/slide/download/
  keyboard/present), `ToolTraceList` (document and slide-deck cards),
  `documents` (DOCX/PPTX/PDF bytes, traversal, slide separators), and
  `slide_design`/`pptx` (layouts, escaping, 1920×1080 dimensions, full-bleed
  transform, relationships, and a Pandoc round trip).

## P3 — Mobile/web/remote clients — separate track

OpenBot is far ahead here. Do not bolt this onto the desktop renderer.

1. Extract a stable local API (the existing Tauri commands are not a network
   contract).
2. Add auth and a paired-device model before any remote listener.
3. Start with a read-only web client (conversation, board, queue, context).
4. Add write actions behind explicit pairing and CSRF/origin checks.
5. Mobile is a client of the same API, not a second backend.
6. Remote desktop and hosted servers remain a separate security review.

## Dependency order

```text
P0 queue ──► P2 step list (events share the stream route)
P1 context ─┘
P1 media ──► P2 browser (both need a stable attachment/preview contract)
P3 skills ──► P3 marketplace
P3 channels ──► P3 mobile/web (needs a stable team contract)
```

## Guardrails carried forward

- Office agents still never get Allow/Deny prompts; machine-destructive actions
  remain denied by the built-in safety guard.
- Queue items, context requests, and skill installs are scoped to one office or
  thread and survive restart.
- Nothing in this roadmap introduces script execution in the app origin. Visual
  pages stay on `ravenvisual://`, skills are instructions until a tool runs, and
  marketplace packages are data until installed.
