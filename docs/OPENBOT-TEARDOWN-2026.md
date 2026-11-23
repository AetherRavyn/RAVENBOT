# OpenBot teardown and RAVENBOT's next moves — 2026

> **Source studied:** `nightly-labs/openbot` at commit
> `3ec318ef791bf3259d84c3dc326d7c66545bc1ec` (OpenBot **0.65.0**,
> 2026-11-08), cloned to `/tmp/openbot`.
>
> **Compared with:** RAVENBOT at the current working tree (`ee948fc` plus the
> trust pass documented in [`COMPETITIVE-AUDIT-2026.md`](COMPETITIVE-AUDIT-2026.md)).
>
> **Licence note:** OpenBot is PolyForm Noncommercial. This document is a
> product/architecture comparison. No OpenBot source is copied into RAVENBOT.
> Any pattern described here is re-implemented in Svelte and Rust against
> RAVENBOT's own contracts. Where a pattern is genuinely novel, the
> implementation must be a clean-room design, not a translation.
>
> **Scope:** architecture, agent runtime, message rendering, interaction
> primitives, composer, side panel, dynamic island, browser/vault, workflows,
> remote/mobile/team, and UI design discipline. The goal is not parity; it is
> to identify the subset of OpenBot's product craft that makes RAVENBOT
> materially better without losing its sovereignty thesis.

---

## 0. Verdict

OpenBot is no longer "a chat wrapper around Codex". At 0.65.0 it is a deep
desktop workspace with:

- **119 conversation components** in one UI package, plus 32 renderer feature
  groups (browser, team, vault, workflows, remote desktop, mobile client,
  dynamic island, routine flows, speech, usage, servers, updates).
- A provider-adapter layer that owns process groups, session leases, idle
  unload, memory pressure, and provider-specific quirks.
- A real interaction protocol: approvals, questions, generic UI blocks, vault
  requests, browser secret cards, side questions, subagents, and tasks.
- A real side-panel model with pane/thread/file tabs, drag-to-chat, and
  per-agent/per-channel state.
- A remote surface: desktop host + web client + iPhone/Android + WebRTC relay +
  multiplayer teams.

RAVENBOT is **behind on product surface** and **ahead on owned runtime**. The
gap is concentrated in five places:

1. **Interaction primitives** — OpenBot has a generic, validated UI-block
   protocol (`confirm`, `quick_replies`, `choice`, `form`) with lifecycle and
   privilege rules. RAVENBOT's question card is one shape of that idea.
2. **A real right-side workspace** — browser/files/settings/questions/subagents/
   voice as tabs beside the chat, with per-agent state. RAVENBOT still has
   separate panels and docks.
3. **Composer as an editor** — mentions (`@agent`, `$skill`, `#file`, `@tab`),
   per-thread drafts, queue reorder/steer, pending-send state.
4. **Turn anatomy** — step list, task list, awaiting replies, open waits,
   subagent cards, memory-change events, trace waterfall.
5. **Reach and trust polish** — browser tabs/profiles, vault/secret cards,
   dynamic island/tray approvals, workflow canvas, and a UI-foundation gate.

The strategic conclusion from `COMPETITIVE-AUDIT-2026.md` still holds:
**do not out-cloud Grok/Muse, and do not beat OpenBot at being a generic agent
shell.** Beat it on *auditable local execution + accountable multi-agent work +
offline artifacts*. The features below are chosen because they strengthen that
thesis, not because OpenBot has them.

---

## 1. OpenBot architecture — what is actually underneath

### 1.1 Process and package layout

```
src/main/           Electron main process, windows, browser views, IPC, updater
src/backend/        Agent services, provider adapters, workflows, vault, team, sync
src/renderer/       SolidJS renderer (feature groups)
packages/ui/        Shared UI primitives and feature components
packages/contracts/ Runtime-validated contracts shared by desktop/web/mobile
packages/brand/     One token file for every surface
packages/i18n/      Message catalogs
apps/mobile, apps/*-app, remote/*, marketplace/*
```

The important architectural choice is **contracts first**. `packages/contracts`
contains not just types but runtime validators (`ui-blocks.ts`,
`ipc-*.ts`, `team-protocol/*`). The desktop, web client, mobile app, and team
server all validate the same payloads. That is why OpenBot can add a remote
surface without a second product.

RAVENBOT's equivalent is Rust types + Tauri commands + hand-written
`$lib/types.ts`. That is smaller and easier to audit, but it has already caused
drift (`any` payloads, `headers_json` leaking to the renderer, feature fields
read at the wrong JSON level). The lesson is not "adopt TypeScript contracts";
it is **generate or validate the IPC contract in one place**.

### 1.2 Providers are adapters, not the product

OpenBot's `docs/architecture/providers.md` and `src/backend` wrap Codex,
Claude Code, Gemini, Grok, Cursor, Cline, Muse Code, Pi, Mercury, OpenCode,
Copilot, Mistral, Hermes, and ACP agents. The adapter owns:

- process groups and child cleanup (SIGTERM, then SIGKILL after 2s);
- thread/session lifecycle, idle unload, resume;
- provider-specific streaming and tool-call quirks;
- usage limits, backup models, and fallback;
- workspace sandboxing and skill injection.

This is OpenBot's biggest adoption advantage and its biggest architectural
liability: the agent loop belongs to someone else's binary. RAVENBOT owns the
loop, so it should compete on **auditability, replay, policy, and local
reliability**, not on the number of CLI adapters. A subscription bridge can be
an optional plugin later; it should never become the core.

### 1.3 Event model

OpenBot's event architecture (`docs/architecture/events.md`) treats the
conversation as a stream of typed events:

- message deltas and final messages;
- tool calls, progress, memory changes;
- approval requests and decisions;
- questions and answers;
- browser takeover and secure-input requests;
- queue deliveries, steering, and pending sends;
- routine/workflow state;
- usage and context changes.

The renderer is a projection of that event stream. This is why a queued message,
a running tool, a parked approval, and a browser takeover can all be rendered
consistently across desktop, web, and mobile.

RAVENBOT has `StreamEvent` and a run ledger, but its UI still reads a mixture of
event payloads and ad-hoc IPC snapshots. The next architectural investment
should be a **single conversation event contract** that the thread, office,
remote client, and audit log all consume.

### 1.4 Remote, mobile, and team

OpenBot's `docs/web-client.md`, `apps/mobile`, `packages/team-client`, and
`remote/*` implement:

- a desktop host with an encrypted direct connection;
- a web client and iPhone/Android clients;
- WebRTC relay for NAT traversal, plus private-network mode;
- multiplayer with roles and per-user audit;
- hosted/VPS deployment.

This is a multi-quarter product, not a UI feature. RAVENBOT should not attempt
it until the local service API and release trust are fixed.

### 1.5 Browser, vault, and computer use

OpenBot's browser is a first-class managed surface, not a webview with a URL
bar:

- `WebContentsView` tabs, popups adopted into tabs, tab lifetime rules;
- per-agent browser profiles (`persist:openbot-browser-profile-<id>`), named
  profiles, proxies, shared profiles;
- cookie-banner refusal on `dom-ready`, with profile-level persistence;
- secure input cards for passwords/OTP, with a "save to Vault" path;
- agent takeover and user takeover, with input leases;
- page previews and download policy.

The Vault is separate from the browser: OS-encrypted storage, 1Password and
Bitwarden integration, login/card/contact items, and a request card that lets
an agent ask for an item without ever seeing the secret.

RAVENBOT has a real child webview stack, native tabs, DOM actions on Linux,
takeover, per-agent/office profiles, a **user-initiated secure input card**, a
**Refuse cookies** action, and a local vault. The secure value is written
directly into the page and never returned to the model; the cookie action
clicks reject/only-necessary consent buttons. Automatic cookie refusal is an
opt-in global setting that uses a conservative consent-dialog detector. It does
**not** yet have proxies or per-profile cookie policy overrides.

### 1.6 Workflows and routines

OpenBot's workflow system (`docs/architecture/agents.md` §Workflows) is a
graph of typed nodes:

- trigger, prompt, asset, LLM, agent, image, condition, approval, output, text,
  note, shape;
- typed ports, branch edges, loops, versions (50 retained), conflict handling;
- schedule/webhook/GitHub triggers;
- approvals and outputs; agent steps run as routine handoffs in ephemeral
  threads;
- the agent can create and edit workflows with six tools; the canvas plays the
  agent's edits with a cursor.

RAVENBOT has cron routines, loopback webhooks, an editable office DAG canvas,
reusable per-office workflow templates with version snapshots and per-version
run history, and an office queue. It does **not** have typed node ports or
loops. This remains a strong differentiator for RAVENBOT's "accountable
offices" wedge, but it must be built on the existing DAG/queue rather than as a
second engine.

### 1.7 Dynamic island and voice

OpenBot's Dynamic Island is an always-on-top desktop surface with compact and
expanded states for idle, working, message, question, approval, takeover, and
failure. It lets the user answer or stop an agent without opening the main
window. Voice uses local STT/TTS in 31 languages.

RAVENBOT has voice (STT/TTS through the configured provider) but no always-on
surface. A tray/always-on-top status window is a smaller, higher-value version
of the island for RAVENBOT: approvals and questions are the events that
actually need attention.

---

## 2. OpenBot's rendering system — the part RAVENBOT should study hardest

### 2.1 Content blocks and bubble variants

OpenBot parses a message body into `MessageContentBlock`s
(`packages/ui/src/features/conversation/DataTable.tsx`,
`MessageRendering.tsx`). The parser distinguishes text, code, tables,
comparison tables, charts, images, and other structured blocks.

`conversationBubbleVariant` chooses the shell from the content:

- user → `secondary`;
- image generation or attachment-only → `ghost`;
- table or comparison table → `muted`;
- any non-text block → `ghost`;
- otherwise → `muted`.

The bubble is a shell; the content owns its layout. Long messages (>20,000
characters) show a 3,000-character preview and a "Show full message" control.
The expanded set survives virtualization, and a message that the reader watched
stream stays open when it ends.

**RAVENBOT status:** already has `msg-bubble-ghost`, a 3,000-character collapse,
content-aware ghost selection (`$lib/chat/grouping.ts`), and a token-based
Markdown renderer. The missing pieces are:

- a named `MessageContentBlock` model that the row, search, copy, and export
  all agree on;
- justified multi-image packing and image-only ghost layout;
- content-block-aware copy/export (copy table as CSV/Markdown, code as code);
- a stable block cache so streaming does not reparse the whole body each step.

**Action:** extract `src/lib/chat/content.ts` with a pure parser and use it for
bubble selection, search, copy, and renderer input. Do not build a second
Markdown parser.

### 2.2 Streaming reveal

OpenBot's `streamingReveal.ts` is a word-boundary pump with:

- a minimum reveal speed (120 chars/s) so the last words of a backlog do not
  crawl;
- a catch-up budget (`--stream-catch-up`, 400ms) so the visible text stays about
  one budget behind the model regardless of burst size;
- per-step reveal chunks and a fade trail, so each revealed word fades on its
  own rather than snapping to full opacity;
- reduced-motion bypass;
- a "revealing" state that keeps a long reply expanded while it types.

**RAVENBOT status:** `src/lib/chat/streamReveal.svelte.ts` and
`revealPump.svelte.ts` already implement a backlog-aware word pump. The gap is
the **fade trail**: RAVENBOT reveals whole words but does not keep per-step
timestamps for a trailing fade. Add a bounded ring of `{length, revealedAt}`
chunks and a CSS `--stream-fade` driven tail. This is a small, high-polish
change.

### 2.3 Selection actions / quote

OpenBot's `SelectionActions.tsx` is a sophisticated text-selection layer:

- a selection must be inside one message block (`data-selection-message-id`);
- it ignores selections that cross interactive elements (links, buttons,
  inputs, citations);
- it positions a floating toolbar above or below the selection, clamped to the
  viewport;
- it installs a persistent highlight via the CSS Custom Highlight API with a
  fallback;
- actions are Reply, Quote, and Copy; the quote shortcut is
  `Cmd/Ctrl+Shift+.`;
- it supports file text (`data-selection-file-quote`) and scoped side threads;
- a pointer-down elsewhere dismisses it without clearing the composer caret.

**RAVENBOT status:** text is selectable (the old `select-none` was removed),
but there is no selection capture, no floating action, no quote-into-composer,
and no file-selection quoting. Replies exist, but they attach to a whole
message, not a passage.

**Action:** add a shared `MessageSelectionToolbar.svelte` and a pure
`src/lib/chat/selection.ts` (`selectionQuote`, `toolbarPosition`) with tests.
Mount it around the transcript in both `ThreadView` and `ChatRoomView`.

**Implemented in this pass:** the toolbar is mounted in both chat surfaces;
`ChatMessageRow` carries `data-message-id`. Reply quotes the source message,
Quote inserts a Markdown blockquote into the draft, Copy copies the passage, and
`Cmd/Ctrl+Shift+.` quotes the selection. Tests cover cross-message and
control-spanning selections, oversized passages, toolbar placement, callbacks,
and the keyboard shortcut.

### 2.4 Sandboxed ChatVisual

OpenBot's `ChatVisual.tsx` renders an agent-authored HTML page in a sandboxed
iframe:

- `allow-scripts`, **no** `allow-same-origin`;
- an opaque origin, so the page cannot read the app;
- a postMessage protocol for height, theme, links, composer text, context,
  fullscreen, selection quote, and key chords;
- every user-acting request must follow a real user gesture;
- the frame stays in the document in fullscreen (top layer), so it does not
  reload.

**RAVENBOT status:** has a `visual` fence served from `ravenvisual://`, a
sandboxed frame, theme variables, and height reporting
(`docs/rendering-and-design-stack.md`). Missing: fullscreen top-layer
preservation, postMessage composer/context/quote actions, a strict
user-gesture gate, and a side-panel "open in panel" path.

**Action:** define a small `ravenvisual` message protocol in Rust/TS and test
it with a fixture. Do not add `allow-same-origin`.

### 2.5 Generic UI blocks — the biggest missing primitive

OpenBot's `packages/contracts/src/ui-blocks.ts` defines a full interaction
protocol:

| Block | Shape | Use |
|---|---|---|
| `confirm` | title, danger flag, hold duration, read-only fields, select fields, safe-Markdown preview, up to 4 actions | irreversible action, "send this?" |
| `quick_replies` | title, up to 8 options, optional text box | one-click answer |
| `choice` | title, single/multiple, up to 20 options with meta/selected, submit label | pick one or several |
| `form` | text/textarea/select/segmented/date fields, required, secret flag | collect structured input |
| hold button | `UiHoldButton` | dangerous confirm requires a hold, not a click |

The protocol has:

- strict size limits and id patterns;
- runtime validation (`validateUiBlockResponse`);
- lifecycle states `pending / answered / expired / closed`;
- fallback questions so a client that does not know a block can still answer;
- privilege rules: `danger`, `confirm`, and a danger confirm's primary button
  are owner/admin-only; a free-text "yes" is also privileged on such a block;
- Vault and API-key adornments (`uiBlock.vault`, `uiBlock.apiKey`).

The UI layer is split into `UiBlockCard`, `UiBlockingBlock`, `UiConfirmBlock`,
`UiChoiceBlock`, `UiFormBlock`, `UiQuickReplies`, and `UiHoldButton`, with a
shared footer/outcome/state renderer.

**RAVENBOT status:** `QuestionCard.svelte` is essentially a `quick_replies`
block with free text. There is no multi-select, form, confirm-with-hold,
lifecycle persistence, fallback questions, privilege model, or shared block
shell. This is the single most leveraged rendering gap because it changes what
an agent can *ask for* and what a user can *safely approve*.

**Action (P0):** add a `blocks` JSON field to `QuestionRequest` (or a new
`InteractionBlock` table), extend `ask_user` with `kind` + `payload`, and build
the four cards on one shell. Persist status and response, validate on both
sides, and gate dangerous actions. Keep `QuestionCard` as the fallback renderer
for old rows. This is a bigger slice than selection quote; it should be the
first feature after the document.

**Implemented in this pass:** migration 036 adds `kind` and `payload` to
questions; `QuestionKind`/`InteractionBlock` validate in
`crates/core/src/question.rs`; `ask_user` accepts the block and returns the
parsed response; `InteractionBlock.svelte` dispatches to
`ConfirmBlock`/`ChoiceBlock`/`FormBlock`/`QuestionCard`; `HoldButton` requires a
real timer hold. Rust and DOM tests cover validation, persistence, confirmation,
multi-select, required fields, and the hold. Vault request cards and
browser-secret cards remain the next layer on this protocol.

### 2.6 Interaction cards beyond questions

OpenBot has distinct cards for:

- tool approval (`ApprovalCard`) with risk, argument summary, and a hold for
  dangerous actions;
- browser secret (`BrowserSecretCard`) with origin, password/OTP, "submit
  once", and "save to Vault";
- Vault request (`VaultRequestCard`) with login/card/contact kinds, masked
  saved values, and a private "the agent never sees this" note;
- MCP sign-in (`McpSignInCard`);
- action marker (`ChatActionMarker`), feedback sent, marketplace suggestion,
  image generation, pending send.

**RAVENBOT status:** has ApprovalCard, QuestionCard, structured confirm/choice/
form blocks, and a good file/image attachment surface. The browser toolbar adds
a user-driven secure password/OTP fill card and a cookie-refusal action. Missing:
a vault-backed browser secret card, MCP sign-in, action markers, pending-send
state, and an approval **hold** for dangerous actions.

### 2.7 Turn anatomy

OpenBot renders a turn as more than a bubble:

- `AgentSteps` — thinking, progress, memory-change, and error steps; one
  collapsible list with durations.
- `TaskList` — a plan card with a progress ring, per-task state (`pending`,
  `active`, `done`, `failed`), and an active-task header when collapsed.
- `AwaitingReplies` — a card above the composer showing which agents were
  asked, whether they are working/replied/failed, and any wait (question,
  approval, takeover, usage limit).
- `Subagents` — a card for delegated work with duration and a trace link.
- `OpenWaits` — outstanding questions/approvals across the agent.
- `TraceWaterfall` — a dialog that shows span timings for the turn.

**RAVENBOT status:** `TurnTrace`, `ToolTraceList`, `TraceWaterfall`,
`ReasoningPanel`, `PlanDag`, `RunTimeline`, `OfficeBoard` exist, which is a good
base. Missing: a unified step list (thinking → tool → memory → answer), a
task-list card with the plan's progress, an awaiting-replies card, a subagent
card, and memory-change events.

**Action:** keep the existing trace components; add a `TurnSteps.svelte` that
consumes the same `StreamEvent`s and renders the four step kinds. Do not create
a second trace store.

**Implemented in this pass:** `TurnTrace` renders model/tool/memory/delegation/
context spans as a step list above the waterfall; the office composer has a
compact `OfficeTaskCard` with live plan progress and node-thread links; and
`AwaitingReplies` names each delegated agent with working/replied/failed state
and opens the child thread. The existing full board and trace dialog remain the
detailed views; no second event store was added.

### 2.8 Transcript infrastructure

OpenBot's transcript is a product in itself:

- virtualizer with stable row keys and smooth-height resizing;
- scroll rail with one tick per user question, reply previews, unloaded-history
  section, and jump animation;
- in-chat search with match tally and navigation;
- message navigation (`Alt+↑/↓`), day markers, new-message tally;
- pending-send status, reply quotes, reactions, attachment cards, image
  lightbox, code blocks with line numbers/copy, data tables, comparison tables,
  charts, Mermaid, KaTeX, file previews, image generation states.

**RAVENBOT status:** has windowing (50/30), in-thread search, reply quotes,
reactions, Alt+↑/↓ navigation, jump-to-latest with count, code/tables/charts/
Mermaid/KaTeX, galleries/lightbox, artifacts. Missing: true virtualization,
scroll rail, day markers, pending-send state, justified image packing, and a
content-block-aware copy/export path.

### 2.9 Composer

OpenBot's `ComposerEditor.tsx` is a contenteditable token editor:

- `@agent`, `$skill`, `#file`, `@tab` mentions with ranked pickers;
- automatic spacing at caret boundaries;
- attachment references and reply previews;
- per-thread drafts, queue, steer/reorder/send-now/run-as-task;
- send shortcut handling, context usage, voice, and error banners.

**RAVENBOT status:** textarea + file/image attachments + voice + reply preview
+ context meter + office queue. Missing: mentions, per-thread draft persistence,
queue reorder/steer, contenteditable token rendering, and a visible pending-send
state.

**Implemented in this pass:** `@agent`, `#file`, and `$skill` mention pickers
in the office composer, plus `#file`/`$skill` in a one-to-one thread; per-thread
and per-office drafts with debounced local persistence and restore-on-switch;
queue up/down reordering persisted by migration 037, plus a Run next action
that promotes a pending message and starts it when the office is idle. An
`@agent` mention passes a recipient id, and the office planner receives an
addressed-to directive while the transcript keeps the typed text. Mid-turn
steer is implemented for 1:1 runs: while a tool is executing, a new message
joins the next model call and is persisted in the transcript. The office queue
still starts after the active office turn; steering a queued office instruction
into the running graph remains a later layer.

### 2.10 Side panel

OpenBot's side panel (`docs/architecture/side-panel.md`) is a tabbed workspace:

- panes: browser, files, settings, question, voice, subagent;
- thread tabs and file tabs alongside panes;
- Add menu for closed panes; New tab while Browser is open;
- drag a thread tab to the chat header; reorder tabs; close with Delete;
- maximize/restore; per-agent/per-channel state; collapse keeps tabs.

**RAVENBOT status:** separate `ArtifactPanel`, `BrowserDock`, `ComputerPanel`,
`OfficeMemoryPanel`, and settings surfaces. There is no unified tab model, no
drag-to-chat, no per-agent panel state.

**Action:** build a `SidePanel` shell with a pane registry and tab strip, then
mount the existing components inside it. Start with three panes: Browser, Files,
Artifacts. Add Settings/Questions/Subagents after the block protocol lands.

**Implemented in this pass:** `SidePanel.svelte` + `sidePanel.svelte.ts` mount
the existing BrowserDock, WorkspaceBrowser, AttentionInbox, and ArtifactPanel as
four tabs in one right-hand column. The active tab is remembered per
agent/office; the rail browser button and `Mod+Shift+B` open the panel;
BrowserDock stays mounted so an agent's `browser-dock:request` still brings the
browser tab forward. Artifacts opened from code blocks now set a global current
artifact and open the Artifacts tab instead of splitting the conversation. The
Settings/Questions/Subagents panes remain future additions because their
existing components are conversation-scoped rather than global.

### 2.11 Dynamic island / always-on attention

OpenBot's island is a small always-on-top window with compact/expanded states
and actions for message/question/approval/takeover/failure. It is the
"assistant is alive" surface.

**RAVENBOT status:** none. A full island is optional; a **tray/always-on-top
attention window** for approvals and questions is enough to close the gap and
matches RAVENBOT's safety story. Do this after the global HITL inbox.

### 2.12 UI design discipline

OpenBot's `docs/ui-foundation.md` and `scripts/ui-foundation-check.ts` enforce:

- one token file (`packages/brand/src/tokens.css`) for every surface, light
  and dark; no palette declarations elsewhere;
- control scale 24 / 28 / 32 / 36 / 40px;
- primitives only: no native `<button>`, `<input>`, `<textarea>`, `<select>`
  outside `components/ui`;
- no direct Kobo/Kobalte/Lucide imports outside the primitive layer;
- no colour literals outside the palette;
- no untokenized `font-size`, `border-radius`, or `transition-duration`;
- no unreachable CSS classes;
- a ratchet on hand-rolled composite ARIA roles and `data-testid` hooks;
- Storybook for shared components and a11y checks;
- motion under 300ms, hover only on fine pointers, press scale 0.97, reduced
  motion respected.

**RAVENBOT status:** has `tokens.css`, a type-scale test, a component-style
test, reduced-motion utilities, a11y focus handling, and a Svelte-aware UI
foundation gate and axe checks on core components. Missing: Storybook and a formal contrast audit across screens.

**Action (shipped):** `scripts/check-ui-foundation.mjs` hard-fails raw hex in
Svelte styles/inline styles, `!important`, and off-scale text, and ratchets
native `<select>` use outside `ui/` against a checked-in baseline. It runs in
`npm run check` and CI.

---

## 3. Feature and rendering gap matrix

Legend: **L** = RAVENBOT leads · **P** = parity / both credible · **B** =
OpenBot materially ahead · **A** = RAVENBOT absent.

### 3.1 Agent and work surface

| Axis | RAVENBOT | OpenBot | Status | Priority |
|---|---|---|---|---|
| Owned agent loop | Rust runtime owns tool loop, approvals, compaction, checkpoints | Wraps provider CLIs | **L** | protect |
| Provider breadth | 20 providers, custom endpoints, Ollama | CLI adapters + existing subscriptions + Ollama | **P/B** | optional bridge |
| Multi-agent offices | Planner, DAG, parallel nodes, board, shared memory, queue | Channels, teams, handoffs, group chat | **P** | improve |
| Delegation | Real `delegate`, depth guard, per-node threads | Channels, agent-to-agent messages, task ownership | **P** | awaiting card |
| Workflows | Reusable DAG templates with versions, conditional branches, explicit input ports, per-task retries, per-task JSON Schema outputs, bounded per-task loops, multi-task loop groups, explicit trigger/agent/LLM/approval/output node types, four built-in editable templates, one-click, scheduled and loopback webhook-triggered runs, live run history, cron routines, office queue; no GitHub triggers or agent/job marketplace | Typed canvas, versions, loops, schedule/webhook/GitHub triggers | **P/B** | P1 |
| Job library / templates | Four built-in editable workflow templates plus skills-first marketplace and blank agent creation; no agent-template gallery or job marketplace | Agent templates + marketplace + job-shaped agents | **P/B** | P1 |
| Tasks inbox | Cross-fleet Inbox for approvals, questions, office queue, configurable budget alerts, opt-in desktop notifications, HTTPS webhook alerts, and SMTP email alerts; per-office board | Tasks page, ownership, approvals, task conversations | **P** | P0 |
| Approvals | Allow/Deny + risk; forced `Full` was removed | Approval ladder, hold-to-confirm, auto-review | **P** | P0 blocks |
| Questions | Four structured blocks + original question card | Four generic blocks + vault/API-key forms | **P** | P0 |
| Audit log | Global filtered/paginated viewer over model rounds and tool calls; arguments redacted; thread hand-off; signed Ed25519 export, in-app verification, explicit and opt-in automatic retention, scheduled signed backups | Activity log, per-user audit | **P** | P0 |
| Memory | Per-bot/office RAG, self-review, editable/deletable memories, source/thread/run provenance, near-duplicate detection with previewed removal, LLM summarization with editable preview and atomic replacement, opt-in Ollama/OpenAI semantic embeddings with atomic reindexing, fixed 50-fact Recall@5/MRR benchmark, `MemoryChanged` and `MemoryEmbeddingsUpdated` audit events; default remains hash | Editable memories, activity log, memory-change events | **P** | P1 |
| Artifacts | DOCX/PDF/PPTX/HTML, designed decks, charts | File previews, charts, visual replies, exports | **P/L** | protect |

### 3.2 Rendering and interaction

| Axis | RAVENBOT | OpenBot | Status | Priority |
|---|---|---|---|---|
| Bubble variants | user/agent/ghost, content-aware | 7 variants + content-block selection | **P** | P2 |
| Long message | >3,000 chars, 520px fold | >20,000 chars, 3,000 preview, survives virtualization | **P** | P2 |
| Streaming reveal | Backlog-aware word pump | Budgeted reveal + per-step fade trail | **P** | P2 |
| Selection quote | Floating Reply/Quote/Copy + shortcut | Floating Reply/Quote/Copy, persistent highlight, file quotes | **P** | P0 |
| Generic UI blocks | confirm/choice/form/hold + question fallback | confirm/quick replies/choice/form/hold | **P** | P0 |
| Vault/secret cards | Vault request + browser secret fill + save-to-vault + vault import | Vault request + browser secret + save-to-vault | **P** | P0/P1 |
| MCP sign-in card | Connector credentials form | Dedicated sign-in card | **B** | P2 |
| Turn steps | Step list + trace + run timeline | Step list, task list, subagents, open waits | **P** | P0/P1 |
| Awaiting replies | Card with state per delegated agent | Card with state per agent and wait reason | **P** | P0 |
| Transcript virtualization | Cursor-paginated loading, bounded 50/30 render window, off-screen `content-visibility`, and cross-page reply targets | True virtualizer | **P** | P1 |
| Scroll rail | User-message ticks plus a viewport band; click to jump. No question/reply ticks or history-gap markers yet | Question ticks, reply previews, history gap | **P** | P2 |
| Day markers / pending send | Local-day dividers and a Sending… state on optimistic user rows | Both | **P** | P2 |
| Content block copy/export | Markdown/CSV for tables, code copy | Block-aware copy | **P** | P2 |
| Side panel tabs | Browser/Files/Inbox/Artifacts/Vault tabs, per-scope state | Pane/thread/file tabs, drag-to-chat | **P** | P0 |
| Composer mentions | @agent/#file/$skill pickers | @agent/$skill/#file/@tab | **P** | P0 |
| Draft persistence | Per-thread and per-office | Per-thread drafts | **P** | P0 |
| Queue reorder/steer | Reorder + pause/cancel | Reorder, steer, send now, run as task | **P/B** | P1 |
| Dynamic island | None | Always-on-top compact/expanded attention | **A** | P2 |
| Voice | Provider STT/TTS | Local 31-language STT/TTS, voice pane | **P/B** | P2 |

### 3.3 Platform, trust, and reach

| Axis | RAVENBOT | OpenBot | Status | Priority |
|---|---|---|---|---|
| Browser tabs/profiles | Native tabs (up to 8) with per-tab webviews/history, reorder and restart restore; automatic per-agent/office profiles plus named shared profiles; opt-in automatic cookie refusal; no proxies | Tabs, named profiles, proxies, shared profiles | **P/B** | P1 |
| Cookie refusal | Manual action plus opt-in automatic refusal scoped to consent dialogs; global default with per-named-profile overrides | Automatic reject on `dom-ready`, persisted per profile | **P** | P1 |
| Secure input | User password/OTP fill card, origin display, origin-checked save-to-vault | Password/OTP card, save-to-vault, origin display | **P** | P1 |
| Vault | Local Vault tab + `ask_user` vault request card; Bitwarden JSON/CSV + 1Password/generic CSV import with metadata-only preview; secrets in OS keychain, metadata only in SQLite, browser fill returns no secret to the model | OS-encrypted vault, native 1Password/Bitwarden integration, request cards | **P/B** | P0 trust |
| Remote/mobile | None | Web + iOS/Android + WebRTC + private network | **A** | P1 (after API) |
| Multiplayer/roles | One human | Roles, per-user audit, shared host | **B** | P2 |
| Chat apps | MCP connector cards | Slack/Discord/Telegram/WhatsApp apps | **B** | P2 |
| Packaging | Linux/macOS/Windows, unsigned | Signed desktop builds, updates, hosted option | **B** | P0 trust |
| i18n/a11y | 6 locales, good focus/reduced motion, axe checks on core components | 15 locales, a11y checks, Storybook | **B** | P1 |
| UI foundation gate | Type-scale test plus a Svelte-aware token/native-select gate with a checked-in baseline | Token/native/ARIA/unreachable-CSS gate | **P** | P1 |

---

## 4. What to build next — ordered by leverage, not by copying

### P0 — the next two sprints

#### P0.1 Generic interaction blocks

**Why:** it changes what an agent can ask, and what a user can safely approve.
It is also the foundation for vault requests, browser secrets, workflow
approvals, and the HITL inbox.

**Scope:**

- Contract: `InteractionBlock` enum (`confirm`, `quick_replies`, `choice`,
  `form`) with size limits and a validator. Store the block in the question row
  as `kind` + `payload_json` (migration), or a new `interaction_blocks` table.
- Runtime: `ask_user` accepts `kind` and `payload`; validates before persisting;
  emits the block in `QuestionAsked`; returns the serialized response as the
  tool result. Keep the old `question/options/allow_custom` fields for
  compatibility.
- UI: one `InteractionBlock.svelte` dispatcher and four cards; a shared
  `BlockShell` with header/status/footer; a `HoldButton` for dangerous actions.
- State: `pending / answered / expired / closed`; disable all controls while
  deciding; show the chosen outcome when frozen.
- Safety: dangerous actions require hold; `confirm` fields are read-only unless
  a select; never render a field as HTML; secret fields never persist.
- Tests: Rust round-trip + validation; frontend DOM tests for each block,
  double-click, escape, disabled state, and frozen outcome.

**Acceptance:** an agent can ask a 4-field form and receive a validated JSON
answer; a dangerous confirm cannot be accepted by a single click; a blocked
question does not lose its state across a reload.

#### P0.2 Unified attention inbox

**Why:** the office queue, approvals, and questions are spread across
surfaces. A fleet product needs one "what is waiting on me" list.

**Scope:** a `list_pending_interactions` command that returns approvals,
questions, and queued office items with bot/office/thread labels. A sidebar
badge and a `Tasks`/`Inbox` pane with filters (`needs you`, `running`, `failed`),
one-click open, and retry-on-decide failure. No new table: aggregate existing
queries.

**Acceptance:** a question parked on a child thread appears in the inbox; open
navigates to the owning thread; answering from the inbox resumes the run;
a failed decide keeps the card and shows an error.

**Implemented in this pass:** the Inbox rail destination, `AttentionInbox.svelte`,
the `attention` store, the `list_attention_items` command, and thread-focus
hand-off. Approvals and questions open the owner's exact thread; queued items
open their office. The rail carries the live count and the inbox refreshes from
stream events plus a 30-second poll. Inline decision cards remain in the owning
conversation, where the run context is complete; the inbox is the map, not a
second execution engine.

#### P0.3 Composer mentions + drafts + queue control

**Why:** the composer is the main input; this is the highest-frequency UX gap.

**Scope:**

- `@` agent picker in office rooms; insert a stable token and a visible chip.
  Routing: `send_to_chatroom` accepts `recipient_bot_id`; the lead delegates or
  the message is queued for that specialist. Start with "route to specialist"
  and keep the lead in the loop.
- `#` file picker over the office workspace; insert a file reference chip.
- `$` skill picker over the bot's enabled skills; insert a skill reference.
- Per-thread drafts in `localStorage` keyed by `botId/threadId`, with a 500ms
  debounce, restore on switch, clear on send.
- Queue rows: drag handle + ArrowUp/Down, `move_office_queue_item` command;
  `steer` sends the queued text into the running turn when the provider allows.

**Acceptance:** switching threads restores each draft; a mention reaches the
named specialist and the transcript shows who was addressed; reordering the
queue changes execution order and survives reload.

**Implemented in this pass:** drafts are keyed by `bot/thread` and by office;
mentions are inserted by keyboard or pointer and the office lead receives the
addressed-to member in its planning context; queue positions are persisted,
can be moved with the arrow controls, and a pending item can be promoted with
Run next. Direct specialist dispatch is implemented, including for queued
messages: the addressed agent runs first and the lead still synthesizes.
Mid-turn steer is implemented for 1:1 runs while a tool is executing; the
office queue still starts after the active office turn.

#### P0.4 Selection quote and reply-to-passage

**Why:** makes long agent reports usable and is a small, visible craft win.

**Scope:** as in §2.3. One shared toolbar, pure selection helper, tests.

**Acceptance:** selecting text in an agent message shows Reply/Quote; Quote
inserts a bounded excerpt above the composer; sending includes the excerpt;
Escape dismisses; keyboard selection works; no toolbar over links/buttons.

#### P0.5 Turn anatomy: steps, tasks, awaits

**Why:** a long agent turn currently reads as a bubble. Users cannot tell what
is happening, what remains, or who owes a reply.

**Scope:**

- `TurnSteps.svelte` consuming the existing stream events: thinking, progress,
  tool, memory, answer. Collapsible, with duration.
- `TaskList` card from the office plan (`plan_ready` / node events): progress
  ring, pending/active/done/failed, active label when collapsed.
- `AwaitingReplies` card above the office composer: one row per specialist,
  state, wait reason, open/stop/steer action.
- `SubagentCard` for delegation, linking to the child thread.
- Memory-change events: emit a `memory` step when self-review saves a fact.

**Acceptance:** a 10-minute office turn shows a live step list, a task card with
the current node, and a wait row for each specialist; no duplicate trace.

### P1 — the next quarter

1. **Side panel shell** (Browser/Files/Artifacts first; then Settings,
   Questions, Subagents, Voice). Tabs, Add, maximize, per-agent state.
   **Done:** Browser/Files/Inbox with per-scope tabs; artifact and conversation
   panes remain.
2. **Browser proxies**; tabs, profiles, secure input cards, vault, and automatic cookie refusal are in place.
3. **Vault** (OS keychain/encrypted fallback) and vault request cards; migrate
   provider/MCP/plugin secrets and delete plaintext.
4. **Workflow canvas v2** on top of saved DAG templates: trigger, agent, LLM,
   approval, output. Conditional branches, explicit input ports, typed
   JSON-schema outputs, bounded per-task loops, versions, retries, and run
   history are in place; richer node types remain.
5. **Global HITL/tray attention window**, desktop notifications, and remote webhook alerts. A tray icon keeps the fleet alive with the window hidden; the always-on-top island remains open.
6. **Transcript virtualizer.** Day markers, pending-send state, and the scroll rail are in place.
7. **Storybook + axe check.** The type-scale/UI foundation gate and axe checks on core components are in place; Storybook and a broader screen audit remain.

### P2 — polish and expansion

- Dynamic island / always-on-top agent status.
- True local STT/TTS with language selection.
- Justified image gallery packing; image-generation states.
- Content-block copy/export and richer tables.
- Chat apps (Slack/Discord/Telegram) and remote/mobile, only after the local
  API and release trust.

---

## 5. What to copy, what not to copy

### Copy the product principle, not the implementation

- **Copy:** generic interaction blocks, selection quote, side-panel tabs,
  awaiting-replies cards, task-list cards, checkout-style approval holds,
  browser profiles/secure input, vault request cards, UI-foundation gate.
- **Do not copy:** provider-CLI dependency as the core, cloud-only execution,
  forced `Full` approval, a dynamic island as a gimmick without a HITL inbox,
  or a job marketplace measured by count.

### Protect RAVENBOT's differences

- **Owned loop:** every tool call, approval, and run is recorded in RAVENBOT's
  own ledger. That is the basis for replay, audit, and policy.
- **Local sovereignty:** no account, no telemetry, no required server. The
  vault and browser profiles must be local-first.
- **Accountable offices:** DAG planning, per-node threads, shared workspace,
  board, queue, per-office cost. Add replay and office-level alerts, not just
  more agents.
- **Offline artifact studio:** DOCX/PDF/PPTX/HTML, designed decks, charts,
  Mermaid, KaTeX. Add a library/versioning UI and brand kits.
- **Local connector fabric:** 135 MCP connectors, OpenAPI directory, skills,
  own MCP server. Add verified manifests and per-operation scopes.

### The sentence to protect

> **RAVENBOT is the agent workspace you can audit, run offline, and hand real
> work to — with a team of agents, not one chat.**

If a feature does not strengthen that sentence, it is not the next feature.

---

## 6. Two-week execution plan

| Day | Work | Files | Proof |
|---|---|---|---|
| 1 | **Done** — selection helper + position math | `src/lib/chat/selection.ts`, tests | unit tests |
| 2 | **Done** — selection toolbar in both chats | `MessageSelectionToolbar.svelte`, `ThreadView`, `ChatRoomView`, `ChatMessageRow` | DOM tests |
| 3 | **Done** — quote sends a blockquote + reply target | `ThreadView`, `ChatRoomView`, `i18n` | DOM test |
| 4–5 | **Done** — interaction block contract + migration | `crates/core`, `crates/db`, `crates/runtime` | Rust tests |
| 6–7 | **Done** — block UI shell + four cards | `src/lib/components/chat/*Block*.svelte` | DOM tests |
| 8 | **Done** — inbox aggregation command + pane | `src-tauri`, `AttentionInbox.svelte`, `attention.svelte.ts` | Rust + DOM |
| 9 | **Done** — composer drafts + mention pickers | `drafts.ts`, `mentions.ts`, both chats | unit + DOM tests |
| 10 | **Done** — queue reorder | `ChatRoomView`, migration 037, Tauri command | Rust + DOM |
| 11 | **Done** — turn steps + task card | `TurnTrace.svelte`, `OfficeTaskCard.svelte` | DOM tests |
| 12 | **Done** — awaiting replies + child-thread links | `AwaitingReplies.svelte` | DOM tests |
| 13 | a11y/keyboard/reduced-motion pass | all new surfaces | `npm test`, manual |
| 14 | Full verification + doc update | workspace | Rust + frontend + Tauri build |

**Do not start** the workflow canvas or remote/mobile in this window. They are
P1 and each is a multi-week epic. Keep the interaction/rendering layer green.

---

## 7. Verification and metrics

### Per-slice checks

- `cargo test --workspace` (no regressions; new Rust tests for every contract
  and persistence change).
- `npm run check` (0 errors, 0 warnings).
- `npm test` (new DOM tests for every card and toolbar).
- `npx tauri build --debug --no-bundle`.
- Manual: keyboard-only path, reduced motion, 900px width, light theme, screen
  reader labels, double-click on every decision, reload with a parked block.

### Product metrics

| Metric | Target |
|---|---|
| Fresh install → first answer (Ollama present) | < 5 min, 0 API keys |
| Pending interactions visible in inbox | 100% |
| Approval/question survives reload | 100% |
| Double-submit / double-decide | 0 |
| High-risk block requires hold | 100% |
| Draft restored after thread switch | 100% |
| Mention routed to the named specialist | 100% |
| Queue reorder persists | 100% |
| New UI strings localized | 100% of new surfaces |
| Rust / frontend tests | 0 regressions |

---

## 8. Bottom line

OpenBot's lead is not a single feature. It is **product density**: a validated
interaction contract, a tabbed side workspace, a composer that knows about
agents/files/skills, and a transcript that explains a turn. RAVENBOT already
has the harder half — an owned runtime, offices, local tools, artifacts, and a
test suite.

The next move is therefore not "add 100 connectors" or "build a cloud". It is:

1. **Make the agent able to ask for anything** (generic blocks).
2. **Make every wait visible in one place** (inbox).
3. **Make the composer a workspace** (mentions, drafts, queue).
4. **Make a turn explain itself** (steps, tasks, awaits).
5. **Make the shell feel like one product** (side panel, selection quote,
   UI foundation gate).

That is the shortest path from "impressive local agent workbench" to "the best
agent workspace", and it keeps the sovereignty thesis intact.

---

### Appendix — OpenBot evidence index

| Area | OpenBot path |
|---|---|
| Architecture docs | `docs/architecture/{agents,browser,events,providers,servers,side-panel,skills-and-plugins,structure,team-api}.md` |
| UI foundation | `docs/ui-foundation.md`, `packages/brand/src/tokens.css`, `scripts/ui-foundation-check.ts` |
| Message rendering | `packages/ui/src/features/conversation/{MessageRendering,ChatMessageRow,DataTable}.tsx` |
| Streaming | `packages/ui/src/features/conversation/streamingReveal.ts` |
| Selection | `packages/ui/src/features/conversation/SelectionActions.tsx` |
| UI blocks | `packages/contracts/src/ui-blocks.ts`, `packages/ui/src/features/conversation/ui-blocks/*` |
| Turn anatomy | `packages/ui/src/features/conversation/{AgentSteps,TaskList,AwaitingReplies,Subagents,OpenWaits,TraceWaterfall}.tsx` |
| Composer | `packages/ui/src/features/conversation/{ComposerEditor,QueuePanel,ComposerSendControl}.tsx` |
| Side panel | `docs/architecture/side-panel.md`, `packages/ui/src/features/conversation/SidePanel.tsx` |
| Browser/vault | `docs/architecture/browser.md`, `packages/ui/src/features/conversation/{BrowserSecretCard,BrowserPreviewSidebar}.tsx`, `packages/ui/src/features/vault/*` |
| Workflows | `docs/architecture/agents.md` §Workflows, `packages/ui/src/features/workflows/*` |
| Dynamic island | `packages/ui/src/features/dynamic-island/*`, `docs/architecture/channels-and-messaging.md` |
| Remote/mobile | `docs/web-client.md`, `apps/mobile`, `packages/team-client`, `remote/*` |
