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
