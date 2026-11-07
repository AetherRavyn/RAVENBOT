# OpenBot comparison and RAVENBOT roadmap

Reviewed from the `nightly-labs/openbot` source (SolidJS renderer, `packages/ui`,
`packages/contracts`, and the Electron backend), not only its README. OpenBot is
PolyForm Noncommercial, so this is a design comparison; no code was copied.

> For the current user-facing UI comparison — global search, first run, tasks,
> usage, sidebar organization, and the remaining priorities — see
> [openbot-ui-gap-analysis.md](openbot-ui-gap-analysis.md). This file remains the
> architecture and runtime comparison.

## What OpenBot does exceptionally well

### 1. Interactive visual replies
An agent publishes a self-contained HTML page (`visual-reply:<height>`), not
just Markdown. The page runs its scripts on a separate origin in a sandboxed
frame and speaks JSON-RPC over `postMessage`:

- reports its height so the message grows with the content;
- receives the app theme as CSS variables;
- asks the app to open links, fill the window, put text in the composer, or
  keep page state for the next message;
- refuses requests that were not preceded by a real user gesture;
- has no `allow-same-origin`, popups, downloads, or top navigation.

This is the feature that makes OpenBot feel like a workspace rather than a chat.
RAVENBOT now has a compatible first version: a `visual` fence served on
`ravenvisual://`, sandboxed with `allow-scripts`, theme-aware, height-aware, and
link-safe.

### 2. Rich message surface
OpenBot’s `ChatMessageRow` and `MessageRendering` combine:

- justified image galleries with a zoomable lightbox, swipe, and keyboard;
- generated-image cards with generating / completed / failed / interrupted
  states, aspect ratio, prompt and download;
- audio and video attachment cards with preview and file references;
- data tables, comparison tables, code blocks with line numbers and copy,
  diagram previews with pan/zoom, and a full-screen dialog;
- reply quotes, reactions, message search, message navigation, and a new
  message tally;
- smooth height resizing and a virtualized timeline.

RAVENBOT now renders charts, slides, Mermaid, interactive visuals, code, tables,
math, and a lightbox for markdown images and image attachments. It also has
quoted replies, reactions, search, and video/audio attachment cards. The
justified multi-image row packing and full timeline virtualization remain.

### 3. Agent steps and traces
OpenBot shows a turn as a list of thinking, progress, memory-change, and
subagent steps, with a duration summary and a trace waterfall dialog. A user can
see *why* a turn took the time it did without reading the raw transcript.

RAVENBOT has a reasoning panel and tool pills. It does not have a unified step
list, memory-change events, duration facts, or a trace dialog.

### 4. Context usage and compaction
A composer ring shows used tokens, window size, per-category usage, the
auto-compaction point, and a compact action. Compaction is visible before the
thread reaches the limit.

RAVENBOT compacts context internally and shows token/cost totals, but not a
context-window meter, categories, or a compaction control.

### 5. Queues and turn control
OpenBot has FIFO per-agent queues with pause, resume, cancel, crash-safe
persistence, and “awaiting replies” / “open waits” surfaces. A user can send the
next instruction while the current turn runs.

RAVENBOT rejects a second office turn while one is active. That is safe, but the
user has nowhere to put the next request. A visible queue is the biggest
functional gap.

### 6. Team channels
OpenBot has shared desktop channels with one task owner, explicit delegation,
shared history, Stop / Resume / Reassign / Archive / Restore, agent-to-agent
messages, replies, reactions, and file transfers.

RAVENBOT has offices with a lead, DAG tasks, delegation, a shared board, and
`Shared/STATUS.md`. It does not have a persistent team channel with per-task
ownership controls.

### 7. Persistent embedded browser and computer use
OpenBot keeps a real browser surface with tabs, profiles, downloads, page
previews, agent takeover, and a separate Computer Use driver.

RAVENBOT now has a persistent browser dock: a real child webview with URL,
back/forward/reload, page title, downloads, session persistence, a
`browser_open` skill, and `browser_dom` snapshot/click/fill/highlight/press/
scroll/read/navigate/eval actions (Linux), with a user Take Over control that
pauses agent actions and refusal at both the command and skill paths. The dock
shows the performing agent and its last action, and clicked elements flash an
outline. The Computer panel has a live preview, a click marker, Take over /
Release, a window list with focus/capture, and an isolated Docker desktop.
Remaining: browser tabs and named profiles, page-only screenshots, per-action
leases, native DOM bridges outside Linux, and Wayland-native window focus.
See [browser-and-computer.md](browser-and-computer.md).

### 8. Local skills and marketplace
OpenBot has a local skill library with `SKILL.md`, scripts/references/assets,
version history, exact-revision installs, protected modified files, preview,
“Try skill”, and a marketplace.

RAVENBOT now has a user-authored, versioned local skill library, marketplace
install from the live VoltAgent catalog, exact-revision install/rollback, and
protected modified copies. Remote marketplace publish/submit remains separate.

### 9. Provider-native sessions
OpenBot drives Codex App Server, Claude Agent SDK, Grok, OpenCode, Gemini,
Cursor, and Cline through their CLIs/ACP, with per-agent sessions, managed
runtime downloads, and context monitoring.

RAVENBOT uses its own provider adapters plus OpenCode Go auto-discovery. This is
a different architecture, not a missing feature; provider-native sessions would
be a large separate track.

### 10. Engineering discipline
OpenBot has Storybook, a `check:ui` design-system gate, i18n linting, type-error
ratchets, browser smoke tests, UI-block tests, CPU/memory baselines, and a
repeatable renderer QA loop. RAVENBOT has strong unit/integration tests and
svelte-check, but no visual regression or design-token lint.

## Implemented in RAVENBOT this pass

- Transcript windowing: the office feed mounts the latest 30 messages (50 in a
  one-to-one thread), auto-loads history at the top with an anchored viewport,
  and keeps a counted "Load earlier messages" control; search and reply jumps
  reveal their target. On the 216-message office this cut initial mount work
  from ~1.7s / 20k nodes to roughly 0.3s / ~2.8k nodes.
- Code blocks highlight lazily within 600px of the viewport, so offscreen
  syntax colouring is not paid for on open.
- Plugins are now real OpenAPI apps: import follows `ai-plugin.json`, parses
  operations, stores plugin-qualified provider-safe ids, applies per-app
  credentials, provider-safe operation ids, tenant server overrides, and
  per-agent scope. The fake `inapp://` success path is removed; placeholder
  rows are retired by migration. The picker now searches the cached APIs.guru
  directory (2,530 apps, OpenAPI 3 and Swagger 2), with category filters,
  pagination, refresh, and a curated starter set. Installing any entry parses
  the real document and prefills the credential header declared by its
  security scheme. Parsed operations keep body encoding and query locations, so
  Swagger 2 form posts and OpenAPI calls with both query and body fields route
  correctly. Apps that declare auth are withheld from the agent's tool list
  until a credential exists.
- The office-open budget has a regression gate: a synthetic 240-message
  transcript must mount exactly 30 rows and stay under 9,000 DOM nodes.
- Board previews are clipped before entering the DOM; a 12KB node result no
  longer sits behind a two-line card.
- Settled office boards open collapsed; a live run expands them automatically.
- Counted new-message tally on the jump-to-latest pill and `Alt+↑` / `Alt+↓`
  message navigation in both the thread and the office feed.
- Document attachment cards with decoded byte size and download, rendered in
  both the thread and office transcripts.
- Single-image messages now keep their aspect ratio; two or more images use a
  compact gallery instead of square crops.
- Quoted replies across one-to-one threads and offices: the quote is resolved
  from the transcript, jumps to the original, survives the office queue, and
  is rejected if it points outside its own conversation.
- Persisted message reactions, stored in `message_reactions` and returned with
  the message, with a palette in the hover metadata row and a one-click remove.
- In-thread search with a match counter, previous/next navigation, wraparound,
  row-level highlighting, and scrolling to the current hit.
- Line-numbered code blocks with a CSS-counter gutter that is sticky on
  horizontal scroll and never enters a copy selection.
- A first-class Paper Light theme: measured canvas luminance drives
  `color-scheme`, light-tuned status/overlay tokens, theme-aware `dark:`
  variants, and preview cards that paint their own ink.
- Shared interaction cards for agent permissions and `ask_user` questions:
  risk tone, readable argument summaries, suggested reply chips, typed answers,
  disabled busy states, and one component used by both thread and office views.
- Table cards with Markdown and RFC 4180 CSV copy actions, plus a bounded
  focusable scroll region.
- Quieter OpenBot-style tool-action rows: status glyph, verb, argument chip, and
  tabular elapsed time.
- `visual` fenced block: interactive HTML with scripts, separate-origin custom
  protocol, theme bridge, height reporting, safe external links, fullscreen,
  source view, and a loading skeleton.
- `chart` JSON renderer: bar, line, area, pie, donut, scatter; themed SVG; no
  external chart runtime; error state and streaming skeleton.
- `slides` Markdown deck with navigation and an internal scroll stage.
- Image lightbox for markdown images and image attachments.
- Media preview in Projects for image, video, audio, and PDF (32 MB cap,
  workspace-confined).
- `visuals` and `fframes` workflow skills, auto-injected by intent.
- `awesome_fetch` now resolves the real `SKILL.md` from the live VoltAgent
  catalog, with a clone hint for skills that need scripts/assets.
- MCP tool collisions are preserved as `<server>__<tool>`; per-bot and per-run
  tool caps were raised.
- Visuals verified in Chromium at 900 px and 620 px, including the production
  bundle, with `scrollWidth === clientWidth`.

## Status of the earlier priorities

Most of the earlier list has shipped:

- **Message queue** — the office queue is persisted, pausable, cancellable, and
  survives restart.
- **Context meter and compaction** — `ContextMeter` shows the estimated window
  and requests compaction on the next turn.
- **Media attachments** — image galleries, lightbox, audio/video cards, and
  generated-image rendering are in place.
- **Agent steps and trace** — `TurnTrace`, `RunTimeline`, and the reasoning
  panel expose model/tool/memory spans with durations and failures.
- **Browser dock** — a real native webview with URL bar, history, take-over,
  and `browser_open` / `browser_dom` actions.
- **Local skill library** — versioned installs, rollback, preview, “Try skill”,
  and the live marketplace.
- **Team channels** — shared timeline and task board with ownership controls.

For the current **user-facing** priorities — tasks inbox, first-run flow, agent
templates, sidebar groups, shortcut help, and file search — see
[openbot-ui-gap-analysis.md](openbot-ui-gap-analysis.md).

## Guardrails

- `visual` pages are always on a separate origin and sandboxed without
  `allow-same-origin`; they never receive the Tauri IPC bridge.
- External links from a visual are validated with `safeHref` and opened through
  the OS opener, never navigated in the app window.
- A visual is capped at 512 KB and the in-memory page store is capped at 96
  entries; the frontend releases a page when its message unmounts.
- Images in the lightbox come from `safeImageSrc`/data attachments, not raw
  model HTML.
