# RAVENBOT Roadmap

## Phase 0 - Scaffolding ✅
- [x] Workspace scaffolding
- [x] Core domain types
- [x] SQLite database + migrations
- [x] Typed IPC contract
- [x] Tauri shell + Svelte frontend

## Phase 1 - MVP ✅
- [x] Single bot with model provider
- [x] Real model calls (OpenRouter, Anthropic, OpenAI)
- [x] Persisted threads
- [x] Message sending
- [x] API key configuration

## Phase 2 - Multi-Bot ✅
- [x] Multi-bot sidebar
- [x] Live status indicators
- [x] Per-bot model selection
- [x] Bot settings modal
- [x] Orchestrator support
- [x] Command palette
- [x] Session cost tracking

## Phase 3 - Skills System ✅
- [x] Skill trait and registry
- [x] Web search skill
- [x] File read/write skills
- [x] Shell execution skill
- [x] Inter-bot delegation
- [x] Skill Manager UI

## Phase 4 - Sandboxed Computer View ✅
- [x] Sandbox isolation system
- [x] Resource quotas (CPU, memory, disk, network)
- [x] Network access policies
- [x] Global kill switch
- [x] Kill switch UI with confirmation

## Phase 5 - Orchestration Graph ✅
- [x] Directed task graph engine (DAG)
- [x] Parallel fan-out/join execution
- [x] Shared scratchpad (blackboard)
- [x] Topological sort for deterministic execution
- [x] Dependency tracking and deadlock detection
- [x] Checklist-style summaries

## Phase 6 - Memory & Self-Review ✅
- [x] Vector embedding system (local)
- [x] Memory storage with similarity search
- [x] Retrieval-augmented generation (RAG)
- [x] Self-review after each run
- [x] Automatic memory promotion
- [x] Skill proficiency tracking

## Phase 7 - Vision & Multimodal ✅
- [x] Screenshot capture system
- [x] Image analysis with vision models
- [x] Computer control (click, type, scroll)
- [x] Audio transcription (local/cloud)
- [x] Text-to-speech output
- [x] Vision skills (screenshot, analyze, voice)

## Phase 8 - Governance ✅
- [x] Budget manager with caps
- [x] Budget checking before model calls
- [x] Audit logger for all actions
- [x] Prompt version control
- [x] Version diff and rollback

## Phase 9 - Routines ✅
- [x] Cron expression parser
- [x] Routine manager (CRUD)
- [x] Event-driven triggers
- [x] Scheduler with tick loop
- [x] Schedule checking and execution

## Phase 10 - Optional Sync ✅
- [x] Bundle export/import (signed)
- [x] Ed25519 signing and verification
- [x] Local network sync server/client
- [x] Pairing flow with codes
- [x] Memory import/export

## Phase 11 - Supply Chain Hardening ✅
- [x] cargo-audit / cargo-deny configuration
- [x] CI/CD security audit workflow
- [x] SBOM generation (CycloneDX)
- [x] Code signing documentation
- [x] Release hardening scripts

## Phase 12 - Polish ✅
- [x] Internationalization system (i18n)
- [x] Accessibility utilities (a11y)
- [x] Keyboard navigation and shortcuts
- [x] Screen reader support
- [x] Focus management
- [x] Reduced motion support
- [x] High contrast mode (macOS, Windows, Linux)

## Phase 13 - GROK-quality Gap: Streaming & Responsiveness ✅
- [x] SSE/token streaming in all model providers (OpenRouter, Anthropic, OpenAI, Ollama)
- [x] Ollama provider fully implemented (real local fallback with tools + streaming)
- [x] Stream relay through Tauri events (`agent-stream` channel)
- [x] Live typing indicator + partial rendering of assistant tokens
- [x] Regenerate response (deduplicated re-run without duplicating user turn)
- [x] Edit-and-resend (backend removes turn onward, inserts edited user turn, fresh run; composer banner + Esc cancel)
- [x] Per-conversation model quick switcher (clickable header pill: Ollama/OpenRouter/Anthropic/OpenAI presets + custom model id)
- [x] Streaming accumulator unit tests
- [x] Skeleton loading states for runs and tool executions

## Phase 14 - GROK-quality Gap: Grounding & Citations 🟨
- [x] Citation harvest pipeline: search/browser tool results → `Source` structs (URL validated, deduped, capped at 10)
- [x] `MessageContent::Text.sources` field — citations persisted on the assistant message (backward-compatible with old DB rows)
- [x] `StreamEvent::Sources` — live citation events during DeepSearch
- [x] Inline numbered source chips under assistant answers (domain labels, clickable via opener)
- [x] Live source chips appear while streaming
- [x] DeepSearch progress visibility (tool_started/tool_finished events shown live)
- [ ] Provider-native grounding (provider citation payloads e.g. OpenAI web_search tool annotations)

## Phase 15 - GROK-quality Gap: Artifacts & Canvas ✅
- [x] Artifact panel component (split canvas docked beside the chat, expand/dock toggle)
- [x] Artifact detection: qualifying code blocks (≥8 lines or ≥500 chars) get an "Artifact" button
- [x] Code / Preview tabs: plain source view, sandboxed HTML iframe preview, rendered Markdown preview
- [x] Copy + download artifacts (language-aware extension)
- [x] Reasoning transparency:` renders as a collapsible 🧠 Reasoning panel
- [x] **Wire provider extended thinking into the reasoning panel (Anthropic `thinking` param)**:
  - `complete_stream(enable_reasoning)` plumbed through the trait + all providers (ignored where unsupported)
  - Anthropic: `thinking` config (budget = 60% of max_tokens, temperature forced to 1 per API constraint)
  - `thinking_delta` streams live inside ` swell → 🧠 Reasoning panel renders the chain-of-thought in real time during `[Think]` mode
  - Reasoning kept separate from content in `StreamAccumulator` (returned as `ModelResponse.reasoning`, unit-tested), persisted as a ` swell prefix so it survives reloads

## Phase 16 - GROK-quality Gap: Multimodal Polish ✅
- [x] Image generation skill (`image_gen`: OpenAI DALL-E when keyed, Pollinations.ai keyless fallback, URL probe)
- [x] Intent-driven auto-equip of image_gen ("image/draw/picture/photo")
- [x] Inline image display in ThreadView (CSP img-src widened, rounded image styling)
- [x] Hands-free voice mode loop (STT → auto-send → TTS playback → resume listening)
- [x] Speech output strips code fences/links/reasoning before TTS
- [x] Temporary/ephemeral threads: `Thread.ephemeral` + migration 007, runtime skips RAG context + self-review memory promotion, Temporary badge + composer toggle
- [x] Provider-backed vision: paste/drop images into the composer — inline base64 plumbing through Core → Runtime → all providers (OpenAI `image_url` parts, Anthropic `image` source blocks, Ollama native `images`), attachment chips + thumbnails in bubbles, `analyze_image` auto-equipped
- [x] Paste/drop non-image files (txt/md already supported via paperclip)

## Phase 17 - Wire Half-Built Features into IPC/UI 🟨
- [x] Scheduler made real: `RoutineExecutor` callback — due routines create a thread and actually run the bot (was a log-only stub)
- [x] Routines wired into IPC: create/get/list/update/delete + `get_scheduler_status` + `run_routine_now` + `routine-executed` event
- [x] Routines UI: RoutinesPanel (cron presets, enable/disable, run now, delete, scheduler status pill) + header 🕐 button + modal
- [x] Sync wired into IPC: `export_bot_bundle` (signed, memory optional) / `import_bot_bundle` / `import_bot_bundle_from_file`
- [x] Updater plugin wired: `tauri-plugin-updater` registered + `updater:default` capability
- [x] Update-key story: `scripts/generate-update-keys.sh` (ed25519 keypair, pubkey into conf, CI secrets documented)
- [x] CI: migrated deprecated `upload-artifact@v3` → `@v4` and `cache@v3` → `@v4`
- [x] Sync UI panel: Fleet Sync & Backup modal (🔄 per-bot **Export** → signed JSON download, **Import** → paste → verify) + 📦 header button + `onBotImported` sidebar refresh
- [x] Updater UI (check-for-updates button once pubkey/endpoints configured) — **blocked on operator secrets** (key script + CI wiring done)
- [x] **Real Ed25519 signing**: legacy placeholder hash-signing replaced — exports sign the serialized bot with Ed25519 and embed the signer's pubkey; imports verify with `verify_with` (arbitrary pubkey)
- [x] **Persisted signing key**: `bundle_signing_key` table (seed stored on first use) — signatures are stable across restarts, required for TOFU
- [x] **TOFU trust registry**: `trusted_bundle_pubkeys` (global) + `bot_trusted_fingerprint` (per-bot key binding)
  - First import from a new signer: verified → trusted on first use
  - Later imports: must carry the same per-bot key — **key swaps and impostor re-signing are rejected**
  - Tampered signatures rejected outright; unsigned bundles import with warning

## Phase 18 - Engineering Quality ✅
- [x] Integration tests for the runtime execution path (4 tokio tests: kill switch blocks run, bot-not-found, unknown provider → model error, ephemeral path runs clean)
- [x] Frontend test suite: vitest + 17 tests (artifact detection/thresholds + i18n coverage across all 6 locales) + `npm test` + CI frontend job
- [x] i18n: all 6 locale files complete (en/es/fr/de/ja/zh), full key coverage asserted per locale
- [x] Kill hardcoded default model: new bots default to **local Ollama** (sovereign), overridable via `RAVENBOT_DEFAULT_PROVIDER`/`RAVENBOT_DEFAULT_MODEL`
- [x] Model plumbing fix: `BotConfig.model_id` was silently ignored — now applied via `create_provider_from_str_with_model` + trait-level `with_model`
- [x] Error surfaces: actionable `error_hint()` per failure class (auth/rate-limit/network/config) on send + regenerate
- [x] Provider-failure fallback: single automatic retry on transient errors (network/5xx/rate-limit), kill-switch aware
- [x] Skeleton loading states for runs and tool executions (Phase 13 leftover)

## Phase 19 - Own CLI, Tools & MCP ✅
- [x] Own MCP **server** (`crates/mcp/src/server.rs`): full JSON-RPC 2.0 MCP protocol over stdio — `initialize`, `tools/list`, `tools/call` — exposes every RAVENBOT skill as an MCP tool so **external** agents (Claude Code, GROK, anything MCP-speaking) can drive RAVENBOT's fleet (smoke-tested live)
- [x] Headless kill switch for remote drives: `RAVENBOT_KILL_SWITCH=1` pauses all MCP tool calls
- [x] Own **CLI** (`ravenbot` binary): `mcp-serve` (stdio MCP server), `run --bot <name|id> --message "..."` (headless single run → prints response), `list-bots`, `--help`; `RAVENBOT_DB` overrides the database path
- [x] Own **browser tool**: `browser_navigate` now does real navigation — fetch + readable text extraction (title + tag-stripped content), URL validation, unit-tested (was a "would navigate" stub)
- [x] Honest stub notes on webview-runtime actions (DOM click/fill need wry webview; screenshot/computer control are real)

## Phase 20 - Connectors & Tools Command Center — Hardening ✅
- [x] **Critical fix — dynamic MCP tool calls**: the runtime resolved MCP tool calls by looking up a *server config by tool name* (almost never matches) — added `McpRegistry::resolve_tool` (tool-cache lookup across servers + `server_id_` prefix heuristic) so every dynamically-called tool reaches the right connector
- [x] **Performance fix — tool discovery once per TTL**: MCP servers (npx spawns, 5s timeouts) were re-discovered on *every message* — discovered tools now cache per server for 10 min (`TOOLS_CACHE_TTL_SECS`), making runs fast after the first message
- [x] **Shadowing fix — built-ins win**: MCP *synthesized* tool lists (e.g. `browserbase` → `browser_navigate`) were assembled ahead of real built-ins and replaced them with fabricated fallbacks — built-in skills now replace same-named MCP tools after assembly
- [x] Credentials precedence: per-connector env (DB, saved via the Command Center) → OS env fallback for any configured keys still missing
- [x] Per-bot MCP cap raised 15 → 24 (runtime's global 32-cap still applies); FK-verified assignments (connectors can only attach to real bots)
- [x] 4 registry resolution tests (prefix heuristic, unknown → None, cache-stage resolution, per-bot assignment isolation) + UI↔IPC audit of all 11 commands in both Command Center and MCP Manager (all aligned, camelCase→snake_case verified)

## Phase 21 - End-to-End Happy-Path Proof ✅
- [x] **MockProvider E2E test** (`crates/runtime`): injectable `Runtime::set_provider_override` + scripted 2-round provider (reasoning stream → tool call → final answer) driven through the **real** Runtime + temp SQLite
- [x] Asserts the entire product spine in one test: `[Think]` → `enable_reasoning` · reasoning deltas stream inside ` swell` · tool round executes · tool result fed back · `ToolStarted/Finished` events · final assistant message persisted with ` swell` reasoning prefix · run Completed · **memory actually persisted** · tool call **audited**
- [x] **Bug found by the test and fixed**: `memory_save`/`memory_recall` were stubs (fabricated success, never touched memory) — now runtime-native: `exec_memory_save` persists real vector facts, `exec_memory_recall` does semantic similarity search over the bot's memory (RAG-visible)

## Phase 22 - Production Honesty Pass — Nothing Lies to the User ✅
*Every path below was found pretending to work. Each item: current fake → production behavior → test.*

### 22.1 Budget enforcement (safety-critical) ⬜
- [x] `execute_run`: `check_budget(bot)` **before** the run → refuse with actionable error when exceeded; `record_usage` **after** with real tokens/cost (was: stored but never enforced, bots could spend unbounded)
- [x] Test: zero-budget bot refused; usage recorded on success

### 22.2 Live status, real telemetry, working pause ⬜
- [x] `StreamEvent::Status { bot_id, state }` emitted across the run lifecycle (thinking → running_tool → done); Sidebar drives live status rings from it (was: static "Ready" forever)
- [x] `StreamEvent::Usage { thread_id, tokens, cost }` at run end → ThreadView telemetry pill shows real session tokens/$ (was: permanently $0.0000)
- [x] Header Pause/Play wired: invoke pause_all/resume_all + reflect paused state (was: dead button)

### 22.3 Real delegation (multi-agent) ⬜
- [x] Runtime-native `delegate`: resolve target bot (id/name) → create thread → run instruction through the **real** runtime → return the target bot's answer + thread link (was: fabricated "delegation_initiated" placeholder — target bot never ran)
- [x] Delegation depth guard (max 3) to prevent recursive loops; delegation audited
- [x] Test: delegate between two bots returns the target's actual response

### 22.4 MCP over remote (SSE) — connectors actually execute ⬜
- [x] Remove the fake `is_sse` rejection: `npx mcp-remote <url>` **is a stdio process** (it bridges to the remote server itself) — route every server through the real stdio JSON-RPC client (was: all https-based connectors returned fabricated "synthesized fallback" success)
- [ ] Live smoke test against a remote connector

### 22.5 Thread management ⬜
- [x] `rename_thread` / `delete_thread` IPC (FK cascade wipes messages) + thread-dropdown UI (was: threads could never be renamed or removed)

### 22.6 Search + honest catalogs ⬜
- [x] `search_messages(query)` IPC + cross-thread search UI in the thread drawer (was: no way to find old conversations)
- [x] Awesome skills return honest "not installed — fetch via awesome_fetch" failure instead of canned fake success
- [x] Local (candle/llama.cpp) provider: explicitly documented as Ollama-superseded in Settings copy

## Phase 23 - Offices Go Live + Real Budgets ✅
- [x] **Offices stream live**: `send_to_chatroom` installs the stream emitter around graph execution — parallel agents emit Status/Tool/Usage/Sources events; ChatRoomView listens: per-agent live status dots in the Roles bar (thinking…/tool…), office telemetry pill (real session tokens + cost), error hint parity (was: silent spinners during team runs)
- [x] **Budgets made real end-to-end**:
  - `record_usage` was a **no-op** and `check_budget` hardcoded `used = 0` — budgets could only trip at max=0
  - Migration 008 `budget_usage` table; `record_usage` accumulates per model round (not just the final round — test caught that), `check_budget` reads real usage, `reset_usage` for rollover
  - IPC: `get_bot_budget` (budget + usage + % + allowed) / `set_bot_budget` (tokens/cost × period) / `reset_bot_budget`
  - Settings → Local tab: **Budgets card** — per-agent selector, usage bar (warn at 80%, blocked state), limit editor, reset
  - Test: scripted 60-token run → `get_usage == 60` → 59-token budget exhausted → refused → reset works
- [x] Honesty copy: Local (candle/llama.cpp) marked experimental/unlinked in Settings; Ollama documented as the supported local path

## Phase 24 - Live Agent Lanes + Persistent Telemetry ✅
- [x] **Per-agent live lanes in offices**: `StreamEvent::Delta`/`Clear` now carry `bot_id` — ChatRoomView renders a **live lane per specialist** during a team run: avatar + rank + status + streaming tokens with cursor (dots before the first token), cleared between tool rounds and on completion. Watch the whole team think, each in their own row
- [x] **Telemetry that survives restarts**: `get_session_usage(bot_id)` sums `runs.tokens_consumed`/`cost_estimate` over the bot's threads — ThreadView loads the lifetime baseline on bot switch; live `Usage` events keep it current afterwards (was: session pill reset to $0 on every restart)
- [x] Runtime tests still green with the extended event payloads

## Phase 25 - Grok-Bot Parity: Live Agent Broker, Native Tool Loop & Agent Engines ✅

### 25.1 Permission broker made real ⬜
- [x] Registered the missing `list_pending_approvals` / `decide_approval` IPC — the DB queries, runtime parking, and UI cards existed but no Tauri command could write the decision, so every high-risk tool hung for 10 min then timed out
- [x] `ask_user` human-in-the-loop question tool end to end: `QuestionRequest` domain type, migration 013 `questions`, `QuestionQueries`, runtime parking + answer polling, `question_asked`/`question_answered` stream events, IPC `list_pending_questions`/`answer_question`, and an inline answer card (option buttons + free text) in ThreadView
- [x] Cooperative cancellation: `request_cancel` flag observed at the run's step boundaries; `cancel_run` IPC; engine processes cancelled immediately

### 25.2 Native agent loop ⬜
- [x] Native tool-message round-trip: `models::Message` now carries `tool_calls` + `tool_call_id`; OpenAI/OpenRouter/openai_compat/MiMo/CommandCode emit native `tool_calls` and `tool` results, Anthropic emits `tool_use`/`tool_result` blocks (was: results flattened into fake `user` text, ids lost)
- [x] Configurable model↔tool rounds (`BotConfig.max_tool_rounds` / `RAVENBOT_MAX_TOOL_ROUNDS`, default 12, was hardcoded 5)
- [x] Parallel tool execution for allowed calls (`futures::join_all`), results fed back in call order
- [x] Ollama tool calls fixed — the default sovereign provider previously dropped them entirely
- [x] Tests: native round-trip asserted in the E2E test, plus parallel-tools, cancellation, and `ask_user` park/resume

### 25.3 Agent engines — run on the CLIs users already have ⬜
- [x] New `ravenbot-engines` crate: an `AgentEngine` SPI that spawns a locally-installed agent CLI and normalizes its native protocol into one event stream (`EngineEvent`), with `CancelToken`, `kill_on_drop`, stderr capture, and honest auth/timeout errors
- [x] **Claude Code driver**: real `stream-json` parsing (init/stream_event/assistant/user/result), session-id capture + `--resume`, effort levels, fail-closed `--permission-prompts none` for Ask/Auto, tokenization/cost; verified against a fake CLI in tests and detected live (2.1.263)
- [x] **Codex driver**: tolerant `item.*`/`turn.completed` parser, approval-mode sandbox flags, detection
- [x] **Generic ACP driver**: covers any Agent Client Protocol CLI (JSON-RPC stdio: initialize / session/new / session/prompt, permission requests); configure via `RAVENBOT_ACP_ENGINES`
- [x] Runtime engine path: per-bot `config.engine`, engine turns stream over the same `StreamEvent` channel, persist the assistant message, record budget usage, honour kill switch/cancel; `<think>` reasoning blocks render in the existing Reasoning panel
- [x] IPC `list_engines` / `set_bot_engine`; BotSettings execution-engine picker (Native / Claude Code / Codex / ACP, dimmed when not installed); chat header shows the active engine
- [x] 17 engine tests incl. a deterministic fake-CLI stream test, plus a full runtime↔engine integration test

## Phase 26 - Resilience: Fallback, Compaction, Resumable Runs ✅

### 26.1 Provider fallback ⬜
- [x] `Runtime::provider_chain` builds primary + configured `fallback_provider`/`fallback_model`; `call_model` tries each in order with one transient-retry per provider and returns the winning index so later rounds stay on the working provider
- [x] BotSettings "Fallback Provider" selector + persisted config
- [x] Test: failing primary transparently falls back to the secondary

### 26.2 Context compaction ⬜
- [x] Token estimator (chars/4) + per-model context window (Gemini 1M, Claude 200k, GPT/o 128k, Llama/Qwen 32k; overridable via `RAVENBOT_CONTEXT_TOKENS`)
- [x] Deterministic sliding-window compaction before every model call: keeps the system prompt + newest messages that fit, replaces the dropped middle with an explicit "compacted" marker, and never starts the kept tail on an orphaned tool result
- [x] Test: over-budget history is compacted while preserving system + latest turn

### 26.3 Resumable pause/checkpoint ⬜
- [x] The tool loop checkpoints the full in-flight message state (native tool results live only here, not in the transcript) after every round
- [x] `request_pause` observed at the round boundary: parks the run as `Paused` with its checkpoint instead of failing; `pause_run` no longer clobbers the checkpoint
- [x] `execute_run` restores a paused run's checkpoint (messages + rounds used) and continues; IPC `pause_run` / `resume_run`; UI Pause/Play targets the active run (from `run_started`) and resumes it, falling back to the global kill switch when no run is tracked
- [x] `BotConfig.max_tool_rounds` + BotSettings "Max Tool Rounds" control
- [x] Test: a run parks on pause with a checkpoint, then resumes to completion

## Phase 27 - Real Sandbox Isolation ✅

The `ravenbot-sandbox` crate was a policy object the runtime never used (`Sandbox::start` just flipped an enum; quotas/network were computed, never enforced). This phase makes isolation real and actually applies it.

- [x] `SandboxRunner` (`crates/sandbox/src/runner.rs`): turns a `SandboxConfig` into an actual isolated `tokio::process::Command`
  - **Filesystem**: bubblewrap mounts the system read-only, hides `$HOME` (so `~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.config/gh`, `~/.netrc` are simply absent), never binds `/etc/shadow`/sudoers, and binds only the configured allowed paths + working directory read-write
  - **Toolchain caches** (`.cargo`, `.rustup`, `.npm`, `.cache`, `.local/share/{pnpm,uv}`, …) are bound rw so `cargo`/`npm`/`pip` keep working without exposing credentials
  - **Namespaces**: `--unshare-pid/uts/ipc`, `--unshare-net` when the network policy is blocked, `--die-with-parent`
  - **Resource limits**: a POSIX `ulimit` shim (CPU time, address space, file size, process count) inherited by the whole process tree
  - **Honest reporting**: `SandboxReport` names the backend actually used and whether filesystem/network isolation was applied; `Host` tier and missing `bwrap` degrade to limits-only and say so
- [x] Per-bot tier flows through `SkillContext.sandbox_tier` (from `BotConfig.sandbox_tier`, `RAVENBOT_SANDBOX_TIER` for headless)
- [x] `shell_exec` now runs every command through the runner and reports the sandbox in its result
- [x] IPC `get_sandbox_report`; BotSettings "Command Isolation" tier picker with the live backend report
- [x] Tests (all passing against real bubblewrap on this host): command runs, secrets hidden, network unreachable when blocked, vmem ulimit applied, toolchain caches reachable; plus shell_exec skill tests

Verification on this machine: `bwrap 0.12.0` detected; `~/.ssh`-style paths hidden, external DNS blocked under `NetworkPolicy::blocked()`, `ulimit -v` capped, `cargo` reachable inside the sandbox.

## Phase 28 - Real Computer Use ✅

The desktop-control functions (`perform_click`/`type_text`/`key_press`/`scroll`/`move_mouse`) previously just logged and returned `success: true` without touching the machine. This phase makes them real and gives the bot a live screen.

### 28.1 Real input injection ⬜
- [x] `ravenbot_vision::input::InputInjector` — runtime backend detection: `xdotool` (X11/XWayland: click/move/scroll/type/key), `ydotool` (Wayland pointer), `wtype` (Wayland typing/keys), chosen per capability
- [x] Command **planning is separated from execution**, so tests assert exact argv without moving the user's real cursor; `RAVENBOT_INPUT_DRY_RUN=1` makes execution a logged no-op
- [x] Honest failure: with no backend the plan is `None` and the caller gets a descriptive error, never a fake success; `backend_summary()` reports which tools are live
- [x] `ComputerController` now performs real actions and returns a post-action screenshot (capture failure never fails the action)
- [x] Detected on this Wayland host: `pointer=ydotool typing=wtype keys=wtype`

### 28.2 Computer-control skill ⬜
- [x] `computer_control` skill exposes click / double_click / right_click / move / type / key / scroll / screenshot to the model, with a JSON schema and screen-pixel coordinates
- [x] High risk → every action is gated by the approval broker (Allow/Deny) unless the bot is in Full mode; `ask_user`/`screenshot` stay ungated appropriately
- [x] Auto-equipped when the request mentions desktop/click/mouse/keyboard/GUI/computer
- [x] Tests: unknown action, missing text, real screenshot frame

### 28.3 Desktop panel ⬜
- [x] IPC `capture_screen` (returns a real PNG data-URL frame) and `get_input_backend` (availability + tool summary)
- [x] `ComputerPanel.svelte`: live screen preview (polls every 1.5s, pausable), manual capture, backend status banner, and honest "no backend" guidance
- [x] Header Monitor button in ThreadView opens the panel

## Phase 29 - Teams, Channels, Webhooks & Marketplace ✅

### 29.1 Install a team from one Markdown file ⬜
- [x] `ravenbot_core::team::TeamPackage` — parses YAML frontmatter (bots with title/rank/specialty/prompt/model, optional office, optional routines) plus the human Markdown playbook; also accepts bare YAML/JSON. Validates: at least one bot, named bots, routines reference real bots
- [x] `import_team_into(pool, md)` core (testable): creates bots (rank/title/prompt/model applied), an optional office with every bot as a member, and routines **created disabled** so an import never runs work unattended
- [x] IPC `preview_team` / `fetch_and_preview_team` (URL) / `import_team`; `TeamImport.svelte` review screen (paste or URL) — nothing is created until Import, and the summary lists bots/office/paused routines
- [x] Marketplace install = import a team file from a public URL (the BotMRR flow), on top of the existing 135-connector catalog
- [x] Tests: parser (valid/missing-frontmatter/no-bots/unknown-bot/routine/summary) + import integration asserting paused routines

### 29.2 Channels — named working contexts ⬜
- [x] `Channel` domain type + migration 014 (`channels`, `channel_bots`, `threads.channel_id`); `ChannelQueries` (CRUD, roster)
- [x] Threads can be filed under a channel; the runtime injects the channel's **shared instructions** and **working folder** into the system prompt for those threads
- [x] IPC list/create/update/delete/set_channel_bots; `ChannelsPanel.svelte` (CRUD, instructions, folder, bot roster); ThreadView header channel picker for new threads
- [x] `create_thread` accepts `channel_id`; Thread + ThreadRow carry it
- [x] Tests: channel round-trip (instructions, folder, roster)

### 29.3 Webhook triggers ⬜
- [x] `ravenbot_scheduler::WebhookServer` — a deliberately minimal HTTP/1.1 receiver on **loopback only**: `GET /health` plus `POST /hooks/<secret>`; secret accepted from the path or `Authorization: Bearer`; body never parsed or trusted; responds 202 and runs the routine through the same executor cron uses
- [x] Migration 014 adds `routines.webhook_secret` + `webhook_enabled`; `WebhookQueries` (set/rotate, lookup-by-secret, enabled-only)
- [x] IPC `enable_routine_webhook` (returns URL+secret once) / `disable_routine_webhook` / `get_routine_webhook_status`; server started in app setup (best-effort; port conflict degrades to disabled, app still runs)
- [x] RoutinesPanel: per-routine webhook toggle + one-time URL reveal with copy
- [x] Tests: webhook server (health open, unknown secret 404, wrong method 405, missing secret 401, valid path + bearer 202 and executor ran) + secret-lookup enabled-only

## Phase 30 - Real Office Orchestration + DAG Fixes ✅

Offices previously routed by keyword substring matching or broadcast the same
task to every member, and the graph executor had two correctness bugs.

### 30.1 DAG correctness ⬜
- [x] **Deadlock bug**: `has_deadlock` required a *running* node, so a graph with permanently-blocked (e.g. cyclic) pending nodes reported `false` and the executor spun forever. Now `!complete && ready.is_empty && running.is_empty`, and the error reports the state counts
- [x] **Failed-dependency bug**: a node whose dependency failed/skipped stayed `Pending` forever, stalling the graph. Added `propagate_skips()` (cascades `Skipped` downstream), called each executor tick
- [x] **DAG data flow**: dependent nodes now receive their dependencies' actual outputs via `input_for()` instead of only a value captured at creation time
- [x] Tests: failed-dependency skip cascade, true-cycle deadlock detection, running-is-not-deadlock

### 30.2 LLM orchestrator ⬜
- [x] `ravenbot_runtime::orchestrator`: `plan_prompt` / `parse_plan` (tolerant of code fences + prose, balanced-brace extraction, capped at 6 tasks, drops out-of-range/forward dependencies) / `synthesis_prompt` / `resolve_member`
- [x] `Runtime::plan_office` asks a **lead bot** (explicit orchestrator flag → "lead"/"chief"/"manager" rank → first member) for a JSON task DAG; `Runtime::synthesize_office` has the lead write the final integrated answer, with a concatenation fallback
- [x] `send_to_chatroom` now: plans → builds a graph with real dependency edges → streams the run live → synthesizes via the lead → persists one coherent answer. Falls back to the previous member fan-out when planning fails, so an office always does *something*
- [x] Tests: plan parsing, plan→runtime parse, synthesis via the lead provider, graceful fallback on unparseable output

## Phase 31 - Honest Core Tools: DB Path, Read-Only Queries, Real Browser Input ✅

### 31.1 Headless processes opened the wrong database ⬜
- [x] `ravenbot_mcp::server::default_db_path()` resolved to `~/.local/share/ravenbot/ravenbot.db`, but the desktop app uses Tauri's `app_data_dir()` = `~/.local/share/com.ravenbot.desktop/ravenbot.db`. So `ravenbot run` / `list-bots` / `mcp-serve` operated on a **different, empty** database (verified: the CLI saw 0 bots while the app had 2)
- [x] New `ravenbot_core::paths` — single source of truth (`APP_IDENTIFIER`, `data_dir`, `app_data_dir`, `default_db_path`), reproducing Tauri's layout on macOS/Windows/Linux; the MCP server and tools now delegate to it
- [x] Verified live: `./target/debug/ravenbot list-bots` now lists the app's real bots; the old wrong-path directory is never created
- [x] Test: resolved path is under the app identifier and the `RAVENBOT_DB` override wins

### 31.2 `db_query` was injectable and returned prose ⬜
- [x] The old skill shelled out to `sqlite3` with hand-rolled, broken quoting (mangled legitimate queries) and a prefix-only guard that let `SELECT 1; DROP TABLE bots` execute the drop
- [x] Rewritten on **sqlx with a read-only connection**: a single statement per call (sqlx rejects chaining), SQLite itself refuses writes, the `path` argument is honored, and rows come back as **structured JSON** with column names, a row count, and a truncation flag
- [x] Tests: structured select, write statements rejected, the `SELECT 1; DROP TABLE` chain leaves the table intact, prefix classifier is conservative

### 31.3 Browser click/fill/scroll were fabricated successes ⬜
- [x] Coordinate clicks, typing, and scrolling now use the real desktop input backend (`xdotool`/`ydotool`/`wtype`); `wait` is implemented
- [x] Selector-only clicks/fills need a live browser DOM we don't own, so they now **fail honestly** with guidance instead of returning `success: true`
- [x] Tests: selector click refused, coordinate click reports the real backend (or its absence)

## Phase 32 - Fix: Dev Server Crash on Svelte `<style>` Blocks ✅

Symptom: `[vite] Internal server error: Invalid declaration: \`invoke\`` from
`@tailwindcss/vite:generate:serve`, on `ThreadView.svelte?svelte&type=style&lang.css`.

Root cause (traced in `@tailwindcss/vite/dist/index.mjs`):
- Tailwind's `enforce:"pre"` transform filter `include` list contains `/&lang\.css/`, so it matches Svelte's compiled-style virtual module ids.
- `vite-plugin-svelte`'s `load` only returns that module's CSS **after** the component has been compiled and its CSS cached. When the style module is requested first (fresh server/HMR ordering), `load` returns nothing, Vite falls back to reading the raw `.svelte` file, and Tailwind parses the whole component — `<script>` included — as CSS, erroring on the first JS token (`invoke`).

Fix (deterministic, no dependency patching):
- Moved every component `<style>` block into one global stylesheet, `src/lib/styles/components.css` (imported by `app.css`): `.shimmer`/keyframes, `.no-scrollbar`, the full `.markdown-content` + `.think-*` set, and `.sr-only`/`.focus-trap-sr`. With no `<style>` blocks, Svelte emits no `&lang.css` modules and Tailwind never sees one.
- Added `src/lib/styles/no-component-styles.test.ts` — a vitest guard that fails if any `.svelte` file reintroduces a `<style>` block.
- Verified: fresh `vite dev` serves the page and components with **no** errors, Tailwind utilities are still generated, `svelte-check` clean, `npm test` 18/18, production build succeeds.

## Phase 33 - Fix: Slow First Reply + Vanishing User Message + Flaky Test ✅

### 33.1 A simple "hi" took ~50 seconds ⬜
- [x] Cause: `McpRegistry::skills_for_bot` ran **synchronously** before the model call, spawning every globally-enabled MCP server (`npx -y …` / `uvx …`) with a 25s init timeout. With 8 enabled servers that is two waves of cold starts (measured: user message at 10:55:50, reply at 10:56:43 = **53s**)
- [x] Fix: build the per-bot tool list from the fresh cache when present, otherwise use the **instant synthesized descriptors** and warm the real discovery in a **detached background task** (deduped by an in-flight set). The run never blocks on server spawn; the real tools land in the 10-min cache for the next turn
- [x] Measured: `ravenbot run --message "hi"` dropped from ~53s to **3.8s**

### 33.2 The user's message disappeared after the answer ⬜
- [x] Cause: `selectedBot` is `$derived(bots.find(…))`, and every `status`/`usage` stream event calls `onBotUpdated`, which replaces that object. ThreadView's thread-loading `$effect` depended on the whole `bot`, so it re-ran mid-run, cleared `messages`, set `selectedThreadId = null`, and reloaded `threads[0]` — dropping the active thread (and its user message) once the run finished
- [x] Fix: the effect is now guarded by `loadedBotId` and only reloads when the **bot id actually changes**; status/telemetry updates no longer touch the transcript
- [x] Persistence itself was already correct (verified: user + assistant rows both present, correct order) — this was purely a UI reset

### 33.3 Flaky engine test (`Text file busy`) ⬜
- [x] `parses_fake_stream_json_into_events_and_outcome` failed ~15% of the time under parallel load: `failed to spawn … Text file busy (os error 26)` — ETXTBSY, a fork/exec race on a just-written script
- [x] Fix (production, not just test): `process::spawn_with_stdin` now retries briefly on ETXTBSY, which also protects real users who point `RAVENBOT_CLAUDE_CMD`/`_CODEX_CMD` at a generated wrapper script
- [x] Stress-verified: 0 failures in 25 runs at `--test-threads=16` (was 3/20)

## Performance Targets

Measured via `scripts/bench.sh` (release build):

| Metric | Target | Measured |
|--------|--------|----------|
| Cold start | < 300ms | **75 ms** (headless CLI, incl. db + migrations) ✓ |
| Idle RAM | < 150MB | measure on desktop (`/usr/bin/time -v`) — GUI-only |
| Binary size | < 40MB | **12 MB** ✓ (LTO, stripped) |
| New bot to first token | < 1s | streaming-first-token path (GUI-only measurement) |
| Stream time-to-first-token | < 500ms (Phase 13) | streaming implemented; measure on desktop |
| Checkpoint write | < 5ms | SQLite WAL, local |
| MCP init+tools/list | — | **68 ms, 62 tools** ✓ |
