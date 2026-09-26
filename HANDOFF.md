# HANDOFF.md — RAVENBOT UI Rebuild (OpenBot-inspired)

> Living knowledge file for agent sessions. Read this first every time context is lost.
> Last updated: 2026-09-24 (session: analysis + handoff written, no UI code changed yet).

## Mission

Make RAVENBOT into the "real-life office with bots" it describes in README/ROADMAP, with a UI
modeled on **OpenBot** (https://github.com/nightly-labs/openbot, cloned shallow at `/tmp/openbot`
— re-clone if wiped). We **completely clear and rebuild the current UI** (not incremental
restyling — previous commits 451e5c1/0bdc197 were partial sweeps; this time it's a full rebuild)
combining OpenBot's design system + layout patterns with RAVENBOT's own concepts:
Offices (multi-bot teams), 135+ MCP connectors, real memory/RAG, routines, sandbox/kill switch,
fleet sync, own MCP server.

**Hard rule: the Rust backend and its Tauri command contract do NOT change.** The UI rebuild
consumes the existing ~100 `invoke` commands and the single `agent-stream` event (contract below).

---

## 1. OpenBot reference (what we are copying and why)

Stack: **SolidJS 2.0-rc + Tailwind v4 + Kobalte** (Electron). Translates cleanly to Svelte 5:
`createSignal→$state`, `createMemo→$derived`, `props.x` getter → Svelte props proxy, per-domain
context providers → module-level rune state or a `$lib/stores/` with a class/module of runes.

### Layout shell (`/tmp/openbot/src/renderer/src/`)
- `WorkspaceShell.tsx` + `styles/app-shell.css:24` — frame is a **CSS grid**, not flex:
  `grid-template-columns: var(--server-rail-width) var(--left-panel-width,280px) 0 minmax(0,1fr)`,
  `container-type: inline-size`, panel resize animated with `cubic-bezier(0.32,0.72,0,1)`.
- Children: `WorkspaceServerRail` (64px icon rail, far left) → `WorkspaceSidebar` (280px,
  resizable 128–400, auto-compact 88px) → `WorkspaceAccountDock` (bottom-left user chip) →
  middle pane chosen by a **precedence ladder** (one `<Show>`-style switch: channel > DM > agent
  conversation). "What's selected" *is* the router (`navigation.tsx`).
- Readiness gating in ONE place (`AppView.tsx`), not scattered.
- Conversation minimum width: `window − rail − sidebar ≥ 424px` else sidebar auto-compacts.
- Files: `layout.tsx`, `layout-constants.ts` hold geometry constants.

### Design tokens — single source: `/tmp/openbot/packages/brand/src/tokens.css`
Always-dark neutral ("Grok Bot-inspired"):
- canvas `#1a1a1a`, surface `#212121`, raised `#242424`, control-hover `#2a2a2a`,
  glass `rgba(33,33,33,0.76)`. Borders near-invisible: `rgba(255,255,255,0.035–0.045)`.
- Text: `#fff` / `#dcdcdc` / muted `#979797` / dim `#6a6a6a`.
- Accent blue `#007cf7` (hover `#1f8fff`, link `#42a0ff`, soft wash `rgba(0,124,247,0.16)`)
  — already our `--brand`.
- Status as **roles not shades**: success `#31cf76`, warning `#ff9412`, danger `#fd2f3b`, each
  with soft/focus/border alpha steps.
- Provider colors + chart series (`#d6adf2/#d97757/#6bc7d9`) via `data-provider` attr → CSS var.
- Type: Inter Variable, 6-step scale **12/13/14/16/18/24** paired 1:1 with line-heights 16–30px,
  weights 400–700, tracking −0.1/0/0.02em. Base UI density 13px.
- Spacing 2,4,6,8,12,16,24,32. Radii 4/6/8/12/**14 card**/20/**22 bubble**/30 modal/999 pill.
  Control heights 24/28/32/36/40. Icon stroke 1.5 (Lucide 24 viewBox).
- z-index layers: dropdown 40, popover 60, toast 120, dialog 100, dialog-host 160.
- Durations 120/160/200/240ms; ease-out `cubic-bezier(0.23,1,0.32,1)`; hover duration **0ms**.

### Chat patterns (`packages/ui/src/features/`)
- `conversation/ChatMessageRow.tsx` — one shared row component for agent + channel chat;
  "reads no context: everything is a prop". Own messages right-aligned, **no avatar/no name**.
  Agent messages left, avatar + name shown **once per consecutive-author run** (`chat-grouping.ts`),
  continuation rows get a gutter spacer to keep bubbles aligned. Author color via inline
  `--message-author-color` from seeded hue.
- Bubble variants (`primitives.css:2122-2230`, picked in `MessageRendering.tsx`):
  user = `#242424`; agent-with-code/tool = **ghost** (no bg/padding); plain agent = `muted`
  (`#212121`); media-only = ghost. Bubble: `max-width:min(80%,720px)`, radius 22, padding 8/12,
  line-height 1.625. Reactions anchor to opposite bubble corner.
- Tool calls / automation render as `ChatActionMarker.tsx` "Marker" rows (not bubbles):
  24px height, 16px mini-avatar, status labels queued→interrupted.
- Streaming (`MessageRendering.tsx:47-134`): client-side **word-by-word reveal decoupled from
  token arrival** — 60ms gap, smooth height (`createSmoothHeightResize`), per-word tail fade
  (400ms, 2px blur), full `prefersReducedMotion` bypass.
- Queues: `QueuePanel.tsx` — reorderable held-message list, custom vertical-drag engine,
  "steer" action.
- Sidebar (`features/sidebar/`): sections (channels/agents/people), pin/reorder drag. The key
  piece is `SidebarAgentIndicator.tsx` **state machine**: `working`→TypingDots, routine
  `running/queued`→Clock3, `needs-attention`→TriangleAlert, `failed`→X, `responded`→check svg,
  `unread`→count badge (`#74b9ff` bg / `#07131f` text). Empty state = pressed-looking
  "Create your first agent" row with live avatar preview.
- Settings modal Cmd/Ctrl+`,` (`SettingsModal.tsx`). Browser panel with picture-in-picture.
  "Dynamic Island" (`features/dynamic-island/`) for cross-app alerts.

### Motion
Named-token CSS transitions (`styles.css:43-98`, `styles/transitions.css`): digit-roll counters
(500ms, `cubic-bezier(0.34,1.45,0.64,1)`, 70ms stagger), modal scale 0.96→1 @250ms, panel slide
`translateY(100px)`+blur @400ms, input shake with per-leg easing, error auto-revert hold 3s.
Every animation has a `!important` `prefers-reduced-motion` opt-out.

---

## 2. RAVENBOT current state (what exists, what must keep working)

### Routes — SPA, `ssr=false` everywhere (`src/routes/+layout.ts`)
- `+page.svelte` — 1 line, empty. The layout IS the app shell.
- `+layout.svelte` — **740 lines**: all app state, welcome hub (prompt box w/ DeepSearch/Think
  toggles, mic, attach), tab switch bots/offices, mounts everything.

### Components (`src/lib/components/`, sizes matter — the big four must be decomposed in rebuild)
- `ThreadView.svelte` **2309** — chat: threads, messages, streaming, approvals/questions, model
  switcher, voice, regenerate, pause/resume runs, search, ArtifactPanel host.
- `ConnectorCenter.svelte` **1590** — MCP connector marketplace/assignment per bot.
- `ChatRoomView.svelte` **1428** — office rooms: org drafting/provisioning, graph execution, shared thread.
- `McpManager.svelte` **1211** — legacy MCP server manager (probably merge/retire into ConnectorCenter).
- `OfficeSettings.svelte` 1159, `Settings.svelte` 995, `BotSettings.svelte` 909, `Sidebar.svelte` 740.
- Mid: `CreateChatRoom` 391, `ComputerPanel` 366, `RoutinesPanel` 363, `PluginsStore` 303,
  `ChannelsPanel` 290, `CommandPalette` 267, `AvatarPicker` 258, `TitleBar` 243 (Tauri window chrome),
  `SkillManager` 242, `SyncPanel` 262, `ConnectorIcon` 501, `PluginLogo` 207, `TeamImport` 191,
  `OfficeMemoryPanel` 195, `ChatRoomList` 172, `ModelPicker` 174, `ArtifactPanel` 170,
  `MarkdownRenderer` 162, `KillSwitch` 146, `ThemeLogo` 112.
- Small: `Toaster` 41, `AgentIntelligence` 74, `SimpleSelect` 55, `ScreenReader` 33, `FocusTrap` 31,
  `ThemeBackground` 31.
- `ui/` = shadcn-svelte primitives (avatar, badge, button, card, dialog, dropdown-menu, empty,
  input, label, progress, scroll-area, select, separator, skeleton, tabs, textarea, tooltip). Keep.

### THE CONTRACT — backend must not change
- **Single Tauri event: `agent-stream`** (listened in ThreadView.svelte:307, ChatRoomView.svelte:351).
- **~100 `invoke` commands**, grouped (full names preserved here):
  - Core: `create_bot, update_bot, delete_bot, duplicate_bot, list_bots, list_bot_contacts,
    set_bot_pinned, set_bot_hidden, mark_bot_read, get_status, create_thread, list_threads,
    rename_thread, delete_thread, get_chatroom_thread, list_messages, search_messages,
    send_message, edit_and_resend, regenerate_message, send_to_chatroom`
  - Runs/control: `pause_run, resume_run, pause_all, resume_all, get_kill_switch_status,
    trigger_kill_switch, release_kill_switch, set_approval_mode, list_pending_approvals,
    decide_approval, list_pending_questions, answer_question`
  - Providers: `get_model_catalog, get_configured_providers, set_provider_api_key,
    fetch_provider_models, fetch_all_provider_models, get_default_model, set_default_model,
    get_ollama_url, set_ollama_url, probe_ollama, list_engines, list_tts_voices,
    get_bot_budget, set_bot_budget, reset_bot_budget, get_session_usage`
  - Offices: `list_chatrooms, create_chatroom, update_chatroom, delete_chatroom,
    list_chatroom_members, add_member_to_chatroom, remove_chatroom_member, update_chatroom_member,
    create_bot_for_office, default_office_org, draft_office_org, provision_office_org,
    draft_office_plan, execute_graph, list_office_memories, search_office_memories, add_office_memory`
  - MCP: `list_mcp_servers, toggle_mcp_server, save_custom_mcp_server, delete_mcp_server,
    get_mcp_server_env, save_mcp_server_env, test_mcp_server, list_bot_mcp_servers,
    toggle_bot_mcp_server, batch_set_bot_mcp, batch_assign_bot_mcp`
  - Routines/channels/plugins/skills: `list_routines, create_routine, update_routine, delete_routine,
    run_routine_now, get_scheduler_status, get_routine_webhook_status, enable_routine_webhook,
    disable_routine_webhook, list_channels, create_channel, update_channel, delete_channel,
    set_channel_bots, list_plugins, list_bot_plugins, sync_plugins, toggle_bot_plugin,
    import_openapi_plugin, list_all_skills`
  - Computer/voice/intel/sync/team: `capture_screen, computer_capabilities, bot_desktop_status,
    start_bot_desktop, stop_bot_desktop, transcribe_audio, synthesize_speech,
    get_agent_intelligence, list_agent_learnings, get_sandbox_report, export_bot_bundle,
    import_bot_bundle, fetch_and_preview_team, preview_team, import_team`

### Backend architecture digest (16 Rust crates, ~31k lines; do not touch during UI work)
- `crates/runtime/src/lib.rs` (4831) — the agent runtime heart; `orchestrator.rs` routes office
  messages, `graph.rs` parallel DAG execution of specialists, `executor.rs` runs skills.
- `src-tauri/src/lib.rs` (4117) — all Tauri commands + CLI headless mode.
- `crates/models/` — provider abstraction (ollama, anthropic, openai, openrouter, local, …) +
  `streaming.rs` accumulators, `manager.rs`, `discovery.rs`.
- `crates/skills/builtin/` — ~25 built-in skills; `crates/mcp/` — client (135+ catalog in
  `registry.rs`) + our own MCP **server** (`mcp-serve`); `crates/memory/` — vector store, RAG,
  self-review; `crates/sandbox/` — kill switch, resource limits, network policy;
  `crates/scheduler/` — cron routines + webhooks; `crates/sync/` — Ed25519 TOFU bundles;
  `crates/db/` — sqlx sqlite, 17 migrations.

### Frontend infra
- State: **no svelte stores** — pure Svelte 5 runes in components; `+layout.svelte` holds
  bots/chatrooms/selection. Hand-rolled pub/sub modules: `theme.ts` (`subscribeTheme`; OpenBot is
  the default theme; brand decoupled from primary via `buttonHex`), `toast.ts`,
  `model-catalog.ts`. Streaming state in ThreadView/ChatRoomView via `agent-stream`.
- i18n: 6 locales (de/en/es/fr/ja/zh), `t()` with dotted keys — new UI strings need all 6.
- a11y: `trapFocus`, `keyboardShortcuts`, `announce`, `prefersReducedMotion` in `src/lib/a11y/`.
- **CRITICAL CI CONSTRAINT**: `src/lib/styles/no-component-styles.test.ts` fails if ANY `.svelte`
  file contains a `<style>` block (Tailwind v4/Vite crash). ALL component CSS lives in the global
  `src/lib/styles/components.css` (369 lines). The rebuild must obey this.
- `tokens.css` already carries OpenBot values (surface-0 `#1a1a1a`, brand `#007cf7`, radii, status
  colors, 12–24px type scale) — extend/replace it wholesale from the analysis above.
- Tests (vitest, node env, `vitest.config.ts`): `artifact.test.ts`, `i18n.test.ts`,
  `no-component-styles.test.ts`. 18 tests green.
- `types.ts` mirrors `crates/core` Rust types — regenerate/extend if UI needs more shape, but
  never change command names/payloads.
- Custom title bar: `TitleBar.svelte` handles Tauri window drag/resize/min/max/close — keep a
  replacement wired up from day one of the rebuild.

---

## 3. Gap analysis → what the rebuilt UI should add (OpenBot concept × RAVENBOT concept)

| OpenBot pattern | RAVENBOT equivalent to build |
|---|---|
| Server rail (64px) | Rail: Home / Bots / Offices / Connectors / Routines / Settings, plus **global kill-switch indicator** |
| Sidebar agent state machine | Bot rows showing working/needs-attention/failed/unread from `agent-stream` + approvals (`list_pending_approvals`) |
| Precedence-ladder middle pane | Chat pane shared for DM-with-bot and Office room; selection-as-router state in one place |
| ChatMessageRow + grouping + bubble variants | Replace ThreadView's monolith; ghost bubbles for tool markers (already have approvals/questions) |
| Word-reveal streaming | Wrap existing `agent-stream` token events in the decoupled reveal renderer |
| ChatActionMarker rows | Tool-call / skill-run / delegate markers inside threads |
| QueuePanel | Per-bot run queue (we have pause_run/resume_run, budgets — surface them) |
| Dynamic Island | Kill-switch banner + run alerts |
| Account dock / Cmd+, settings | Global settings modal (provider keys, budgets, Ollama probe) |
| Empty-state "create first agent" | Bots/Offices empty states with live preview |
| *(ours, no OpenBot equiv)* | Offices org-graph view (`execute_graph`), ConnectorCenter, memory/RAG panel, sync bundles, voice mode, artifacts canvas |

Retire candidates in rebuild: `McpManager.svelte` (fold into ConnectorCenter),
`SimpleSelect` (use ui/select), scattered legacy panels after new shell lands.

---

## 4. Plan (phases — update this section as work progresses)

- [x] P0: Analyze OpenBot UI + RAVENBOT contract; write this HANDOFF.md.
- [x] P1: Foundation tokens — DONE 2026-09-24. `src/lib/styles/tokens.css` now carries the full
      OpenBot role set: status ladders (success/warning/danger + soft/wash/focus/border),
      question/approval/new badges, `--unread-surface #74b9ff`, washes (`--hover-wash`,
      `--overlay-bg`, `--glass`, `--dialog-*`), file accents, data-table trio, chart series,
      provider colors + `[data-provider]` → `--provider-current` rules, switch geometry,
      chat-marker geometry, extra durations (`--duration-hover: 0ms`, dialog/toast,
      `--agent-activity-exit-delay`, `--stream-gap 60ms`/`--stream-fade 400ms`),
      `--ease-panel` for grid resize, `--layer-dialog-host`, layout constants
      (`--rail-width 64px`, sidebar min/max/compact, `--conversation-min 424px`),
      `--radius-modal` raised 18→30px (OpenBot). theme.ts untouched and compatible (it only
      re-tints core tokens). Vitest: 18/18 green.
- [x] P2: Shell — DONE 2026-09-24. New OpenBot grid frame:
      * `src/lib/workspace.svelte.ts` — selection-as-router rune state (dest, bots, chatrooms,
        selection, sidebar, overlays, init/actions). All panes read it.
      * `src/lib/components/workspace/` — `Workspace.svelte` (frame + pane ladder: loading →
        connectors → routines → room → bot → home; kill-switch ribbon; all keyboard shortcuts
        mod+k/,/b/n + window events office-deleted/updated/bots-changed/open-settings/
        open-connectors), `WorkspaceRail.svelte` (64px: Home/Agents/Offices/Connectors/Routines
        + Halt status + Settings), `WorkspaceSidebar.svelte` (brand strip + ⌘K; wraps existing
        Sidebar/ChatRoomList), `HomePane.svelte` (welcome composer hub moved out of layout).
      * `+layout.svelte` is now a 5-line wrapper. `.app-frame` grid CSS in components.css with
        `--frame-sidebar` column animation (`--ease-panel`), reduced-motion off.
      * Verified: svelte-check 0 errors, vitest 18/18, vite build OK. NOT yet visually run in
        `npm run tauri dev` — do that before/while starting P3.
      * Deferred from P2: sidebar drag-resizer + auto-compact below 424px, AccountDock,
        bot status state machine (→ P4), Connectors/Routines panes still host the old panel
        components inside the new chrome.
- [x] P3: Chat core — DONE (core) 2026-09-24.
      * New `src/lib/chat/` modules: `grouping.ts` (showAuthorHeader per consecutive-author run,
        authorKey from sender_bot_id/sender_name/role, `authorHue` seeded color),
        `streamReveal.svelte.ts` (`StreamReveal.track(rawBuffer)` word-reveal @60ms,
        reduced-motion snap, restart on buffer replacement), `grouping.test.ts` (3 tests).
      * ThreadView: `const reveal = new StreamReveal(); $effect(() => reveal.track(streamingText))`
        — streamingText stays the raw buffer (all existing mutation sites untouched);
        live bubble renders `reveal.shown`; OpenBot bubble variants (user = surface-2,
        streaming/agent = surface-1, radius-bubble 22px, no shadow, `max-w-[min(80%,720px)]`,
        line-height 1.625); avatar gated per author run with gutter spacer.
      * ChatRoomView: same bubble spec + multi-author grouping (name row + avatar only on first
        row of a run) with per-author `--message-author-color` inline hue.
      * Verified: svelte-check 0 errors, vitest 21/21, vite build OK. NOT yet run in `tauri dev`.
- [x] P3.5 (remaining chat work) DONE 2026-09-25: extracted the now-parallel markup in
      ThreadView/ChatRoomView into one shared `chat/ChatMessageRow.svelte` ("everything is a prop";
      see §5 entry). ~~tool-call rows as
      OpenBot `ChatActionMarker` mini-rows~~ DONE 2026-09-24: `src/lib/components/chat/ChatActionMarker.svelte`
      (icon+verb rows; spinner running → icon+opacity-60 done; verb map keyed by backend tool ids,
      unknown → "Running {name}"). ThreadView accumulates `actionMarkers` on tool_started/tool_finished
      (cleared on send + loadMessages; ToolStarted payload only carries `name`, no id), renders the feed
      above the streaming bubble; shimmer label no longer duplicates the tool name. CSS
      `.chat-action-markers`/`.chat-action-marker` in components.css. ChatRoomView keeps its per-lane
      tool chip (already shows live tool + status). Marker verbs are hardcoded English → P7 i18n.
      per-bot streaming
      `lanes` in ChatRoomView (~line 245, listener ~:354, lanes render ~:930-974) also routed
      through StreamReveal; edit-resend/regenerate strip unchanged.
- [x] P4: Sidebar intelligence — bot state machine indicator fed by agent-stream + approvals;
      empty states; pin/reorder. DONE 2026-09-27 (icons+badge rollup landed; drag-reorder
      deferred, see below).
- [x] P4: DONE (core) 2026-09-24 — `src/lib/fleetActivity.svelte.ts`: refcounted global
      agent-stream listener deriving per-bot activity `working|attention|responded|idle`
      (run_started/delta/tool_started→working; approval/question/paused→attention;
      decided/answered→working; done→responded, 3s hold→idle). Started/stopped in
      Workspace.svelte; Sidebar bot rows override the status dot color + label line with
      live activity (pulse brand=working, pulse warning=attention, success=responded).
      Deferred: ~~TypingDots/triangle-alert icons~~ DONE 2026-09-27 (Sidebar badge: working
      = 3×`.typing-dot` pill in `--brand`, attention = `TriangleAlert` in `--warning-text`;
      see §5 entry), responded label
      strings are hardcoded English → must go through i18n in P7 (6 locales),
      ~~rail-level badge rollup~~ DONE 2026-09-27 (WorkspaceRail Offices button: warning
      count pill for attention bots, else pulsing brand dot for working — see §5 entry),
      ~~office-member activity in ChatRoomView headers~~
      DONE 2026-09-25 (header roster rings/dots via fleetActivity — see §5 entry).
      Remaining (deferred, needs backend): drag-reorder of Sidebar bot rows — requires a
      persistent ordering column + migration in the bots table; pin already exists
      (`set_bot_pinned`, sort priority in `filteredBots`) and empty states exist
      (`sidebar.noBots` in Sidebar, `office.empty*` in ChatRoomList). i18n label part of
      this deferral list was satisfied earlier (fleet.{working,attention,replied} shipped
      in all 6 locales during P7).
- [x] P5: Offices as the hero surface — org graph, member ranks, run timeline (our moat).
      DONE 2026-09-25 (all sub-items; see struck lines below).
      2026-09-24 lane work: ChatRoomView streaming lanes use `laneRevealOf(botId)`
      (`StreamReveal` per bot, stale lanes tracked to "") for word-reveal output; lane
      bubbles follow the OpenBot spec (radius-bubble, surface-1, hairline-faint, lh 1.625)
      and per-author hue on names (persisted rows via `--message-author-color`, live lanes
      via inline hsl).
      ~~org-graph visualization~~ DONE 2026-09-25: Plan Mode modal live run-graph preview —
      see §5 entry (dag.ts + PlanDag.svelte + dependency chips).
      ~~run timeline~~ DONE 2026-09-25: live office activity strip — see §5 entry
      (runTimeline.svelte.ts + RunTimeline.svelte + room.tl* keys).
      ~~office memory panel home~~ DONE 2026-09-25: Office Brain drawer on the room header —
      see §5 entry (ChatRoomView + reused OfficeMemoryPanel, zero new i18n keys).
      ~~member activity in room header~~ DONE 2026-09-25: live activity rings + dots on the
      header roster avatars (fleetActivity-driven) — see §5 entry.
      ~~CreateChatRoom wizard restyle~~ DONE 2026-09-25: 3-step New Office wizard
      (stepper + Back/Next, one section per step) — see §5 entry.
      ~~retire McpManager into ConnectorCenter~~ DONE 2026-09-25: legacy 1211-line dialog
      deleted; all per-bot MCP entry points now navigate to the Connectors hub — see §5 entry.
- [x] P6: Secondary panels restyled/re-homed — ALL 7 DONE 2026-09-25.
      ConnectorCenter, Routines, Memory, Sync, Computer, Settings modal, Bot settings.
      ~~ConnectorCenter~~ DONE 2026-09-25: compact pane header + token fixes — see §5 entry.
      ~~Routines~~ DONE 2026-09-25 (P6 batch 2 — re-homed pane header; see §5 entry).
      ~~Memory~~ DONE 2026-09-25 (P6 batch 3 — compact pane header + SkillManager mangled-class fix; see §5 entry).
      ~~Sync~~ DONE 2026-09-25 (P6 batch 4 — compact pane header + modal title dedupe + notify(); see §5 entry).
      ~~Computer~~ DONE 2026-09-25 (P6 batch 5 — solid-pastel invisible-text buttons + banner pairings fixed; see §5 entry).
      ~~Settings modal~~ DONE 2026-09-25 (P6 batch 6 — modal-panel shell + raw-black/palette tokens + no-scrollbar; see §5 entry).
      ~~Bot settings~~ DONE 2026-09-25 (P6 batch 7 — hairline modal border + destructive-token delete zone + banner pairings; see §5 entry).
- [ ] P7: i18n full coverage (6 locales), ~~a11y pass (WCAG AA)~~ DONE
      2026-09-25, vitest green
      (`npm test`), `cargo test` untouched, final `npm run tauri dev` manual pass.
      ~~chat-surface leftovers~~ DONE 2026-09-25 (P7 batch 13 — 9 new keys × 6 locales;
      see §5 entry).
      REMAINING i18n: AvatarPicker, ChatRoomList, ChatRoomView empty-state trio,
      OfficeSettings descs, ThreadView stragglers, Workspace boot line,
      WorkspaceSidebar, SkillManager 20-tool catalog. ~~REMAINING P7: a11y
      pass~~ DONE 2026-09-25 (see §5 a11y entry). REMAINING P7: user-run
      `tauri dev` visual pass.
      ~~AvatarPicker + ChatRoomList + empty-state trio~~ DONE 2026-09-25 (P7 batch
      14 — 20 new keys × 6 locales + 4 dead-hover button fixes; see §5 entry).
      ~~KillSwitch/OfficeSettings palette+i18n~~ DONE 2026-09-25 (P7 batch 15 —
      see §5 entry). ~~ThreadView/Workspace/WorkspaceSidebar stragglers~~ DONE
      2026-09-25 (P7 batch 16 — 9 new keys × 6 locales + `thread.historyToggle`
      reuse; pills + Toaster/LEAD badge swept by rescan; see §5 entry).
      ~~"Agent"/"Generalist" display-vs-data audit~~ DONE 2026-09-25 (P7 batch
      17 — 7 new keys × 6 locales incl. new `sr` namespace for ScreenReader
      status messages; see §5 entry). ~~SkillManager native-tool catalog~~
      DONE 2026-09-25 (P7 batch 18 — 35 built-ins × {name,desc} + 9 permission
      labels + toggle aria × 6 locales; community "awesome" skills stay raw by
      policy; Rust Debug permission strings fixed; see §5 entry).
      REMAINING i18n: utils.ts data catalogs (needs data-localization
      decision). ~~a11y pass~~ DONE 2026-09-25 (see §5 P7 a11y entry).
      REMAINING P7: user-run `tauri dev` visual pass.
- **P8 — Any provider/model + MCP/plugins usable everywhere** (approved plan
      `lone-glade-finch`; user-approved ADDITIVE backend scope — existing
      command signatures and the `agent-stream` payload stay immutable).
  1. [x] Custom providers backend: migration 018 (`custom_providers` +
      `provider_base_urls`), `ModelProvider::Custom`, manager custom-spec
      resolution + built-in base-URL overrides, discovery
      `fetch_models_for`, 6 new commands, startup hydration. DONE 2026-09-25.
  2. [x] Custom providers UI: model-catalog merge, Settings "Custom
      Providers" card + dialog, ModelPicker free-form model id +
      noToolsWarn/customBadge, i18n × 6. DONE 2026-09-25.
  3. [x] Remote MCP + plugin backend: migration 019 (mcp url/transport/
      headers, openapi server_base, plugins enabled_global), URL-XOR save
      validation, server_base persistence + legacy re-derive, global plugin
      fallback, toggle_plugin_global/list_global_plugins. DONE 2026-09-25.
  4. [x] Connectors + plugins UI: Stdio|HTTP transport dialog in
      ConnectorCenter, PluginsStore global toggle, i18n × 6. DONE 2026-09-25.
  5. [x] Forward MCP servers to external engines: EngineMcpServer +
      EngineRequest.mcp_servers, shared resolution helper, ACP/Claude/Codex
      dialect serialization (cap 16, warn-not-fail downgrade). DONE 2026-09-25.
  6. [x] Tool emulation for tool-less models: capability helper, prompt
      injection + fenced-JSON parse + plain-text transcript + repeat guard,
      status pill, i18n × 6. DONE 2026-09-25.

## 5. Session log / gotchas

- 2026-09-24 (P7 i18n batch 2 — tool/ui/skills/home):
  - Injected batch-2 keys into ALL 6 locales: `tool.running`("Running {{name}}…")/`tool.done`("Ran {{name}}")
    with `{{param}}` substitution (t() already supports it); `ui.save/saving/plugins/mcp/connectors/on`;
    `skills.capabilitiesTitle/description2` (merged into existing `skills` section);
    `home.placeholder/deepSearch/think/attach/send`.
  - ChatActionMarker rewritten: English verb MAP → ICONS-only map (tool name → lucide icon);
    label is now `t(done ? "tool.done" : "tool.running", { name: display })` where
    `display = name.replace(/_/g," ")`. Verbs are no longer hardcoded, so they translate for free.
  - SkillManager wired: capabilities title + description2 + Plugins/MCP/Connectors chips + save/saving.
    ("ON" badge, "Cancel", per-skill names/descriptions/permissions still English — those are
    data-layer strings from the backend, intentionally left.)
  - HomePane wired: placeholder + DeepSearch/Think/Attach/Voice/Send `title=` → `t("home.*")`.
  - CommandPalette wired + copy trimmed (per no-unnecessary-text): new `palette.*` key group in all 6 locales
    (switchTo with `{{name}}`, connectors/createBot/settings labels+descriptions, search/clear/
    noMatches/navigate/select/dismiss). Long marketing strings ("Manage 135+ MCP tools…",
    "Provision a new sovereign AI agent") replaced with short translated ones. Category union types
    ("Bots"/"Actions"/…) are filter-only, never rendered — left as-is. Footer "RAVENBOT Core" = brand, kept.
  - HomePane extra pass: heading `What's on your mind?` → `t("home.heading")`; the 4 suggestion cards
    converted from `const` to `$derived` with `home.s1Title/s1Text…s4Title/s4Text` (shortened copy);
    voice titles → `home.voice`/`home.listening` (batch-2 claim "Voice wired" was wrong — now true).
    Chip labels "DeepSearch"/"Think" intentionally kept as feature names.
    KEY-GROUP TIP: `home.*` now merged in-place in the JSONs via dict.update() — safer than assignment.
  - RoutinesPanel fully wired: new `routines.*` group (24 keys × 6 locales) — title (shortened "Scheduled
    Routines"→"Routines"), scheduler running/idle badges, New/Create/Enable/Disable, form placeholders,
    5 cron-preset chips (schedulePresets became `$derived`), empty state with `{{name}}`, last-run with
    `{{when}}` + `routines.never`, webhook on/off/runNow/delete/copyUrl titles + webhookHint.
    Backend error fallbacks ("Failed to load routines" etc.) left English — data-layer, like skill names.
  - Verified again after RoutinesPanel: check 0, 21/21, build OK.

- 2026-09-24 (P7 i18n batch 3 — memory/room/bot groups):
  - OfficeMemoryPanel: new `memory.*` (19 keys ×6). Title shortened "Shared Office Brain (Blackboard
    Memory)"→`memory.title` ("Office Brain"); CATEGORIES became `$derived` (backend ids unchanged, only
    labels translated); count/relevance/recalls use `{{params}}`.
  - ChatRoomView: new `room.*` (56 keys ×6) + generic `ui.cancel/close/done`. Wired: header tooltips
    (editOffice/telemetry/hire/settings/plan), composer (removeAttachment, split/dispatch; attach reuses
    `home.attach`), full Plan modal, full Hire modal (hire1/hireN pair instead of plural-params —
    t() has no pluralization), Manage-agent chooser rows, pipeline-error banner. Trimmed two long
    marketing paragraphs per no-unnecessary-text. All title/placeholder/aria attrs now `t()`-driven.
    Kept English: "CEO / ORCHESTRATOR" + "LEAD" badges (role jargon), workdir path placeholder, "None".
  - BotSettings: new `bot.*` (16 keys ×6) — avatar title, engine/name/specialty/instructions
    placeholders, unsaved badge, provider/temperature/maxTokens/fallback/maxRounds labels,
    autoEdit/readOnly policy descriptions.
  - INCIDENT (gotcha!): injector used `d["bot"] = vals` which CLOBBERED the pre-existing 5-key `bot`
    section (deleteConfirm etc. referenced by i18n.test) → check+tests failed. Restored via
    `git show HEAD:...locales/{loc}.json` merged back with `{**old_bot, **new_bot}`.
    RULE: always `d.setdefault(ns, {}).update(vals)` unless you verified the namespace is new.
    `room`/`memory` were genuinely new (safe); `ui`/`skills`/`home` merges used update() correctly.
  - Verified: check 0 errors, 21/21 tests, build OK. NOT run under tauri dev.

- 2026-09-24 (P7 i18n batch 4 — ThreadView, the last big chat surface):
  - 33 new keys merged into the EXISTING `thread.*` namespace (used setdefault().update() per the
    batch-3 rule; now 39 keys ×6 locales; old welcome/send/typing/noThreads keys kept — noThreads reused).
  - Wired: all header tooltips (cliBot/switchModel/tempTip/switchThread/newThread/telemetry/intelligence/
    historyToggle/computer/channels/importTeam/fleetSync/settingsKeys), routines tooltip reuses
    `routines.title`, composer reuses `home.deepSearch/think/attach/send`, attachments reuse
    `room.removeAttachment`, thread-search panel (searchThreads/noMatches/noPrev), message actions
    (playAudio/editResend/copyFull/regenerate/cancelEdit), ask_user answer placeholder, telemetry bar
    (Tokens/Cost/Model), modelNeeded + highStakes + editingHint + autoRead + fleetSync banner.
  - Kept English (deliberate): "model id…" placeholder, DeepSearch/Think chip labels (feature names,
    same call as HomePane), "CEO / ORCHESTRATOR" / "LEAD" badges.
  - Remaining hardcode counts after this pass (attr+visible): OfficeSettings 48, Settings 45,
    ConnectorCenter 41, McpManager 33, then small ones ≤11 (TeamImport, Sidebar, ComputerPanel,
    ChannelsPanel, ArtifactPanel, TitleBar, CreateChatRoom, AgentIntelligence, SyncPanel, ModelPicker...).
  - Verified: check 0, 21/21, build OK. NOT run under tauri dev.

- 2026-09-24 (P7 i18n batch 5 — OfficeSettings):
  - New `office.*` namespace (60 keys × 6 locales; asserted the namespace was genuinely new first).
  - Wired all 8 tabs (tabGeneral…tabGoal), header (title reuses `room.settings`, agent-count via
    office.agent1/agentN), identity/name/mission, project folders (browse/remove/empty), avatar-picker
    toggle, domain-template section ("{{n}} ranks defined"), roster (fleetTag/noAgents/remove-agent
    title), add-existing + provision-new panels (3 placeholders + button), MCP matrix, policy & terms
    (presets label ×2 + both example placeholders), budget block (label/ceiling/share/roleDist),
    goal, footer (saved/`ui.saving`/saveChanges/del) and the full delete-confirm modal (desc uses
    {{name}}). Long strings trimmed per no-unnecessary-text.
  - Kept locale-neutral: "/home/you/projects/my-app" + "e.g. 500" placeholders; "Custom Office"/template
    names come from OFFICE_TEMPLATES data (backend-side, English).
  - Remaining hardcode counts: Settings 45, ConnectorCenter 41, McpManager 33, then small ones ≤11.
  - Verified: check 0, 21/21, build OK. NOT run under tauri dev.

- 2026-09-25 (P7 i18n batch 6 — Settings.svelte):
  - Merged 83 new `settings.*` keys × 6 locales into the EXISTING settings namespace (merge-only via
    setdefault().update(); namespace already had title/apiKeys/save/saved/cancel/about/version…).
  - Wired: navSections → $derived (labels via settings.nav*/ui.connectors/settings.about; "135+" badge
    kept numeric; "Live" → settings.live), sidebar stats (connected/modelsFound/refreshStatus), header +
    breadcrumb (settings.title) + close tooltip, full Providers section (providerKeys(+Desc), search,
    testAll/refresh, Saved/Connected/Not configured/modelsCount/failed badges, test/remove-key tooltips,
    getApiKey, noProvidersMatch + availableLabel, ollamaKeylessA/B split around the Local-AI button),
    Model Manager (modelsDesc, stat tiles, discoverAll/clear, default-model card w/ defaultDesc{{source}},
    saveDefault, modelsCount/free/vision, empty state), Budgets (agentBudgets(+Desc), selectAgent ×2,
    tokensUsed, cost reuses thread.cost, pctUsed{{pct}}+blocked, kind/period SimpleSelect labels — these
    are inline option arrays so re-render with locale automatically, limit, resetUsage/saveBudget,
    loading/empty hints), Profile (profileDesc, you alt, tapToChange, displayName/yourName, language(+Desc)),
    Themes (themesDesc, active), Local AI (localDesc, ollamaEndpoint, ui.save, probe, modelInstalled/
    modelsInstalled plural pair, moreCount, installOllamaA/B split around ollama.com link, experimental(+Desc)),
    About (sovereignOs, stat tiles, sovereignty), and the three alert() fallbacks (alert*Failed prefixes).
  - GOTCHA: `{#each THEMES as t}` shadowed the i18n `t` → svelte-check "not callable" error; renamed
    loop var to `theme`. Watch for single-letter loop vars when wiring {#each}.
  - Kept locale-neutral: "RAVENBOT v0.2.0", "http://localhost:11434" placeholder, provider-name list
    (proper nouns), theme names/descriptions/badgeLabel (THEMES data layer), console.error/dev comments.
  - Remaining hardcode counts: ConnectorCenter 41, McpManager 33, then small ones ≤11.
  - Verified: check 0, 21/21, build OK. NOT run under tauri dev.

- 2026-09-25 (P7 i18n batch 7 — ConnectorCenter.svelte):
  - New `connector.*` namespace, 136 keys × 6 locales (assert-new guard passed).
  - Wired: header (title trimmed from "Connectors & Tools Command Center" → "Connectors & Tools",
    subtitle trimmed, count badge {{n}}, presetStacks/syncing/settings.refresh/addCustom), agent strip
    (selectAgent + activeFor/connectorsUnit split around bold spans), searchPh (trimmed), all 5 scope
    tabs (tabActiveFor {{name}} reused by card status line), categoryDefs → $derived (ids are backend
    category values — NEVER translate), PRESET_STACKS → $derived (p1n..p6n/p1d..p6d, trimmed; connector
    id lists untouched), selection toolbar (selectedN/enableFor/disableFor/enableGlobally/clearSelection),
    cards (select/deselect titles, custom/unverified badges + trimmed unverifiedTip, toolsCount,
    moreActions, menuTest/menuCredentials/copied/copyCommand/menuDelete, full status-line branch set,
    btnActive/btnEnable, assignMultiTip, globalOn/globalOff), empty state, all 5 modals (preset/assign/
    register-custom/credentials/test-inspector/delete-confirm) incl. trustWarning, envHint, errRequired/
    errIdFormat/errArgs/errEnvName{{key}}/errEnvReserved{{key}}, every notify() toast (n* keys), and
    reused ui.close/ui.cancel/ui.saving/settings.refresh/settings.displayName (note: settings.refresh +
    settings.displayName were added to all 6 locales in batch 6, so these re-exports are safe).
  - Long marketing copy trimmed per no-unnecessary-text (inspector desc, delete modal — name now
    interpolated into delDesc instead of a styled span).
  - Kept locale-neutral/data-layer: "⚡" + "npx / python / uvx" + multi-line command/env example
    placeholders, "Custom"/category filter ids, "Custom MCP server connector" saved-desc fallback,
    s.category/s.description from backend.
  - Remaining hardcode counts: McpManager 33, then small ones ≤11.
  - Verified: check 0 (first pass), 21/21, build OK. NOT run under tauri dev.

- 2026-09-25 (P7 i18n batch 8 — McpManager.svelte):
  - New `mcp.*` namespace, 23 keys × 6 locales (assert-new guard passed): title (trimmed from
    "MCP Server Hub — 100+ Native Tools"), subtitle (rewritten short), toolsFor (split before bot-name
    span), count {{n}}, addCustom (one key now serves header + empty-state buttons), searchPh (trimmed),
    tabFor {{name}}, globalOnTip/globalOffTip, credEdit/credMissing/credConfigured/required,
    delBtn/delTitle, emptyTitle/emptyHint, footerOf {{shown}}/{{total}}, footerActiveFor {{name}},
    footerGlobal, lblName, connecting, failConnect.
  - Heavy reuse of connector.* (McpManager will eventually merge into ConnectorCenter — convergence
    deliberate): tabAll/tabGlobal/tabNeedsKeys/tabCustom, syncing, customBadge/unverifiedBadge/
    unverifiedTip, toolsCount, menuTest, copyCommand, statusNeedsKeys {{keys}}, tabActiveFor/enableFor
    {{name}}, globalOn/globalOff, clearFilters, registerTitle/registerDesc (replaced stdio/SSE marketing
    copy), phName/phId/lblId/lblIcon/lblCategory/lblDesc/phDesc/trustWarning/lblCommand/lblArgs/
    lblEnv/envHint/registering/saveConnector, errRequired/errIdFormat/errArgs/errEnvName/errEnvReserved
    (script-layer validation), credTitle/credDesc (replaced "sovereign local database" copy)/nCredsSaved/
    secretWriteOnly (trimmed)/savedKeep/enterKey {{key}}/saveCredentials, diagTitle/diagDesc (replaced
    "protocol handshake" copy)/connOnline/connFailed/latency {{ms}}/discoveredTools {{n}}/deleting,
    nFailCreds/nFailDelete alert prefixes; plus settings.refresh and ui.done/cancel/saving/close.
  - categoryDefs → $derived (backend ids untouched); "All" chip uses connector.tabAll (plain "All").
  - FIX beyond i18n: custom-server Category SimpleSelect used `label: c.id` (raw backend strings) —
    now maps to translated `c.label` while value stays the backend id.
  - Removed per no-unnecessary-text: "Autonomous Execution Ready" decorative span in test modal.
  - Kept locale-neutral/data-layer: "e.g. ⚡", "npx, uvx, python, node", multi-line arg/env example
    placeholders, "NATIVE" tool badge, "Custom MCP server integration" saved-desc fallback,
    console.error strings, all category/filter id comparisons.
  - Remaining hardcode counts: small surfaces ≤11 each (TeamImport, Sidebar, ComputerPanel,
    ChannelsPanel, ArtifactPanel, TitleBar, CreateChatRoom, AgentIntelligence, SyncPanel, ModelPicker,
    PluginsStore, MarkdownRenderer, KillSwitch).
  - Verified: check 0 (first pass), 21/21, build OK. NOT run under tauri dev.

- 2026-09-25 (Visual fix — ConnectorCenter cards + ConnectorIcon, from user screenshot):
  - Root causes found in the screenshot: (1) the Anthropic ConnectorIcon SVG was literally a
    check-in-a-box, so the Claude card looked like a checked selection checkbox — replaced with the
    Anthropic/Claude 8-spoke starburst (4 rotated rounded rects, #D97706). (2) Icon matcher bugs:
    `cleanId.includes("x")` gave the Twitter/X logo to ANY id containing "x" (context7, text-*) and
    `includes("git")` gave Git to "digitalocean" — now `startsWith("git")` and exact/prefix "x".
    (3) Five AI/ML cards all showed the same purple Layers fallback — added brand branches:
    langchain (Link2 #12B76A), chroma (Palette #FB4E5C), qdrant (Radar #F59E0B), milvus (Boxes
    #2DD4BF), replicate (Clapperboard #F472B6), weaviate (Boxes #C4FA77).
  - Card noise cut (OpenBot flat style): removed the UNVERIFIED title badge (the status line already
    says it), raw backend category "AI / ML" now renders translated label via new
    `categoryLabel(s.category)` helper (ids untouched), selection Square contrast bumped
    text-muted → text-tertiary.
  - Agent access (user: "everything can be accessed by any agents properly"): when NO agent is
    selected (bots empty / none picked), the card footer previously showed only an unlabeled robot
    icon — now a full-width labeled "Assign to agents" outline button (new key
    connector.assignAgents ×6 locales, merged via setdefault().update()). With an agent selected the
    Enable/Active + icon-assign layout is unchanged. The Global toggle label is now always "Global"
    (state = blue brand-soft vs muted styling) with action tooltips reused from mcp.globalOnTip /
    mcp.globalOffTip — "Global off" as a label was ambiguous (state vs action).
  - Note: screenshot showed NO agent strip ⇒ that render had bots.length === 0 (workspace.bots not
    yet loaded or list_bots failed); strip/footer logic is reactive, so it appears once bots load.
    ConnectorCenter call sites verified: Workspace passes all 4 props; Settings.svelte passes only
    {bots} (no selectedBotId/onSelectBot) — acceptable, ConnectorCenter self-manages currentBotId.
  - Verified: check 0, 21/21, build OK. NOT run under tauri dev.

- 2026-09-25 (Model settings + engine integration overhaul — OpenBot-style):
  - Research: OpenBot handles models via ONE descriptor table, runtime discovery that BEATS any
    shipped catalog, and refuses to guess prices ("cost:null rather than guess"). RAVENBOT's
    backend catalog (lib.rs, frozen) is stale (claude-3.x, gpt-4o, grok-2) and carries NO
    context-window/price data; runtime context_window_for() is a substring guess; UI showed
    neither. All fixes frontend-only (backend frozen, invoke contract untouched).
  - `src/lib/model-catalog.ts` rewritten: CatalogModel gains context_window,
    input/output_cost_per_1m, supports_tools, reasoning (all optional; undefined = unknown,
    never guessed). New `CURATED` table layers verified 2026-09-25 facts (from
    platform.claude.com, developers.openai.com, ai.google.dev, docs.x.ai, console.groq.com,
    api-docs.deepseek.com) over the frozen backend catalog in `getCatalog()`/`minimalCatalog()`
    (anthropic→claude-fable-5/opus-5/sonnet-5/haiku-4-5; openai→gpt-6 astra/sol/luna,
    gpt-5-pro, o3-mini; gemini→3.5-flash…2.5-flash-lite; deepseek→V4.1-Flash/V4-Pro;
    xai→grok-4.7/4.3; groq→llama3.3+gpt-oss; perplexity→sonar*). Gateways we cannot verify
    (commandcode/opencode/cline/tokenrouter/mimo) keep backend data.
  - Discovery-first merge: `modelsFor()` now normalizes + merges live
    `fetch_provider_models` results over curated facts (live fields win). Module-level
    `noteDiscovery`/`modelMetaFor` cache feeds EVERY surface (gateway ids "vendor/x" hop to
    the vendor's facts). Formatters: `formatContext` (131072→128K), `formatPricing`,
    `modelSummary`. Settings/BotSettings/ThreadView discoveries call noteDiscovery.
  - `ModelPicker.svelte`: per-row facts line (ctx · $in/$out per 1M) + Tools/Reason badges;
    fully i18n'd (was hardcoded English) via NEW `model` namespace 18 keys × 6 locales
    (/tmp/i18n_add15.py) + `settings.envHintTitle` merged (/tmp/i18n_add16.py).
  - `Settings.svelte`: providers badge = real catalog.length (was `keyedCount || 12`);
    About stats dynamic (was "16"/"28+"); empty-state provider list from catalog (was a
    hardcoded sentence); provider cards show the env var the Rust runtime reads when no key
    is stored (Provider rules); discovered-model panels render ctx+pricing per row; the
    default-model card shows live facts for the selected provider/model.
  - `BotSettings.svelte`: stale hardcoded defaults `openrouter/anthropic/claude-3-5-sonnet`
    GONE — seeds from `get_default_model` (globalDefault) else ollama; facts line under the
    picker; "not listed" note now `model.notListed`. Engine (CLI) path untouched: it already
    lists `list_engines` CLIs with availability/install/sign-in hints and optional
    engine_model override, and dims the API picker while a CLI owns the loop.
  - `ThreadView.svelte`: header pill getModelDisplayName no longer invents
    claude-3.5-sonnet fallback (uses global default loaded at mount, honest "?"), pill title
    shows model facts; switcher inherits enriched ModelPicker.
  - NOT touched (frozen, logged): Rust context_window_for() substring guess + fake
    usage.cost(0.003,0.015) constants + plaintext provider_keys storage — backend work if
    unfrozen.
  - Verified: check 0, 21/21, build OK. NOT run under tauri dev.

- 2026-09-25 (Fix — rail Skills/Store buttons dead-ended, from user screenshot #9):
  - Diagnosis: rail Sparkles (Skills) and Store (Marketplace) buttons looked dead because
    `openSkills`/`openMarketplace` require a selected bot; with zero bots they fell back to
    `workspace.goto("agents")` — but dest `"agents"` had NO rendering branch in Workspace.svelte
    when no bot is selected, so the main area silently fell through to HomePane. Same dead-end
    affected the rail "+" (addAgent) button and CommandPalette's onCreateBot.
  - Fix: new "Pick or create an agent" pane — Workspace.svelte renders a branch for
    `dest === "agents" && !selectedBot` with bot chips (select) + inline create form
    (Button/Input, `createAgent()` calls `workspace.createAgent(name)`).
  - `workspace.svelte.ts`: added `showAgents()` (clears selection, dest="agents", announce)
    and `createAgent(name)` (invoke `create_bot` camelCase, then `handleBotCreated` →
    ThreadView opens on success). Rail "+", palette onCreateBot, Skills/Store fallbacks all
    now route through `showAgents()`.
  - i18n: 4 new `rail.*` keys (agentsTitle/agentsHint/agentsPh/agentsCreate) × 6 locales via
    merge-only injector (/tmp/i18n_add14.py).
  - Verified: check 0, 21/21, build OK. NOT run under tauri dev.

- 2026-09-25 (P7 i18n batch 9 — TeamImport/Sidebar/KillSwitch/TitleBar):
  - Audited first: ChatActionMarker + fleetActivity already clean (tool.*/fleet.* keys, no strings).
  - i18n injector /tmp/i18n_add17.py: merge-updated `sidebar.*` (~40 keys: status labels idle/thinking/
    runningTool/waitingOnYou/paused, newChat, fleetAgents, agentOptions, createTip/show/hideHiddenTip,
    unread {{n}}, menu pin/unpin/markRead/duplicate/hide/unhide/connectorsHub/mcpTools/remove,
    provision dialog createBot/provisionDesc/agentDetails/chooseAvatar {{style}}/agentName (+Ph)/
    mission (+Ph)/newAgent/styleLabel/customize/creating/createAgent, duplicated {{name}}; reused
    search/noBots/pauseAll/filterWaiting with new fleet wording — safe: zero prior usages), plus
    `killSwitch.*` additions (tip/reasonLabel/stopping/confirmStop + reworded confirmTitle/confirmMessage/
    reasonPlaceholder) — ALL into 6 locales. New namespaces: `titlebar.*` (9), `team.*` (16).
    Two micro follow-ups: sidebar.skills, sidebar.createAgent (inline python, merge-only).
  - Interpolation is `{{param}}` (i18n/index.ts:83) — single-brace `{style}` placeholders silently
    fail; injector script sed-fix caught this pre-run.
  - Sidebar.svelte: full sweep incl. getStatusTheme labels, unread badge title, 11 menu items,
    whole create dialog, duplicated toast. KillSwitch: banner + dialog (backend "Manual trigger"
    reason value kept English — it is sent data, not chrome). TitleBar: all window-control
    titles/aria + theme popover. TeamImport: wizard + result summary.
  - Verified: check 0, 21/21, build OK. NOT run under tauri dev.

- 2026-09-25 (P7 i18n batch 10 — ComputerPanel/ChannelsPanel/ArtifactPanel/CreateChatRoom):
  - Injector /tmp/i18n_add18.py (all 6 locales): NEW namespaces `computer` (26), `channel` (19),
    `artifact` (11); merge-only additions `office.*` (21 create-dialog keys: createTitle/createHint/
    changeIcon/createNamePh/createMission(Ph)/createAvatar/templateLabel/rolesFor {{name}}/autoStaff(+Desc
    {{name}})/staffing {{n}}/autoFill/noBotsYet/generalAgent/removeMember/assignRank {{rank}}/establishing/
    createBtn {{name}}/fallbackName…) and `ui.loading` + `ui.delete`.
  - Mixed-markup sentences handled by fragment keys with embedded spaces (spaces live INSIDE the
    strings, no source whitespace between expression and mono `<span>`): computer.noBackend1/2/3 around
    xdotool/ydotool/wtype; hostPolicyBlocked1/2 around RAVENBOT_ALLOW_WAYLAND_CONTROL=1.
  - ComputerPanel: header/tabs/Live toggle/capture btn, capability strip (host-control + Docker lines),
    capture error+empty states, img alt, isolated-desktop card (desc, docker warning, Running/Stopped,
    Open/Stop/Start buttons), host-control policy block. Close → ui.close.
  - ChannelsPanel: title/subtitle/New/loading/empty/botsN/selectHint, all Labels (Name → room.name),
    instructions(+Ph), folder, responder rules (mode labels now t() inside the each-array), autoLead
    option + Auto placeholder, manualHint, roster, Delete → ui.delete, Save Channel. Backend default
    channel name "Channel {n}" kept (data).
  - ArtifactPanel: chip, Code/Preview tabs + titles, dock/expand, download/copy/close, iframe title.
  - CreateChatRoom: full dialog incl. auto-staff checkbox copy, staffing count label, roles header,
    create button `Create “{{name}}”` (en uses curly quotes in the key itself). Rank fallbacks
    "Member"/"Generalist" and avatar seed "office" kept — those are backend data.
  - Verified: check 0 errors/0 warnings, vitest 21/21, build OK. NOT run under tauri dev.
  - Remaining small surfaces for batch 11: AgentIntelligence (~8), SyncPanel (~6), PluginsStore (~6),
    MarkdownRenderer (~4); then BotSettings full surface (bigger, last i18n job).

- 2026-09-25 (P7 i18n batch 11 — AgentIntelligence/SyncPanel/PluginsStore/MarkdownRenderer):
  - Injector /tmp/i18n_add19.py (all 6 locales): NEW namespaces `intel` (15), `sync` (20),
    `store` (28), `markdown` (6). Patched pre-run so store.countEquipped takes {{name}} in ALL
    locales (ja "件を{{name}}に装備", zh "已为 {{name}} 装备") and the call site passes bot.name.
  - AgentIntelligence: level names, title {{name}}/{{score}}, desc {{p}}/{{t}}/{{l}}, stat labels,
    learnNote, recentTitle, empty, loading. Also fixed 4 leftover broken class tokens
    `bg-[var(--surface-2)]ard` → `bg-[var(--surface-2)]` (old perl-sweep regression; project grep
    now shows zero `]ard`).
  - SyncPanel: header/signed chip/desc, export btn+tip, exportFailed alert, current badge,
    import section (title/pastePh/openFile+tip/verifyImport+tip/tamperNote), 4 result messages
    (importedSigned/Unsigned + File variants, {{id}}/{{file}}). Bundle filename pattern and
    bot.name fallbacks stay data.
  - PluginsStore: categories → labelKey array rendered with `t(cat.labelKey as TranslationKey)`
    (same cast pattern as WorkspaceRail; first check run flagged the string→literal union error),
    title/desc {{name}}, sync/syncing, in-app banner fragments store.inapp1/2/3 around
    `<code>plugin_search</code>`, searchPh/urlPh, addSpec/adding, Active/Available badges,
    noDesc fallback, inappTool/ready, Enabled/Enable, empty/emptyHint, footer counts, Done → ui.done.
    "All" kept as internal category id (not display text).
  - MarkdownRenderer: script-literal strings — think-summary 🧠 Reasoning, Copied!/Copy toast
    swap-back, artifact button title+label, copy button title+initial label (all via t() inside
    the innerHTML template literals). console.error dev strings left English.
  - Verified: check 0 errors/0 warnings, vitest 21/21, build OK. NOT run under tauri dev.
  - Next i18n piece: BotSettings.svelte full surface (bigger, last i18n job).

- 2026-09-25 (P7 i18n batch 12 — BotSettings, LAST i18n surface):
  - Injector /tmp/i18n_add20.py: merge-only ~70 new `bot.*` keys × 6 locales (tabs, engine cards,
    picker errors/footer notes, voice/auto-read, computer control, project folder, temp/tokens/
    fallback/rounds, avatar box, orchestrator/approval/isolation option name+desc pairs, directive
    tab, footer + delete dialog). Fragments bot.runsOnCli1/2 wrap the engine-name `<span
    class="font-mono">` (leading/trailing spaces live inside the strings; de/es/ja/zh word order
    patched pre-run).
  - Reused instead of duplicating: bot.engineDefault (voice fallback name + engine-model
    placeholder), bot.builtInLoop/cliDefaultHint/autoEditLow/readOnlyFree, sidebar.agentName +
    sidebar.mission (identity labels, exact matches), ui.cancel, bot.saving, bot.unsaved (title).
  - Script section wired too: native folder-picker dialog title, picker-failure notify,
    "Agent saved" toast, discard-changes confirm(), synthetic "Engine default" voice name.
  - Data kept English: engine ids/versions/install_hint, sandboxReport backend+note, voice
    provider names, "local" engine fallback, tier ids (OsLevel/Docker/Host), approval ids,
    folderPh example path prefix "~/RAVENBOT/…" (parenthetical translated), brand names
    (DiceBear, Ollama, OpenAI, Wayland, Docker, git, `ollama serve`).
  - One check error fixed: t("bot.unlockKey") param typed string|number → pass
    `activeProvider?.name ?? modelProvider`.
  - Verified: check 0 errors/0 warnings, vitest 21/21 (locale parity test covers all new keys),
    build OK. NOT run under tauri dev.
  - P7 i18n sweep COMPLETE — every component surface now goes through t(). Remaining P7:
    a11y WCAG AA pass + user visual pass under tauri dev.

- 2026-09-25 (P3.5 ChatMessageRow extraction — last chat piece):
  - NEW `src/lib/components/chat/ChatMessageRow.svelte`: single row shell owning the
    `message-entry` flex + `data-grouped`, 24px author gutter (avatar or spacer), author name row
    (uses `--message-author-color` set via `authorColor` prop), bubble variants (user plain / agent
    MarkdownRenderer / ghost), error-card slot, meta line (time + hover actions) and the room's
    trailing 32px user avatar. Everything is a prop; optional named snippets: `errorCard`,
    `actions`, `aboveBubble`, `belowBubble`, `userExtras` (Svelte 5 child-snippet → same-named prop).
  - ThreadView: whole each-block row replaced — model-error card → errorCard snippet, checklist
    accordion → aboveBubble, sources chips → belowBubble, attachment images → userExtras (wrapper
    `{#if messageImages.length}` moved INTO the snippet so empty cases don't render padding),
    5 hover buttons → actions snippet; still passes grouped/!continuesRun/ghost and the
    `hasChecklist ? content.text : rawContent` bubble-text rule via `text`. Import added.
  - ChatRoomView: same, `showAuthorHeader(messages, mi)`/`authorHue` stay in the view (row is
    dumb), pipelineError card → errorCard snippet. Rendered meta unifies to the thread's
    `message-meta flex px-3` shape (was text-right/left) — deliberate convergence per OpenBot spec.
  - Untouched on purpose: ThreadView live-streaming row (not a persisted message; still raw
    `message-entry`, the only one left project-wide) and ChatRoomView per-lane streaming bubbles.
  - First check run caught 2 nits, fixed: missing `import type { Snippet } from "svelte"` and the
    ChatMessageRow import in ThreadView.
  - Verified: check 0 errors/0 warnings, vitest 21/21, build OK. NOT run under tauri dev —
    visual diff of 1:1 chat + office channel is part of the pending user visual pass.

- 2026-09-25 (P5 org-graph — Plan Mode run-graph DAG preview):
  - NEW `src/lib/chat/dag.ts`: pure longest-path layering (`dagLayers`), measured-free layout
    (`dagLayout` — positions/width/height from DAG_NODE_W/H/COL_GAP/ROW_GAP constants), `dagEdges`
    (dedupe + clamp). Cycle guard returns finite layers for cyclic plans (09-graph can't hang the UI).
  - NEW `src/lib/chat/dag.test.ts` (6 tests): layering, longest path, cycles/out-of-range, row
    stacking + canvas sizing, edge dedupe, empty plan. Vitest total now 27 (5 files) — update the
    expected string in future verification runs: "Tests 27 passed (27)".
  - NEW `chat/PlanDag.svelte`: dumb renderer — SVG bezier edges with arrow marker (stroke via
    `var(--brand)` inline style, no hex), absolutely-positioned node chips (20px DiceBear avatar +
    bot name + truncated task label), horizontal `overflow-x-auto no-scrollbar`. Dynamic positions
    use inline `style=` (allowed; still no `<style>` blocks).
  - ChatRoomView plan modal: live `<PlanDag tasks={planTasks} {members}/>` preview card under the
    task list + per-task "After" chips calling `togglePlanDep` (was dead code — now wired;
    restricted to earlier indices so plans stay topologically orderable).
  - i18n: `room.dagTitle` + `room.dependsOn` × 6 locales via /tmp/i18n_add21.py (merge-only).
  - Verified: check 0 errors/0 warnings, vitest 27/27, build OK. NOT run under tauri dev.
  - Not done (deliberate): rendering the *persisted* 📋 Briefing message as a graph in the feed
    (parse-markdown-into-DAG was judged lower value than the live editor preview; possible later).

- 2026-09-25 (P5 run timeline — office activity strip):
  - New `src/lib/chat/runTimeline.svelte.ts`: `describeRunEvent(p)` maps already-emitted
    agent-stream kinds (run_started / tool_started / tool_finished / approval_requested /
    question_asked / paused / done) into `{phase, detail}` and returns null for everything else
    (delta, usage, status, events without bot_id — frontend-only, backend frozen);
    `class RunTimeline` keeps a `$state` list capped at `MAX_ENTRIES=40` (slice keeps newest),
    `track(p)` returns whether the event was recorded, `reset()` clears on room switch.
  - New `src/lib/components/chat/RunTimeline.svelte`: horizontal pill strip (dumb renderer —
    props `{ events, nameFor }`), one icon+tone per phase (Play/Wrench/Check/ShieldAlert/
    MessageCircleQuestion/Pause/Flag), tooltip `clock · name · verb · detail`,
    auto-scrolls to the newest pill on each event. Sits under the Team strip in ChatRoomView,
    only rendered when `runTimeline.events.length > 0`.
  - ChatRoomView wiring: `const runTimeline = new RunTimelineState()` beside the other live
    telemetry state; `runTimeline.track(p)` fires first in the onMount agent-stream listener
    (before the existing status/usage/delta branches); `runTimeline.reset()` added to the
    room-change $effect alongside `agentStatus/lanes/agentTool` clearing.
  - i18n: `room.tlStart/tlTool/tlToolDone/tlApproval/tlQuestion/tlPause/tlDone` × 6 locales
    via /tmp/i18n_add22.py (merge-only).
  - Tests: `src/lib/chat/runTimeline.test.ts` (3 tests: kind mapping/guards, append + ignored
    kinds, cap-at-40 keeps newest + reset). Vitest now 30 tests / 6 files — update the expected
    string in future verification runs.
  - Verified: check 0 errors/0 warnings, vitest 30/30, build OK. NOT run under tauri dev.

- 2026-09-25 (P5 office memory panel home — Office Brain drawer):
  - OfficeMemoryPanel was only reachable two levels deep (room → Settings → "memory" tab).
    Gave it a first-class home on the room hero surface (P5's "our moat" item): a Brain
    button in ChatRoomView's header (between Settings and Plan) opens a right-side drawer
    hosting the EXISTING `<OfficeMemoryPanel chatroomId={room.id} />` — no component
    duplication, no backend change (memory invoke commands were already in use).
  - Drawer chrome follows the file's modal conventions: fixed inset-0 z-50 bg-black/60
    backdrop, click-outside + Escape to close, header bar = Brain icon + room name + ✕,
    scrollable body `p-4 overscroll-contain`. Zero new i18n keys (reuses `memory.title`
    for the button/dialog label and `room.close` for dismiss).
  - Gotcha: `<aside role="dialog">` trips svelte-check a11y
    (`a11y_no_noninteractive_element_to_interactive_role` — aside's implicit landmark
    role conflicts); use a plain `div role="dialog"` like the existing modals.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK. NOT run under tauri dev.

- 2026-09-25 (P5 member activity in room header — live roster badges):
  - ChatRoomView header avatar roster (the -space-x-2 stack) now shows each member's LIVE
    activity, closing P4's deferred "office-member activity in ChatRoomView headers" item:
    reads the global `fleetActivity` singleton (refcounted agent-stream listener already
    started in Workspace.svelte — no new listener, backend untouched), so the header agrees
    with the Sidebar's bot-row indicators by construction.
  - Rendering: wrapper div is now `relative` (hover scale/zoom moved onto it), the circle
    keeps overflow-hidden and swaps its static brand ring for `rosterRing(act)`
    (working → ring-2 brand, attention → ring-2 warning, responded → ring-2 success,
    idle → old ring-1 brand/40); non-idle members also get a corner dot `rosterDot(act)`
    (brand/warning pulse, success solid) with `ring-2 ring-[var(--surface-0)]` to match
    the header background. Colors mirror the Sidebar's exact mapping.
  - Deliberately visual-only: no new English label strings (activity WORDS stay i18n-pending
    P7 like the sidebar's), no new keys, tooltip unchanged (name + specialty).
  - Removed the dead `, i` from the roster each-block while rewriting.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK. NOT run under tauri dev.

- 2026-09-25 (P5 CreateChatRoom wizard restyle — stepped New Office dialog):
  - CreateChatRoom.svelte (our-only surface; OpenBot has no office-creation equivalent) is
    now a 3-step wizard instead of one long scroll: ① Office (identity card + avatar picker),
    ② Template (grid + role preview), ③ Staffing (auto-staff or manual roster). `let step =
    $state(0)`; a `$effect` resets it when `open` goes false, and create() resets on success.
  - Stepper sits under Dialog.Header: mono uppercase pills (size-5 number chip → CheckCircle2
    once passed, brand ring when active), hairline connectors, forward jumps gated on
    `name.trim()` (step 1 can't be skipped without a name). Sections wrapped in
    `{#if step === N}` — zero invoke changes, all state logic untouched.
  - Footer split: Cancel left; Back (step>0) + Next (step<2, disabled without a name) or the
    existing Create button (step 2) right. Dialog narrowed sm:max-w-3xl → 2xl since one
    section renders at a time.
  - i18n: 5 new keys office.stepOffice/stepTemplate/stepStaffing/back/next × 6 locales via
    /tmp/i18n_add23.py (merge-only into existing office ns, assert-new-keys passed).
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK. NOT run under tauri dev.

- 2026-09-25 (P5 McpManager retired into ConnectorCenter — P5 COMPLETE):
  - Deleted `src/lib/components/McpManager.svelte` (1211 lines, legacy per-bot MCP dialog).
    Pre-delete audit: invoke-command diff showed the ONLY McpManager-only command was
    `list_bots` (ConnectorCenter receives `bots` as a prop) — every MCP data command
    (list/assign/remove/env/custom) is identical, so per-bot assignments persist unchanged.
    File was git-tracked → fully recoverable.
  - Navigation instead of embedding: the 1599-line ConnectorCenter is a full panel (bot rail,
    batch ops), so per-bot entry points now jump to the Connectors hub with the bot
    preselected rather than hosting it in a dialog. `open-connectors` event gained an
    optional `{ detail: { botId } }`: Workspace's handleOpenConnectors sets
    `workspace.selectedBotId` (ConnectorCenter already adopts selectedBotId into
    currentBotId) then `goto("connectors")` — no new plumbing.
  - Call sites migrated (all previously mounted McpManager):
    - Sidebar bot action menu "MCP & Tools" row → `onSelectBot + dispatch open-connectors
      {botId}` (mirrors the adjacent connectorsHub row); modal block + 2 states deleted.
    - ChatRoomView manage-agent chooser "MCP servers" row → dispatch {botId: manageBot.id}
      + close chooser; mount block + showMcpManager state + `!showMcpManager` guard deleted.
    - SkillManager: the per-bot "MCP" button (opened McpManager) and the "Connectors" button
      became the SAME action post-retirement → merged into ONE Layers button
      (label ui.mcp, icon Layers) that closes the dialog and dispatches {botId};
      import + showMcp state + mount deleted.
  - i18n: no changes needed (sidebar.mcpTools / room.manageMcp* / ui.mcp labels reused;
    mcp.* namespace keys stay — ConnectorCenter uses them).
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P6 batch 1 — ConnectorCenter compact header + token fixes):
  - Header restyled to the OpenBot design system: the "Command Center" block (px-6 py-4,
    size-11 icon tile, text-base extrabold title, subtitle paragraph) collapsed to a single
    compact bar (px-4 py-2.5) — Layers size-4 + 13px bold title + mono count chip
    (rounded-full→rounded-md, shadow dropped). `connector.subtitle` dropped from the UI
    per no-unnecessary-text (key left in locales, harmless).
  - Bug fixes found while restyling:
    - `scrollbar-none` was a DEAD class — the real utility is `.no-scrollbar`
      (components.css); the agent strip now actually hides its scrollbar.
    - "Needs keys" active tab was `bg-warning + text-[var(--text-primary)]` — an
      invisible-text pairing on the pastel theme; now `bg-warning/15 text-warning
      border-warning/50`.
    - Agent count badge on unselected chips used raw `bg-black/40`; now token
      `bg-[var(--surface-1)]`.
    - Removed the only glow (`shadow-[0_0_12px_rgba(245,158,11,.35)]`) — design system is glow-free.
  - Agent-strip paddings tightened (mt-4/pt-3.5 → mt-2.5/pt-2.5). All functionality, invoke
    calls and modals untouched.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK. NOT run under tauri dev.

- 2026-09-25 (OpenBot text rendering & animation — perfection batch, tasks #23–#27):
  The full §2 streaming/motion spec is now wired end-to-end. Frontend-only; backend frozen
  (no invoke-command or agent-stream contract changes).
  - `streamReveal.svelte.ts` rewritten: reveal pump reads `--stream-gap`; catching up to the
    buffer stops the timer but KEEPS `active` so the blur-in tail stays on the newest word
    until `finish()` (was snapping mid-run); `track("")` kills the pump instantly;
    `finish()` = snap full + sharp; `stop()` public for teardown. ThreadView and every
    ChatRoomView lane reveal are stopped in onDestroy.
  - NEW `chat/smoothHeight.ts` action (OpenBot createSmoothHeightResize): tweens the bubble
    CONTAINER height per content growth (240ms cubic-bezier(0.23,1,0.32,1), WAAPI, reads the
    currently-animating height as next start, `data-resizing` overflow-hidden guard, skips
    first measure + reduced motion). Applied inside ChatMessageRow's agent bubble and the
    ChatRoomView lane bubbles.
  - NEW `chat/entrance.ts` action: live-append rows fade+rise 160ms; skipped when the row
    mounts inside a container flagged `.entries-static` (views toggle it around bulk history
    loads) or under reduced motion. IMPORTANT GOTCHA: a CSS `animation` on `.message-entry`
    + class suppressor does NOT work — removing the suppressor after a load re-triggers the
    keyframes on every history row; WAAPI-on-mount is the only correct mechanism here.
    components.css accordingly keeps `.message-entry` animation-free (rb-message-in keyframes
    removed; `.modal-panel` rb-modal-in 0.96→1 scale stays CSS — modals always mount fresh).
  - `ThreadView.svelte`: live streaming row now reuses `ChatMessageRow` (streamTail prop
    added to the shared row: text + tail + ghost + fullWidthAgent + smoothHeight + entrance —
    streamed and committed rows are pixel-identical; shared `liveMarkers`/`liveExtras`
    snippets keep action markers/sources/images in both pre-token and bubble states);
    done-flash fixed: `case "done"` now calls `reveal.finish()` and HOLDS the streamed text
    until sendMessage's finally reloads history (sources/images clearing moved to run
    teardown; regenerate finally clears them too). MarkdownRenderer import dropped.
  - Stick-to-bottom scrolling in BOTH ThreadView feed and ChatRoomView room feed:
    `stickToLatest` updated by onscroll (`scrollHeight - scrollTop - clientHeight <= 80`),
    token/tool/image scrolls are conditional; explicit sends/thread-opens force; scroll
    container wrapped in a `relative` div with the `.jump-latest` pill (CSS ready, new
    `thread.jumpLatest`/`room.jumpLatest` i18n keys in all 6 locales) that smooth-scrolls
    (auto under reduced motion) and re-sticks.
  - `MarkdownRenderer.applyStreamTail` rewritten with `resolveTailTarget`: tail span can only
    land inside inline hosts (p/li/blockquote/h1-6 → element; table → last cell; pre → code;
    unresolvable → skip) — no more broken DOM from injecting a span into <table> or
    monospace text into <pre>.
  - ChatRoomView lane nits: lane bubble max-w now matches persisted rows
    `max-w-[min(80%,720px)]`; duplicate shadowing `{@const laneReveal}` removed; lanes keyed by
    `m.bot?.id ?? ""` resolve to an inert shared reveal instead of leaking a ""-keyed live
    lane; staticEntries wrapping around load()/send/dispatchPlan history swaps; same jump-pill
    + stickiness treatment.
  - NOTE: the `/tmp/openbot` reference clone no longer exists on disk — future reference
    lookups must use §2/§5 analysis notes or upstream github.com/nightly-labs/openbot.
  - NOTE for P7: §2 says OpenBot prose line-height 1.625 but our bubble CSS ships the
    20px-leading tokens; left as-is deliberately, flag in the visual pass.
  - Deferred (signature-motion candidates, not blocking): digit-roll counters for telemetry
    numbers, per-word markdown block-reuse instead of parse-per-tick, full 44px→36px jump-pill
    count morph.
  - Verified: svelte-check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P6 batch 2 — Routines re-home + header dedupe):
  - RoutinesPanel now OWNS its pane header (the OpenBot ConnectorCenter pattern):
    Clock size-4 + `text-[13px] font-bold` title + routine-count chip +
    scheduler status chip, right side = optional bot SimpleSelect + New button.
    Removed the old uppercase 11px mini-header.
  - De-duplicated titles at BOTH hosts: Workspace `dest === "routines"` lost its
    46px bar entirely (panel header carries title + bot switcher now; content
    wrapped `px-4 py-3` + `max-w-3xl`); ThreadView routines modal lost its raw
    "{bot.name} Routines" header row (close-✕-only row remains) — a P7 leftover
    raw string eliminated on the way.
  - New OPTIONAL panel props `bots`/`onBotChange` (switcher renders only when
    provided with >1 bot) — modal hosts pass neither, so no behavior change
    there. Backend untouched: same invoke commands, same payloads.
  - Token fixes: Create button was `bg-[var(--surface-light)] hover:bg-white`
    (raw white hover) → brand `bg-[var(--brand)] hover:bg-[var(--brand-hover)]
    text-[var(--text-on-light)]` per standing directive; list scroller got
    `no-scrollbar`.
  - Raw Workspace empty-fleet string → new `routines.needBot` key in ALL 6
    locales (/tmp/i18n_add25.py, merge-only).
  - Removed now-unused SimpleSelect import from Workspace.svelte.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P6 batch 3 — Memory panel restyle + mangled-class fix):
  - `OfficeMemoryPanel.svelte` header now follows the compact pane pattern
    (Brain size-4 + `text-[13px] font-bold` title + bare count chip in
    brand-soft mono): dropped the boxed 28px icon tile, the uppercase mono
    mini-title, the `memory.desc` subtitle line and the Badge count text.
  - Token sweep: relevance stat was raw `text-pink-300` → `text-[var(--brand-text)]`;
    search button had a dead hover (`hover:bg-[var(--surface-3)]` = base bg) →
    brand border + primary-text hover (established chip-hover pattern); list
    scroller got `no-scrollbar`; stray trailing space in the card class fixed.
  - DEAD-CODE FOUND & FIXED (real defect from an earlier token sweep):
    `SkillManager.svelte` memory tool colors contained a mangled class —
    `bg-[var(--surface-2)]uchsia-950/40` (a find/replace ate `bg-f` mid-string).
    Both `memory_save`/`memory_recall` entries re-painted as proper
    brand-soft/border tokens. Grep for `)]<letters>` proved these were the
    only mangled classes project-wide.
  - Import hygiene in OfficeMemoryPanel: unused `* as Card`, `cn`, and seven
    unused lucide icons removed (Badge/SimpleSelect/Input/Textarea/Button stay).
  - Left on purpose: hardcoded English tool-catalog names ("Memory Save" …) —
    the WHOLE catalog array is raw strings; converting 2 of ~25 lines would be
    inconsistent. That's a P7 catalog sweep item. `memory.desc`/`memory.count`
    keys are now unused but harmless (locales stay parity-checked, not
    usage-checked).
  - Backend untouched; same invoke commands. Verified: check 0 errors/0
    warnings, vitest 30/30 (6 files), build OK. NOT run under tauri dev.

- 2026-09-25 (P6 batch 4 — Sync panel re-home):
  - `SyncPanel.svelte`: compact pane header (Boxes size-4 + `text-[13px]
    font-bold` title + NEW bot-count chip; signed-badge chip kept), removed the
    uppercase mono mini-title and the `sync.desc` subtitle paragraph; import
    sub-section label de-uppercased (text-[12px] bold, icon size-3.5); list
    scroller got `no-scrollbar`.
  - Token fix: Verify-Import button was `bg-[var(--surface-light)]
    hover:bg-white` (raw white) → brand `bg-[var(--brand)]
    hover:bg-[var(--brand-hover)] text-[var(--text-on-light)]` (now the last
    known raw-white-hover button in these secondary panels).
  - UX: export failure used native `alert()` → design-system `notify(...,
    "error")` toast ($lib/toast).
  - Header de-dupe like batch 2: ThreadView Fleet Sync modal's title row
    (Boxes + t("thread.fleetSync")) removed → close-✕-only row; SyncPanel's
    own header is the single title. `Boxes`/`thread.fleetSync` remain in use
    by the rail button tooltip.
  - Backend untouched; bundle JSON handled as opaque text (signature/pubkey
    presence check only), same invoke commands.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P6 batch 5 — Computer panel invisible-text fix):
  - REAL BUG: Start/Open-Desktop buttons were SOLID pastel mint
    (`bg-success hover:bg-success text-[var(--text-primary)]`) — light text on
    a light pastel = unreadable, plus a dead hover (same bg). Replaced with the
    project's canonical soft-success button (ChannelsPanel pattern):
    `bg-success/15 border border-success/40 text-success hover:bg-success/25`.
  - Warning + danger banners in the desktop tab used the invisible-text pairing
    `bg-warning|danger/30 + border-*/20` → standing directive combo
    `bg-*/15 text-*/ border-*/50`.
  - Screen-preview letterbox `bg-black` → `bg-[var(--surface-0)]` (last raw
    black in the panel; keeps the neutral mat around captures).
  - Rest of the panel audited clean already (token header, soft tab pills,
    capability strip, modal-panel scale-in inherited). No invoke/contract
    changes; capability strings are labels only.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P6 batch 6 — Settings modal restyle):
  - Shell (`Settings.svelte`): hand-rolled `rounded-xl border … shadow-[var(--shadow-xl)] bg-[var(--surface-1)]`
    replaced with the shared `.modal-panel` class → dedupes modal styling and
    inherits the rb-modal-in scale entrance every other modal has. Scrim stays
    `bg-black/60` (app-wide convention). Dynamic `style=` theme-hex bindings in
    the Themes sidebar cards are ALLOWED (CI only bans `<style>` blocks) — left.
  - Raw palette classes → semantic tokens: nav + section-header icons
    `text-pink-400` → `text-info`, `text-green-400` → `text-success`
    (`--info` token exists in tokens.css and `text-info` already used by
    ChatRoomView chips).
  - Five raw-black surfaces tokenized: model-list chip `bg-black/20` →
    `bg-[var(--surface-2)]`; budget bar track `bg-black/40` →
    `bg-[var(--surface-0)]`; connector model-name chip `bg-black/40` →
    `bg-[var(--surface-1)]` (ConnectorCenter precedent); `ollama pull` code well
    `bg-black/40` → `bg-[var(--surface-0)]`.
  - `no-scrollbar` on all scrollers: left nav, right-pane content divs (×8),
    model-list.
  - API-key save code path (`saveKey` / `set_provider_api_key`) NOT touched —
    out of UI scope, values never echoed. "RAVENBOT v0.2.0" literal at ~:454 is
    a P7 i18n leftover, logged not fixed.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P6 batch 7 — Bot settings restyle; P6 COMPLETE):
  - `BotSettings.svelte` is a bits-ui `Dialog`, NOT a hand-rolled overlay, so it
    does NOT get `.modal-panel` (would double-animate against the base's
    `data-open:animate-in … zoom-in-95`); instead the shell's brand-tinted
    border → `border-[var(--hairline-strong)]` (modal border standard) and the
    content scroller got `no-scrollbar`.
  - Delete-zone raw red palette classes (`bg-red-950/40 text-red-400 …`,
    `bg-red-600 hover:bg-red-500`, `ring-8 ring-red-900/20` glow ring) removed:
    the `variant="destructive"` base ALREADY implements the canonical soft
    pattern (`bg-destructive/10 text-destructive hover:bg-destructive/20`), the
    custom classes were overriding tokens WITH raw palette; confirm-icon tile
    → `bg-destructive/15 text-destructive border-destructive/40`.
  - Banner pairings fixed to the standing directive (`bg-*/15 text-*/
    border-*/50`): full-access warning + sandbox report (success+warning
    branches).
  - Kept: selection-tile active fills `bg-warning/30 ring-warning/40` /
    `bg-[var(--brand-soft)] ring-*/40` (active-state affordance, not a banner);
    uppercase `Label` form-field labels (body pattern, not pane headers).
  - Behavior leftover logged, NOT fixed: `confirm(t("bot.discardConfirm"))`
    (~:352) is a native dialog; `$lib/toast` has no action buttons so a styled
    confirm needs a small new primitive — P7 candidate.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P7 i18n batch 13 — chat-surface leftovers):
  - New keys via /tmp/i18n_add26.py + add27.py (7 room + 1 thread + 1 new room
    title; all 6 locales, en-parity test green): room.parallelLane,
    room.specialistAssigned / specialistsAssigned ({{n}}, singular/plural kept),
    room.defaultDesc, room.hireFirstPh, room.taskPh ({{name}},{{n}}),
    room.workspaceTitle ("{{name}} Workspace"), thread.configureApiKey.
  - ThreadView model-error CTA: raw "Configure API Key in Settings (⌘,)" → t();
    bonus token fix on same button (`bg-[var(--surface-light)] hover:bg-white`
    raw-white hover → brand CTA pattern `bg-[var(--brand)] hover:bg-[var(--brand-hover)]
    text-[var(--text-on-light)]`, dropped `shadow`).
  - ChatRoomView: header chips (Parallel Lane / N specialists assigned), office
    default description, empty-office + task composer placeholders, and
    "{room.name} Workspace" empty-state title all through t().
  - Already done before this batch (verified, stale backlog notes): Sidebar
    fleet-activity labels (`fleet.working|attention|replied`) and
    ChatActionMarker verbs (`tool.done|running`) — nothing to change.
  - "RAVENBOT v0.2.0" (Settings ~:454) deliberately NOT converted: brand +
    version are data, not prose; wire to package version in P7 closeout instead
    if desired.
  - Remaining P7 i18n (heuristic scan of >2-word literals in templates):
    AvatarPicker (5 strings), ChatRoomList (5), ChatRoomView empty-state trio
    ("This office has no team yet"/"Staff this office"/"CEO / ORCHESTRATOR"),
    OfficeSettings (5 section descs), ThreadView (LOCAL HARDWARE ENCLAVE,
    Thread History, explore prompt), Workspace boot line, WorkspaceSidebar
    "Fleet navigator", SkillManager 20-tool catalog (names+descriptions+
    permissions; backend `list_all_skills` strings need a frontend id→key map —
    biggest single chunk).
  - Injector gotcha re-learned: guard is `assert loc in per_loc` (locale key),
    not `assert k in per_loc` — wrong form fails on the FIRST key with a
    misleading "missing translation" error and writes nothing.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P7 i18n batch 14 — AvatarPicker/ChatRoomList/office empty-state):
  - New ns `avatar` (10 keys) + office (5) + room (5 incl. ceoStaffDesc) × 6
    locales via /tmp/i18n_add28.py + add29.py. AvatarPicker and ChatRoomList
    previously had NO `t` import — added.
  - Bonus token fixes found en route: AvatarPicker `custom url` chip
    `bg-success/80` (invisible text) → /15+border/30; ChatRoomView no-team
    banner → /15+/50 pairing; FOUR more instances of the batch-5 dead-hover
    solid-pastel button (`bg-success hover:bg-success text-[var(--text-primary)]`)
    fixed to the soft-success pattern: ChatRoomView ×2 (no-team card + hire
    modal footer), KillSwitch resume, OfficeSettings save-success state.
    Repo-wide grep for the pattern now returns zero.
  - Logged, NOT fixed (each is its own data-localization problem):
    AvatarPicker category chips (:55 list is BOTH display and filter key —
    needs key/label split), `dicebearStyles()` labels/descriptions and
    `OFFICE_TEMPLATES` names/ranks/specialties in `src/lib/utils.ts`
    (backend-seeded org blueprints), SkillManager 20-tool catalog (needs
    id→key map over `list_all_skills` results), KillSwitch + OfficeSettings
    raw red-*/black palette classes → batch 15 (task #35).
  - "DiceBear 9.x" version-chrome dropped from the style-library label.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P7 batch 15 — KillSwitch + OfficeSettings palette/i18n):
  - KillSwitch.svelte raw-red zone → semantic: active banner → danger /15+/50
    pairing, trigger pill `red-500/[0.06]` etc → `danger/5|10|25|50`, confirm
    icon tile → `bg-danger/15 border-danger/40` (glow `ring-8 ring-red-900/20`
    deleted), solid `bg-red-600` confirm button → plain `variant="destructive"`
    (already the soft-destructive pattern in ui/button).
  - OfficeSettings.svelte: `getTemplateTint` raw hex backgrounds + red/pink/
    fuchsia borders → semantic /10 tints (destructive/info/warning/success/
    brand; default neutralized to surface-2 — NOTE the old function had a
    DEAD duplicated default branch after `}`, removed while rewriting);
    four `bg-black/40` wells → surface tokens; modal + delete-dialog borders
    brand/red-500 → hairline-strong / danger tints; all `text-red-400/300`
    icons → text-danger, `text-pink-400` → text-info; hover-red remove-agent
    button → danger/10 soft hover; footer delete + confirm-delete buttons →
    canonical soft-danger pairings; content scroller `no-scrollbar`.
  - Kept: avatar hover-veil scrim `bg-black/60` (:410) — same family as modal
    scrims; "N/A" and "e.g. 500" (universal/data-ish).
  - i18n: 8 new `office.*` keys × 6 locales via /tmp/i18n_add30.py (tab
    descriptions policyDesc/termsDesc/budgetDesc/goalDesc/goalHint, hardStop
    ({{v}}), uncapped, perAgent ({{v}})); footer "Cancel" → t("ui.cancel").
  - Logged NOT fixed: POLICY_TEMPLATES/TERMS_TEMPLATES title+content bodies
    (5 long English prose blobs that get INSERTED into user-editable fields —
    localizing them is a data-vs-UI decision, and `OFFICE_TEMPLATES` in
    utils.ts is the same problem); KillSwitch "Manual trigger" reason string
    is persisted to the backend as event data — translation would rewrite
    history semantics; `|| "Custom Office"` fallback badge label (custom
    template exists in utils, fallback nearly-dead).
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P7 batch 16 — ThreadView/Workspace/WorkspaceSidebar i18n):
  - ThreadView.svelte: telemetry-strip badge "LOCAL HARDWARE ENCLAVE" →
    `t("thread.enclaveBadge")` (sentence-case value + `uppercase` class added,
    render identical); drawer header "Thread History" → REUSE existing
    `t("thread.historyToggle")` ("Thread history" + CSS uppercase — no new key);
    empty-state "What would you like to explore?" → `thread.exploreTitle` and
    the bot-description fallback "Sovereign desktop agent ready…" →
    `thread.defaultBotDesc`.
  - workspace/Workspace.svelte boot screen: "Initializing {brand} Runtime" →
    `t("workspace.boot", { brand })` (new `workspace` ns, {{brand}} param) and
    "Connecting local engine and database..." → `workspace.bootDesc`.
  - workspace/WorkspaceSidebar.svelte: `aria-label="Fleet navigator"` →
    `t("sidebar.fleetNavigator")` (added t import).
  - Post-batch rescan of the touched files caught the composer mode pills:
    "Think" (ThreadView :2184 + workspace/HomePane.svelte :168) → new
    `home.thinkPill` × 6 locales. KEPT as-is: "DeepSearch" pill label —
    product feature name (all 6 locales' `home.deepSearch` tooltips already
    leave it untranslated) and its `[DeepSearch]` message prefix is backend
    protocol data.
  - Repo-wide rescan after the pills caught two more own-code literals,
    fixed: Toaster.svelte `aria-label="Dismiss"` → `ui.dismiss` (added t
    import) and ChatRoomView manage-modal "LEAD" orchestrator badge →
    `room.leadBadge` + `uppercase` class (existing `room.ceoBadge` is
    "CEO / ORCHESTRATOR", a different string). Rescan hits KEPT: CommandPalette
    "RAVENBOT Core" and Settings "RAVENBOT v0.2.0" (brand strings; version
    already logged), `ui/dialog` Close defaults (vendor shadcn primitives —
    don't hand-edit).
  - Logged NOT fixed (new finding): "Agent"/"Generalist" fallbacks are MIXED
    between display copy (ChatRoomView:1490 rank/specialty line,
    AvatarPicker:128 seed label) and DATA seeds persisted through the backend
    (getDiceBearUrl seeds, `name: String(r?.name || "Agent")` fed into
    provisioning, CreateChatRoom:410 / OfficeSettings:289 member-specialty
    defaults). Localizing the data-side ones would translate values written
    into agent records; needs a display-vs-data audit as its own mini-batch,
    NOT a blind sed.
  - 6 new keys × 6 locales via /tmp/i18n_add31.py, all grep-verified (6/loc)
    (+3 more injected inline: home.thinkPill, ui.dismiss, room.leadBadge —
    grep-verified 1/loc each).
  - After this batch the heuristic template-literal scan is EXHAUSTED for
    hand-written chrome (all survivors logged KEPT above); remaining i18n is
    only: SkillManager `list_all_skills` names/descs/permissions (needs a
    frontend id→key map, ~240 strings, own batch); utils.ts
    dicebearStyles/OFFICE_TEMPLATES labels + POLICY/TERMS bodies (needs a
    data-localization decision); and the "Agent"/"Generalist" audit above.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P7 batch 17 — "Agent"/"Generalist" display-vs-data audit):
  - Audited all 30 `"Agent"`/`"Generalist"`/`"Member"` literals repo-wide.
    Display-side (keyed, 7 new keys × 6 locales via /tmp/i18n_add32.py):
  - NEW finding beyond batch 16's list: workspace.svelte.ts feeds the
    ScreenReader live region (Workspace.svelte :291) with 4 hardcoded English
    status strings → new `sr` namespace: fleetLoadFailed, botCreated
    ("Agent {{name}} created" — kept wording parity), botUpdated, botDeleted
    ("{{name}} deleted", unknown-name fallback via ui.fallbackAgent); file
    now imports `t` from $lib/i18n (plain function call at event time —
    locale-reactive because t() reads localeState at call).
  - ui.fallbackAgent/fallbackMember/fallbackGeneralist wired into pure
    display fallbacks: ChatRoomView :883 (nameFor) + :1490 (rank·specialty
    line), CreateChatRoom :413 (assignRank param), AvatarPicker :128 (seed
    label).
  - Data-side — KEPT untranslated (values are persisted through the backend
    or feed deterministic generators): all getDiceBearUrl seed fallbacks
    (utils.ts:48, Sidebar :194/:680, BotSettings :235, AvatarPicker props/
    url builders), creation/provisioning payloads (CreateChatRoom :410
    toggleMember args, OfficeSettings :84-85/:271/:288-289, ChatRoomView
    :142-144 record normalization + :178 new-member template), and
    AgentIntelligence :11 prop default (dead — only caller ThreadView :2304
    always passes bot.name).
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P7 batch 18 — SkillManager native-tool catalog i18n):
  - SCOPE REALITY: the "20-tool catalog" estimate was wrong — Rust
    `SkillRegistry::default()` registers 34 named built-ins + awesome_fetch
    + 1,497 generated "awesome/community" skills (crates/skills/src/
    registry.rs). Translating community ids is out of the question (~9k
    strings) and they are third-party data anyway → catalog i18n covers the
    35 product built-ins only; community rows render the raw backend strings
    by design (same policy as plugins/marketplace).
  - Extracted id/name/description from all `impl Skill` blocks with a python
    pass (one Rust string = one source of truth; multi-line descriptions
    rewritten concisely for en values). Injector /tmp/i18n_add33.py wrote
    `skills.catalog.<id>.{name,desc}` × 35 + `skills.perm.<Variant>` × 9 +
    `skills.toggleSkill` into ALL 6 locales (nested dicts; 420+ strings).
    Verified by json walk (35/9/1 per locale).
  - GOTCHA (tests): i18n.test.ts `enKeys()` is a HARDCODED probe list, not a
    real walk of en.json — new keys do NOT get parity-enforced by CI. Manual
    verification pass required when adding keys (done via json walker).
  - SkillManager.svelte rewired: dropped the 20-entry hardcoded English
    placeholder array (transient pre-invoke state, duplicated catalog) →
    `availableSkills = []` + `t("ui.loading")` empty row; render-time lookups
    `catalogText(id, field, raw)` / `permLabel(p)` (key-probe: t() returns the
    key on miss → fall through to raw backend string; locale-switch reactive
    since t() runs in template).
  - BUG FIXED (was never noticed): permission chips rendered raw Rust Debug
    output (`FileSystem { paths: ["/"] }`) — `list_all_skills` sends
    `format!("{:?}", p)`; frontend now extracts the variant token and shows
    `skills.perm.<Variant>`, unknown variants still fall through raw.
  - Same-file palette sweep: colorMap red/orange/yellow/green-400+950 tints →
    danger/warning/success tokens (+web_search/file_read/file_write/
    shell_exec/delegate entries added — they had colors only in the deleted
    placeholder array and would have silently gone neutral); toggle knob
    `bg-white` → `bg-[var(--surface-light)]`; save button dead-hover
    `hover:bg-white` → canonical brand pattern; "ON" badge → `t("ui.on")`,
    "Cancel" → `t("ui.cancel")`, `Toggle ${name}` aria →
    `t("skills.toggleSkill", { name })`; Dialog.Content +no-scrollbar.
    save()/toggleSkill/update_bot invoke flow untouched.
  - Verified: check 0 errors/0 warnings, vitest 30/30 (6 files), build OK.
    NOT run under tauri dev.

- 2026-09-25 (P8 batch 1 — custom providers backend):
  - Migration `018_custom_providers.sql`: `custom_providers` (id/display_name/
    kind openai|anthropic|ollama/base_url/default_model/supports_tools/enabled)
    + `provider_base_urls` (opt-in built-in override; absence = compiled
    default). GOTCHA: the migrations runner splits SQL on `;` — statements
    MUST be semicolon-terminated (newline-only separation → "near CREATE:
    syntax error" and 6 tier4 tests failing on temp_pool).
  - `ModelProvider::Custom` added (core/model.rs); compiler surfaced the
    manager provider_str match + models/lib.rs factory arms.
  - `ProviderManager` gained `custom: HashMap<String, CustomProviderSpec>` +
    `register_custom/remove_custom/custom_spec/custom_provider_ids`;
    `create_provider_from_str_with_model` now resolves custom ids BEFORE the
    built-in match (ids validated against `is_reserved_provider_name`, so
    built-ins can never be shadowed). `build_from_spec`: anthropic kind reuses
    `AnthropicProvider::with_base_url` (new), ollama kind reuses
    `OllamaProvider`, openai kind → new `openai_compat::custom_provider`;
    bot `model_id` overrides spec `default_model`.
  - Built-in base-URL overrides route through new
    `openai_compat::openai_compat_with_base` (keeps built-in default model)
    only for the 12 genuinely openai-compatible built-ins + anthropic;
    openrouter/mimo/commandcode/local ignore override rows. Without a row the
    construction path is byte-identical. Empty-string override is filtered.
  - Discovery: `AuthStyle` enum replaces the name-string auth match in
    `fetch_from_provider`; shared `fetch_with_endpoint`; new
    `fetch_models_for(provider, key, base_url, kind)` (ollama kind → /api/tags,
    keyless). `fetch_provider_models`/`fetch_all_provider_models` command
    signatures unchanged, bodies consult `custom_spec`.
  - 6 new src-tauri commands registered: list/upsert/delete/test_custom_provider,
    set/get_provider_base_url. upsert validates slug `^[a-z][a-z0-9_-]{0,31}$`
    + rejects reserved names; delete returns in-use bot count and refuses
    without `force` while bots reference the provider; test = one-token
    `complete()` with 30s timeout. Startup hydration loads both tables into
    the manager (setup block, after provider keys + ollama_url).
  - DB: `CustomProviderQueries` + `ProviderBaseUrlQueries` in queries.rs
    (tuple rows — this crate avoids sqlx::FromRow derives).
  - Tests: manager unit tests (per-kind resolution, case-insensitive id +
    scoped resolution, reserved-name rejection, built-in parity with/without
    override row, openrouter ignores override); full `cargo test --workspace`
    green; clippy — no new warning classes. NOT run under tauri dev.
- 2026-09-25 (P8 batch 2 — custom providers UI):
  - `src/lib/model-catalog.ts`: `CatalogProvider` gained optional
    `custom/kind/base_url/supports_tools`. New `fetchCustoms()` invokes
    `list_custom_providers` (catch → []), maps enabled rows to catalog entries
    (icon 🧩, description = base_url, `keyless` when kind=ollama, single
    fallback model from default_model carrying supports_tools). `getCatalog()`
    rewritten: base catalog → `curate()` → append customs (id-collision-safe,
    case-insensitive). Customs therefore propagate to every picker that reads
    the catalog (Settings grid, BotSettings, ModelPicker) with no call-site
    changes. Edit nearly deleted the `discoveredCache` declaration — restored.
  - `ModelPicker.svelte`: universal escape hatch — free-form "enter model id"
    Input + Use-ID button (Enter key works) calling onSelectModel with the
    trimmed id, available for ANY provider, not just customs. Added
    `customBadge` chip in the models-pane header and a `noToolsWarn` banner
    when `activeProvider.custom && supports_tools === false`.
  - `Settings.svelte`: "Custom Providers" management card in the providers
    section (add/edit/delete/test). Dialog reuses the ConnectorCenter
    Dialog.Content pattern; fields = display name (existing
    `settings.displayName` key reused), kind SimpleSelect, base URL, default
    model, write-only API key (blank on edit = keep), supports-tools checkbox
    (`accent-[var(--brand)]`; no ui/switch exists), Test button
    (`test_custom_provider`), Save (`upsert_custom_provider` with
    `{ provider: {...} }` and snake_case NESTED fields — Tauri only
    camelCases top-level args). Provider id derived from the display name via
    `customIdFromName` (slug, "p-" prefix if not letter-initial). Delete is
    two-phase: plain confirm, then on the in-use Err it re-confirms and
    retries with `force:true`. After any save/delete,
    `refreshProviderSurfaces()` reloads customs + `getCatalog(true)` + keys.
  - i18n: 25 keys × 6 locales (`/tmp/i18n_add34.py`) — settings.customProviders
    …forceDeleteQuestion/testReply, model.enterModelId/useId/noToolsWarn/
    customBadge/noToolsShort. `settings.displayName` collided with an existing
    key → injector dropped it and the code reuses the shipped key; parity walk
    printed ok for es/fr/de/ja/zh.
  - Fix: `<span />` self-closing placeholder in the dialog footer tripped
    svelte-check (element_invalid_self_closing_tag) → restructured to a
    conditional Test button + `ml-auto` buttons div.
  - Verification: `npm run check` 0 errors / 0 warnings; `npm test` 6 files
    30/30; `npm run build` "Wrote site to 'build' ✔ done".
    NOT run under tauri dev.
- 2026-09-25 (dialog horizontal-overflow fix — SkillManager "window issue"):
  - Symptom: skills dialog rendered with rows clipped mid-word on the LEFT,
    icons/names invisible, and the built-in ✕ appearing top-LEFT. Root cause:
    `Dialog.Content` is a `grid` with an implicit `auto` column; the restyled
    rows' `truncate` (white-space:nowrap) descriptions made that column grow
    to max-content (~1037px in a 574px panel), so the panel overflowed
    horizontally and ended up scrolled right — dragging the absolute
    `right-2` close button with it. Old (pre-restyle) rows wrapped text, which
    is why this only appeared after the P7-era redesign.
  - Fix (global, in `src/lib/components/ui/dialog/dialog-content.svelte`):
    base class gains `grid-cols-[minmax(0,1fr)]` so grid items can never size
    past the panel; verified in a headless-chromium repro against the built
    CSS (scrollWidth 1037 → 574, ✕ back at top-right, rows truncate cleanly).
    SkillManager header also got `pr-8` so Plugins/MCP buttons clear the ✕.
  - GOTCHA: any `truncate` text directly inside a `Dialog.Content` grid item
    needs the item chain to have `min-w-0`; the grid-column cap is the safety
    net for the Content itself.
  - Verification: `npm run check` 0/0; `npm test` 30/30; `npm run build` done.
    NOT run under tauri dev.
  - FOLLOW-UP: user still saw the bug after "restarting the app" — they run
    the Sept-25 `src-tauri/target/release/ravenbot` binary, which embeds the
    frontend at compile time, so source/build changes never reach it. Proved
    the fix correct against the REAL component by mounting `SkillManager.svelte`
    in a headless-chromium vite harness (stubbed `invoke`, full Tailwind CSS):
    scrollWidth == clientWidth == 574, ✕ top-right, rows intact. Remedy:
    `npx tauri build` (or run `tauri dev`). GOTCHA: after any frontend fix,
    the release binary must be rebuilt or the user sees stale UI.
- 2026-09-27 (P4 deferred icons + rail rollup — closes P4 except drag-reorder):
  - Sidebar.svelte avatar badge: `working` now shows a pill (h-3.5,
    `bg-[var(--surface-3)]` + `ring-2 ring-[var(--surface-1)]`) holding 3×`.typing-dot`
    colored via `text-[var(--brand)]` (dots use `background: currentColor`; animation and
    reduced-motion already in components.css); `attention` shows a `TriangleAlert`
    (lucide, imported) at `size-[9px] text-[var(--warning-text)]` in a size-4 ring,
    `animate-pulse`. `responded`/idle keep the plain size-2.5 dot. Badges are
    `aria-hidden` — the label line already conveys state via translated `fleet.*` keys.
  - WorkspaceRail.svelte: `$derived` rollups `attentionCount`/`workingCount` over
    `fleetActivity.states` values (reactive through the class `$state` record). On the
    Offices section button: attention → `bg-[var(--warning-text)]` count pill (9+ cap)
    top-right; else working → pulsing `bg-[var(--brand)]` dot. aria-label appends
    `· {n} · {t("fleet.attention")}` when attention>0 (reuses existing i18n keys; no new
    strings invented).
  - Already done, verified this pass: empty states (`sidebar.noBots`,
    `office.emptyTitle/emptyDesc`), pin (`set_bot_pinned` + sort priority).
    Drag-reorder still deferred: needs a persistent order column + migration; Rust side
    is frozen except P8 additive scope, so it waits for an explicit go-ahead.
  - Verification: `npm run check` 0/0; `npm test` 30/30; `npm run build` "Wrote site to
    'build' ✔ done"; release binary rebuilt via `npx tauri build` so the binary the user
    launches contains this. NOT run under tauri dev.
- 2026-09-27 (Command Code provider system fix + ThreadView top-bar overlap):
  - Verified live: `GET https://api.commandcode.ai/provider/v1/models` is PUBLIC
    (no key) — 82 models, fields `id,name,context_length,supported_endpoints`
    (NO pricing). Claude ids are `/messages`-only; free models are id-shaped
    (`…:free`, `…-free`, `stealth/…`). Snapshot kept at /tmp/cc_models.json.
  - BUG (crates/models/src/commandcode.rs): the trait `with_model` (the runtime
    path used by ProviderManager) overwrote `model_id` WITHOUT re-running the
    claude→/messages auto-detect — switching a Command Code bot to Claude sent
    OpenAI-shaped requests to /chat/completions → 400. Fixed by delegating the
    trait method to the detecting builder. Also: health_check returned true on
    401/4xx (now requires success status); added ZDR support via env `CMD_ZDR=1`
    → `x-cmd-zdr: 1` header on both wire formats (mirrors the CLI).
  - Discovery (crates/models/src/discovery.rs): new `AuthStyle::BearerOptional`
    so commandcode `/models` works keyless (picker/Settings populate the live
    82-model list even before a key is saved); new per-endpoint
    `free_by_id_only` flag — commandcode omits pricing, and the old
    `cost==0 ⇒ free` rule would have marked ALL models free; free now comes
    from the id suffixes above. Tests added (14 pass in ravenbot-models).
  - Frontend (src/lib/model-catalog.ts): added `CURATED.commandcode` (verified
    2026-09-27 against the live list) — default `deepseek/deepseek-v4-flash`,
    top models (Claude Sonnet/Opus 5, GPT-6 Sol, Gemini 3.5 Flash, Kimi K3,
    Grok 4.7, GLM 5.3, DeepSeek V4/V4.1 Flash) + the 4 free models, with real
    context windows; replaces the frozen backend list via `curate()` and the
    stale minimalCatalog entry. No pricing invented (unknown = "—").
  - ThreadView.svelte header overlap (user screenshot: thread title bleeding
    into "$0.0000" cost pill): left cluster `flex-1 min-w-0`, right controls
    `shrink-0`, header `gap-3 overflow-hidden`; thread-switcher button gets
    `min-w-0` + truncatable label; "…Intelligence" label now `hidden xl:inline`
    (icon-only at narrower widths); channel option label now uses
    `t("thread.noChannel")` instead of a hardcoded string.
  - Verification: cargo test --workspace 0 failures; `npm run check` 0/0;
    `npm test` 30/30; `npx tauri build` exit 0 (fresh binary at
    target/release/ravenbot). NOT run under tauri dev.
- 2026-09-25 (P7 a11y pass — WCAG AA across all panels):
  - Explore-agent audit over ~20 components found 6 gap categories: unnamed
    icon-only interactives, mouse-only clickables, missing toggle state,
    silent status regions, dead focus rings, reduced-motion/contrast.
  - src/app.css: `prefers-reduced-motion` block extended with the universal
    `*`/`::before`/`::after` animation+transition neutralizer, and a new
    `html.reduce-motion *` mirror block — this gives the previously DEAD
    `.reduce-motion` class (Workspace.svelte toggles it on `<html>`) real
    meaning; one CSS change covers ~54 `animate-*` usages.
  - ThreadView: header icon-buttons aria-label/aria-pressed, model-switcher
    aria-expanded+hasPopup + custom-id input label+ring, thread dropdown
    aria-expanded/aria-pressed, composer + question-answer inputs labelled,
    approval banner role="alert" / question banner role="status", attachment
    ✕ names, send/mic/voice/temp/auto-read toggles, drawer thread-list ring.
    NEW: `announce()` (from $lib/a11y) fires on status transitions to
    emulating_tools / waiting_on_user / paused (labels come from
    getStatusTheme, so they're translated) and on approval_requested — this
    is the first in-component use of the announce helper outside workspace.
  - ConnectorCenter: tab bar + chips + select-checkbox + global/enable
    buttons aria-pressed, more-menu aria-expanded+label, transport segment
    → role=radiogroup/radio/aria-checked, ALL custom-server modal fields
    for/id paired (id/icon/name/desc/url/headers/command/args/env), search
    + clear-X + Eye ("Show/Hide token") + env input aria-label={key NAME}
    (variable name only — values never surfaced), save-error role="alert".
  - Settings: nav sections + theme cards aria-pressed, provider search
    aria-label, custom-provider dialog for/id pairs (cp-display/cp-baseurl/
    cp-model/cp-key) + error role="alert", profile name + locale label-for,
    custom row edit/trash aria-labels. API-key save region (saveKey/
    clearKey) deliberately NOT touched.
  - ModelPicker/PluginsStore: searches aria-label, provider/model rows +
    category chips + global/enable buttons aria-pressed, load-error
    role="alert", noToolsWarn role="status". KillSwitch banner role="alert"
    (safety-critical). ChatRoomView: dep-chips aria-pressed, role-instruction
    inputs/textarea labels, ✕ names, roleInstructions aria-expanded, notice
    strip role="status".
  - Two background agents swept the remaining long tail with the same recipe
    — group A (Sidebar, AvatarPicker, ChannelsPanel, TeamImport,
    CreateChatRoom, ChatRoomList, CommandPalette — incl. listbox/
    activedescendant wiring in the palette) and group B (OfficeSettings,
    OfficeMemoryPanel, RoutinesPanel, SyncPanel, BotSettings, ComputerPanel,
    ArtifactPanel, workspace/HomePane). CommandPalette's new `t()` calls all
    hit EXISTING `palette.*` keys (verified ×6 locales before merging).
  - GOTCHA: aria-labels must not invent i18n keys — plain English strings
    are the policy for un-i18n'd panels; one draft `t("room.removeTask")`
    was caught and replaced before check ran.
  - Verification: `npm run check` 0 errors / 0 warnings; `npm test` 6 files
    30/30; `npm run build` "Wrote site to 'build' ✔ done".
    NOT run under tauri dev.
- 2026-09-25 (P8 batch 6 — tool emulation for tool-less models + P8 CLOSEOUT):
  - Capability helper `Runtime::model_supports_tools(bot, provider_type)`:
    custom provider `supports_tools` column wins → `builtin_models()` catalog
    (model id exact or `/suffix` match) → `ModelProvider::Local` = false →
    default TRUE, so every native function-calling path is byte-identical.
  - `execute_run`: `emulate_tools` computed once after tool assembly; when
    true BOTH call_model sites receive `&[]` tools and the system prompt gets
    `tool_emulation_prompt` (protocol rules + full JSON schemas) instead of
    the plain "Tools available:" line. Status `emulating_tools` emitted once
    (StreamEvent::Status — no agent-stream payload change).
  - `parse_emulated_tool_calls`: extracts one-or-more fenced blocks whose JSON
    has a `tool_calls` array (handles ```json tag, bare ```, and single-line
    fences where the payload starts the "tag" line); synthesizes ToolCalls
    with `emu-<uuid>` ids; `arguments` or `args`; non-tool fences (real code,
    plain json answers, malformed, unclosed) survive VERBATIM. Transcript in
    emulation mode is plain text: assistant gets its own raw protocol text
    (`last_raw_content`), results ride back as user
    `[tool_result:<name>]` turns. Repeat guard: 3 identical
    `emulated_call_signature` rounds → nudge + re-query → still stuck → drop
    calls and finalize. Approvals/risk gating and all StreamEvents unchanged.
  - UI: ThreadView `getStatusTheme` case "emulating_tools" (brand pulse dot,
    `t("runtime.emulatingTools")`) + `/tmp/i18n_add36.py` created the NEW
    `runtime` namespace ×6 locales, parity ok. NOTE: locales show ~1165-line
    diffs vs HEAD — that is the whole uncommitted P7/P8 i18n body, not
    injector corruption.
  - Tests: 7 `emulation_tests` (parse/strip, fence preservation, alias args,
    signature, prompt). GOTCHA: the first parse draft only accepted JSON on
    the line AFTER the fence — single-line ```{"tool_calls":…}``` blocks were
    treated as ordinary text; fixed via the lang-starts-with-{-or-[ branch.
  - Verification: `cargo test --workspace` 33 suites ok / 0 failed; clippy
    runtime shows only the 2 PRE-EXISTING warnings; `npm run check` 0/0;
    `npm test` 6 files 30/30; `npm run build` "Wrote site to 'build' ✔ done".
    P8 COMPLETE (all 6 batches). NOT run under tauri dev.
- 2026-09-25 (P8 batch 5 — MCP servers forwarded to external engines):
  - crates/mcp/src/registry.rs: factored `enabled_server_ids(bot_id)` (the
    per-bot-else-global fallback) and `server_config_with_env(id)` (config +
    DB env + OS-env fallback) out of skills_for_bot — the native loop and the
    engine forwarder now share ONE resolution path.
  - crates/engines/src/lib.rs: new `EngineMcpServer { name, transport
    ("stdio"|"http"), command, args, env, url, headers }`; EngineRequest
    gained `mcp_servers: Vec<EngineMcpServer>` (Default = empty → every
    existing construction unchanged via `..Default::default()`).
  - crates/runtime/src/lib.rs: `engine_mcp_servers(bot_id)` helper resolves
    the bot's effective servers (cap 16, never fatal — warn + empty list),
    maps McpServerConfig→EngineMcpServer with `${VAR}` header substitution
    matching McpClient::resolve_headers; execute_engine_run passes the list
    in the request. agent-stream mapping untouched.
  - Dialects: acp.rs `session/new` now serializes stdio as
    `{name,command,args,env[{name,value}]}` and http as
    `{type:"http",name,url,headers[{name,value}]}` (pairs sorted for stable
    payloads); if `initialize` lacks `agentCapabilities.mcpCapabilities.http`
    the remote servers are DROPPED with an EngineEvent::Warning (stdio
    survives). claude.rs: per-run temp `mcp-config.json` (0600 on unix,
    deleted by TempMcpConfig Drop) + `--mcp-config` flag; write failure =
    warn-not-fail. codex.rs: `-c mcp_servers.<name>.command/args/env` and
    `.url/http_headers` overrides (JSON literals are valid TOML for these
    shapes; odd names get a quoted key).
  - Tests: 3 ACP serialization snapshots (shapes, downgrade skip, broken
    entries), 3 Claude config tests (incl. temp file deleted on drop), 3
    Codex override tests (stdio/http, skip/quote, build_args wiring), plus a
    registry `enabled_server_ids` fallback-parity test against
    skills_for_bot. GOTCHA: clippy `if has identical blocks` at
    registry.rs:23 and runtime lib.rs:2651 too_many_arguments are both
    PRE-EXISTING — do not chase them as batch-5 regressions.
  - Verification: `cargo test --workspace` all green (engines 30, mcp 20,
    incl. new tests); scoped clippy shows zero new warnings.
    NOT run under tauri dev.
- 2026-09-25 (P8 batch 4 — connectors + plugins UI):
  - ConnectorCenter.svelte: McpServerSummary interface gained optional
    url/transport. New custom-server states `customTransport` (stdio|http),
    customUrl, customHeaders. saveCustomServer now mirrors the backend URL-XOR
    rule client-side: HTTP requires a URL (`errUrlRequired`), validates the
    http(s):// scheme (`errUrlScheme`), parses `Name: value` header lines
    (≤32, `errTooManyHeaders`/`errHeaderName`), and sends an empty command;
    stdio keeps the command/args path. Payload adds url/transport/headers;
    success resets transport+url+headers too. Dialog: segmented Stdio|HTTP
    control swaps the Command/Args grid for a URL Input + Headers textarea
    (with `${VAR}`-reference hint). Server cards show a `remoteBadge` when
    transport === "http". GOTCHA: inline `[["stdio",…],["http",…]]` each
    makes `mode` type `string`, so the onValueChange needs `mode as "stdio"|
    "http"` to satisfy the union-typed state.
  - PluginsStore.svelte: `globalPlugins` Set from list_global_plugins;
    `toggleGlobal(id)` → toggle_plugin_global. Card footer gained a Globe
    chip (store.global / store.globalOn ✓, tinted success when on) beside the
    per-bot Enable button; title = store.globalHint.
  - i18n: 16 new keys × 6 locales (`/tmp/i18n_add35.py`) — connector.transport
    Label/Stdio/Http, lblUrl, phUrl, lblHeaders, phHeaders, headersHint,
    remoteBadge, errUrlRequired/errUrlScheme/errTooManyHeaders/errHeaderName;
    store.global/globalOn/globalHint. NOTE: `connector.globalOn` already
    existed and `store.*` had none — the earlier "collision" was a false
    positive from a naive whole-file grep (keys are namespaced). Injector
    asserts en-collision + full parity walk (es/fr/de/ja/zh parity ok).
  - Verification: `npm run check` 0/0; `npm test` 30/30; `npm run build`
    "Wrote site to 'build' ✔ done". NOT run under tauri dev.
- 2026-09-25 (P8 batch 3 — remote MCP + plugin backend):
  - Migration 019 (CURRENT_VERSION=19): mcp_servers gains url/transport/
    headers_json, openapi_operations gains server_base, plugins gains
    enabled_global. All statements `;`-terminated, no semicolon inside any
    literal. GOTCHA caught early: an SQL comment containing a semicolon
    would split the runner's statement stream — keep comments
    semicolon-free.
  - crates/mcp/src/registry.rs: `ensure_tables` gained three defensive
    ALTERs mirroring migration 019. New `parse_transport(stored)` maps
    'http'→Http, else Stdio. `list_servers` + `get_server_config` SELECTs
    widened to 14 columns (a `ServerRow` type alias keeps clippy's
    type_complexity quiet) and now populate real url/transport/headers
    (previously forced None/Stdio). `validate_server_for_save` → URL XOR
    command: url present (trailing '/' trimmed) ⇒ HTTP — command must be
    empty, url must start with http(s):// and be ≤2048, ≤32 headers with
    valid names ≤128 / values ≤2048; otherwise stdio keeps the old
    1-1024-command rules. `save_custom_server` persists
    url/transport/headers_json; summaries stop nulling url/transport.
    McpClient unchanged (already HTTP-capable with `${ENV}` headers).
  - crates/plugins/src/openapi.rs: store_operations INSERTs server_base;
    load_operations reads COALESCE(server_base,'') and re-derives it from
    the plugin's stored openapi_spec for legacy empty rows (parse once,
    reuse for all rows).
  - crates/plugins/src/store.rs: defensive ALTER enabled_global; new
    set_plugin_global + list_global_plugins; list_enabled_for_bot falls
    back to global plugins when a bot has zero enabled assignments
    (mirrors MCP skills_for_bot — "enable once, use everywhere").
  - 2 new src-tauri commands registered: toggle_plugin_global,
    list_global_plugins (mirror the existing plugin command family).
    ravenbot-db added to the plugins crate's [dev-dependencies] for tests.
  - Tests: mcp — http round-trip (url trim, headers, summary), URL-XOR
    rejects (both / neither / bad scheme), stdio parity; plugins —
    server_base store+reload, legacy re-derive, global fallback scope.
    ravenbot-mcp 19 green, ravenbot-plugins 3 green, full
    `cargo test --workspace` green, src-tauri 17 green; clippy shows only
    pre-existing warning classes. NOT run under tauri dev.
- 2026-09-24 (P3.5 markers + P7 i18n kick-off):
  - ChatActionMarker done (see P3.5 note above).
  - i18n made REACTIVE: new `src/lib/i18n/state.svelte.ts` holds `localeState = { current: $state }`
    (class-field $state); `index.ts` setLocale/getLocale/t read it, and re-exports `Locale` + `localeState`.
    Import MUST be `./state.svelte` (no `.ts` suffix — svelte-check's TS rejects TS-extension imports),
    and vitest.config.ts now loads the `svelte()` plugin so `.svelte.ts` runes compile under tests.
  - New key groups `rail.*`, `activity.*`, `fleet.*` added to ALL 6 locales (en/es/fr/de/ja/zh).
    The i18n coverage test walks every en key across locales, so any new key missing elsewhere fails CI.
    Script that injected them: pattern in git-less /tmp (non-persistent) — future keys: add to all 6 files.
  - Wired: WorkspaceRail (Home/Offices/Skills/Marketplace/MCPs & Connectors/Routines/Settings/Add agent/
    aria labels — `sectionItems`/`bottomItems` labels are now i18n keys, rendered `t(item.label as TranslationKey)`),
    ThreadView ACTIVITY_LINES (keys, rendered `t(activityLine)`), Sidebar fleet status (Working/attention/replied).
  - NEW: Language switcher in Settings → profile section (native `<select>` → setLocale; reads localeState.current
    so it live-updates). There was NO locale-switch UI anywhere before this.
  - NOT yet i18n'd (still English): ChatActionMarker verbs (big set), SkillManager "Tool Capabilities",
    Plugins/MCP/Connectors chips, most panel copy, CreateChatRoom/HomePane strings.
  - Verified: svelte-check 0 errors, vitest 21/21, build OK. NOT run under `tauri dev`.

- 2026-09-24 (message-render fidelity pass, from two Explore-agent digests of /tmp/openbot):
  EXTRACTED SPEC (implemented; re-derive from these values, don't re-read OpenBot):
  - Layout: scroll padding 8px 12px 24px; column cap 720px; row gap 12px, grouped (same author
    consecutive) 4px via `.message-entry` / `[data-grouped]` (components.css).
  - User bubble: bg surface-2 (#242424), uniform r22 (NEVER varies corners), pad 8px 12px,
    14px/20px, letter-spacing -0.15px, white-space pre-line PLAIN TEXT (no markdown),
    max-width min(420px, 100%-81px), right-aligned, no avatar/name in agent chat.
  - Assistant: bg surface-1 (#212121) bubble, max-width min(720px, 100%-81px); GHOST variant
    (transparent, no pad/radius) when reply is only a code fence or only a table
    (`isGhostContent()` in ThreadView).
  - Markdown (`.markdown-content` in components.css): 14/20; children margin-top 12px; h1 18/24
    h2 16/22 h3+ 14/20 all w600 ls -0.1px; ul pad 24 disc (nested circle), ol decimal, li+li 4px;
    blockquote 2px hairline-strong left + 12px pad, text-secondary; inline code mono 13px r4
    surface-1 border pad 2x4; link w560 inherit color + 1px text-muted underline offset 2;
    table: ring shadow 0 0 0 1px dialog-ring, r8, bg surface-0, th surface-2 #a1a1a1 w500,
    cells 8x12 pad, inner borders #303030, nowrap; img max-h 360 r8 contain.
  - Code block (.md-code-*): wrapper hairline r8 surface-1; header min-h 34 pad 4/6/4/10
    border-bottom; lang = mono-12 pill (bg hairline, pad 1x6); body bg surface-0 mono 12/19
    pad 8/0/10 max-h 420; copy btn h26 r6 pad-inline 7 mono-12 text-muted.
  - Streaming: word every 60ms; ONLY the newest word animates — `.stream-tail` opacity 0→1 +
    blur(2px)→0, 400ms cubic-bezier(0.22,1,0.36,1). StreamReveal exposes body+tail;
    MarkdownRenderer `streamTail` prop appends the span to the last block (recreated per word).
    NO prose caret. Pre-first-token = AgentActivity row: 32px avatar + `.agent-activity-label`
    shimmer gradient text (300% bg, rb-shine 2.25s infinite), playful line pool picked per run,
    tool name shown as "Running X…" while streamingTool set. Row enters fade+blur 240ms.
  - Meta: timestamp 12px text-muted pad-inline 12 + icon-only 24x24 r6 actions (copy/edit/
    listen/regenerate) inside `.message-meta` (opacity 0 → 1 on `.message-entry:hover`);
    hidden entirely when the NEXT message continues the same author (`continuesRun`).
  - Office lanes (channel variant): 24px avatar align-self:end, name 12px w400 in author hue
    (hsl(hue 55% 68%)), lane bubbles use same msg-bubble + streamTail.
  - TypingDots now exact: 3px, 900ms ease-in-out infinite, delays 120/240ms, -1px travel.
  - OpenBot has NO thinking block / tool rows / todo board — we keep ours (engine emits them):
    checklist accordion + think-details remain, restyled to tokens.
  - ENGINE BACKLOG (future, backend unfreeze needed): message projection like app-message-projection
    (merge streaming into final message atomically), queue panel for messages sent mid-run
    (OpenBot spec: rows above composer, 29px, r12, queue-panel-in 160ms), ChatActionMarker centered
    12px text-muted rows for routine/tool lifecycle events.
  - Verified: check 0 errors, 21/21 tests, build OK. NOT yet seen in tauri dev.
- 2026-09-24 (pastel + Fira Code pass): User wanted visible text, 8-bit pastel colors, FiraCode.
  - Contrast fix (root cause): applyTheme in theme.ts derived the text ramp by mixing muted toward bg
    too aggressively (secondary=muted #979797, muted 55%→bg, faint 75%→bg → invisible). Now:
    secondary = text mixed 18% toward muted, tertiary = muted, muted = 28%→bg, faint = 45%→bg.
    Static ramp in tokens.css brightened too (tertiary a6a6ab, muted 8b8b90, faint 6e6e73).
  - 8-bit pastel palette (all ~75-79% lightness, soft not neon): brand/accent pastel blue 79b8ff
    (hover 97c8ff, text a6d2ff), success mint 9ae6b4, warning amber ffd591, danger coral ff96a0,
    info sky 8fd4ff, thinking lavender c4b5fd, question cyan 93dcda, badge-new pink f2a6d2.
    Updated in tokens.css (status ladder, badges, ring, destructive) AND theme.ts openbot entry
    (primaryColor/accentColor/secondaryAccent/mutedTextColor) + applyTheme destructive triplet.
    Tailwind bg-success/text-warning etc. follow via the hsl triplets — component status helpers
    (getStatusTheme in Sidebar/ThreadView) needed no change, they read tokens.
  - Fira Code: installed @fontsource-variable/fira-code, imported in app.css BEFORE tokens.css;
    --font-mono leads with "Fira Code Variable". Bundled woff2 verified in build/_app/immutable/assets.
    Sans stays Inter stack — say the word to make Fira Code global.
  - NOTE: a PreToolUse hook flags 6/7-hex-digit CSS color codes as "secrets" — false positives,
    edits still apply. Don't panic on those messages.
  - Verified: check 0 errors, 21/21 tests, build OK. Still no tauri dev visual run.
- 2026-09-24 (rail nav pass): Per user screenshots —
  - Logos/badges removed: TitleBar dropped the ThemeLogo diamond + `badgeLabel` chip (wordmark only);
    WorkspaceSidebar brand strip is now pure text (no ThemeLogo).
  - WorkspaceRail no longer shows bot avatars. It is section nav: Home, divider, Offices (dest),
    Skills (Sparkles), Marketplace (Store), dashed "+" (→ agents), bottom: MCPs & Connectors (Plug),
    Routines (Clock), Settings. Skills/Marketplace are NOT new dests — they open the existing per-bot
    `SkillManager` / `PluginsStore` modals, mounted at Workspace level, targeting
    `selectedBotId || bots[0]` (no bots → goto agents). Rail props: onOpenSettings/onOpenSkills/onOpenMarketplace.
  - Gotcha: inline object arrays in `{#each}` widen string fields to `string` → `workspace.goto()` type
    error; hoist to script-level `as const` arrays.
  - Verified: svelte-check 0 errors, 21/21 tests, build OK. Still no `tauri dev` visual run.
- 2026-09-24 (later): Screenshot-alignment + clutter cleanup pass.
  - Tokens: canvas moved to OpenBot native 0x141414 (surface-0 / --background 8%), added
    `--rail-selected` (white pill), font stack leads with "Inter Variable"; theme.ts openbot bgHex synced.
  - WorkspaceRail rewritten: Home tile, divider, up to 6 bot avatar tiles (size-11 rounded-xl,
    white left pill when active, activity ring from fleetActivity via `ring-2` on the img),
    dashed "+" → agents, bottom Offices/Connectors/Routines/Settings icons. Section list is a
    script-level `as const` array (inline object array in `{#each}` widened dest to string → TS error).
  - Sidebar rows: white pill marker (in the relative wrapper, not inside the button), selected bg
    surface-1 (no border), name 13px semibold + specialty chip, status line 12.5px.
  - User feedback: structure was fine — the complaint was verbose text/chrome. Removed from
    HomePane: "Sovereign Enclave Active / Local-First • Zero Telemetry" strip, the
    subtitle+tagline line (headline is now just "What's on your mind?"), suggestion-card `desc`
    lines (cards are now compact icon+title rows), and the bottom ⌘K/⌘, button strip (only KillSwitch remains).
    Removed from WorkspaceSidebar brand strip: badgeLabel mono chip and the ⌘K button (logo + wordmark only).
    Rule going forward: quiet, minimal copy; no marketing text in the UI.
  - Verified: svelte-check 0 errors, vitest 21/21, `npm run build` OK. Still NOT run under `tauri dev`.
- 2026-09-24 (final): Layout + modal bug/color sweep (user screenshot: SkillManager gray/invisible buttons).
  - `+layout.svelte` was missing `{@render children()}` (SvelteKit warning, inner content unrendered).
    Fixed: layout = CSS import + `{@render children?.()}` only; `<Workspace />` shell moved to `+page.svelte`.
  - SkillManager.svelte template rewritten: removed stray `]` class tokens, invalid
    `border-[var(--brand)]/30 ]`, and Button-variant/`bg-[var(--brand)] text-white` conflicts (root cause of the
    gray-button-invisible-text in the screenshot). Header chips are now plain `<button>`s with token classes;
    skill rows horizontal; primary action = `bg-[var(--surface-light)] text-[var(--text-on-light)]`.
  - Project-wide perl repair pass across all .svelte (15+ files: McpManager, PluginsStore, ConnectorCenter,
    OfficeSettings, BotSettings, ChatRoomView, ComputerPanel, ChannelsPanel, TeamImport, CreateChatRoom,
    ArtifactPanel, ModelPicker, ChatRoomList, AvatarPicker, AgentIntelligence, dialog-overlay…):
    1. `bg-[var(--surface-2)]lack/N` (failed bg-black migration) → `bg-black/N`.
    2. Duplicate `border-[var(--hairline)] border-[var(--hairline)]` → `border-b border-[var(--hairline)]`;
       `border-[var(--hairline)] border-dashed border-[var(--hairline)]` → `border border-dashed …`;
       orphan double-color combos (`border-[var(--hairline)] border-[var(--brand)]/40`) → single bordered.
    3. Stray standalone `]` tokens inside class strings removed. DANGER: the stripper regex
       (` \](?=[ "'])`) also ate real `] as const;` / `] as opt}` closers (4 sites, restored manually).
       Any future bulk pass must exclude script/each-expression lines.
    4. Old purple-theme hex tints (#12101e…#202038 lowercase family, #262638-style borders) → surface/hairline
       tokens. UPPERCASE hexes in ConnectorIcon/PluginLogo/service lists are brand logo colors — keep.
       OfficeSettings role tints (#1c0f10 etc.) kept (intentional role color-coding).
    5. All `text-white` → `text-[var(--text-primary)]`, except on `bg-[var(--brand)]` elements →
       `text-[var(--text-on-light)]` (pastel blue needs dark text).
    6. `py-0.2` (nonexistent utility) → `py-[2px]`.
  - Verified: svelte-check 0 errors, vitest 21/21, build OK after the sweep.
- `git clone --depth 1 https://github.com/nightly-labs/openbot /tmp/openbot` (this session).
  OpenBot is SolidJS despite `.tsx` looking like React.
- Branch: `feature/grok-parity`. Recent UI commits 451e5c1, 0bdc197 already applied OpenBot
  tokens but kept old component structure — rebuild replaces structure, keeps tokens contract.
- Never add `<style>` blocks to `.svelte` (CI fails). All styling via global `components.css` +
  Tailwind utilities + tokens.css vars.
- Do not rename/remove any invoke command or the `agent-stream` event. Rust
  side is FROZEN except P8 additive scope (new commands/tables/columns only —
  no existing command signature or agent-stream payload changes).
- User's framing of the goal: *"real life office with bots"* — the Office/rooms experience is the
  product center, not an afterthought. Chat quality should feel GROK/OpenBot-grade.
