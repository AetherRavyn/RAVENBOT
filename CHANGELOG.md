# Changelog

All notable changes to RAVENBOT are documented here.

## 0.2.0 — GROK-Parity + Production Honesty Pass

The release where RAVENBOT matched GROK's chat experience feature-for-feature, then
kept going with capabilities GROK doesn't have — and where every path that *pretended*
to work was made real.

### The Chat Experience (GROK-quality, local)
- **Token streaming** — live SSE from OpenRouter, Anthropic, OpenAI, and Ollama (Ollama provider fully implemented); typing dots, shimmer skeletons, live tool rows
- **Live citations** — search/browser tool results harvested into numbered, clickable source chips (live during streaming, persisted on answers)
- **Artifacts & canvas** — qualifying code blocks open in a split panel: code, sandboxed HTML preview, rendered Markdown, copy/download
- **Extended thinking** — `[Think]` streams Anthropic's real chain-of-thought token-by-token into a 🧠 Reasoning panel, persisted across reloads
- **Vision** — paste/drop images flow as inline base64 to all provider vision formats; `analyze_image` auto-equips
- **Image generation** — `image_gen` skill (OpenAI DALL-E 3 or keyless Pollinations)
- **Voice mode** — hands-free loop: STT → auto-send → TTS playback → resume listening
- **Ephemeral chats** — 👻 temporary threads that never feed agent memory (migration 007)
- **Message UX** — regenerate, edit-and-resend, per-conversation model quick switcher, actionable error hints, automatic retry on transient failures

### The Moat
- **Offices go live** — parallel agent runs stream per-agent lanes (watch each specialist think), live status rings, office telemetry, error hints; delegation between agents runs the target bot for real (depth-guarded)
- **Real budgets** — usage tracked per model round (migration 008), enforcement before every run, Settings Budgets card, reset/rollover
- **Real memory** — `memory_save`/`memory_recall` are runtime-native vector operations (were stubs)
- **Own MCP server** — `ravenbot mcp-serve`: every skill exposed as an MCP tool over stdio (JSON-RPC 2.0), remote MCP connectors execute via `mcp-remote` stdio bridging
- **Own CLI** — `ravenbot run|list-bots|mcp-serve` headless modes with `RAVENBOT_DB`/`RAVENBOT_KILL_SWITCH`
- **Real browser tool** — `browser_navigate` fetches + extracts readable text (was a stub)
- **Fleet sync with TOFU** — Ed25519-signed bundles with persisted signing key, Trust-On-First-Use registry, per-bot key binding (key swaps rejected); Fleet Sync & Backup UI
- **Connectors hardened** — dynamic MCP tool resolution (was: broken lookup by tool name), 10-min tool-discovery cache (was: re-spawn per message), built-in skills shadow MCP synthesized names, credential precedence DB→OS env
- **Routines made real** — cron executor creates threads and actually runs agents; Routines UI
- **Live status + telemetry** — status rings change during runs; telemetry pill shows real, restart-surviving usage

### Quality
- **45+ tests** across the stack, including a full **E2E happy-path test** (scripted MockProvider through the real runtime + SQLite: stream → tool → reasoning → persistence → audit)
- Runtime integration tests (kill switch, budgets, delegation, ephemeral), streaming accumulators, MCP resolution, sync TOFU (roundtrip/tamper/key-swap), i18n full-coverage × 6 locales, frontend suite (`npm test`)
- 6 fully translated locales; accessible, keyboard-first UI
- Verified performance: 12 MB release binary, 75 ms headless cold start, 68 ms MCP roundtrip (`scripts/bench.sh`)

## 0.1.0 — Foundation

24-phase build-out: workspace + SQLite, multi-bot, skills system, sandbox + kill switch,
DAG orchestration, vector memory + self-review, vision/multimodal, governance (budgets,
audit, prompt version control), cron routines, signed bundles, supply-chain hardening
(cargo-audit/deny, SBOM), a11y + i18n.
