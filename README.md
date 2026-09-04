# RAVENBOT — Sovereign, Local-First, Rust-Native Multi-Agent Desktop OS

> **Not a chatbot — a fleet of persistent, specialized agents that live entirely on your machine.**

Each bot has an identity, a model, a skill set, a memory, a sandboxed computer, and a thread. One orchestrates the rest — exactly like your team. And unlike other AI apps, RAVENBOT both **consumes MCP** (135+ connector catalog) and **is an MCP server** — external agents like Claude Code can drive your entire fleet.

![License: MIT](https://img.shields.io/badge/License-MIT-indigo.svg)
![Rust](https://img.shields.io/badge/Rust-1.77%2B-orange.svg)
![Tauri](https://img.shields.io/badge/Tauri-2.x-blue.svg)
![Svelte](https://img.shields.io/badge/Svelte-5-ff3e00.svg)
![a11y](https://img.shields.io/badge/a11y-WCAG%202.1%20AA-green.svg)
![i18n](https://img.shields.io/badge/i18n-6%20locales-lightgrey.svg)

---

## The Chat Experience (GROK-quality, local)

| Feature | What you get |
|---------|--------------|
| **Token streaming** | Live SSE streaming from OpenRouter, Anthropic, OpenAI, and Ollama — tokens render as they arrive with a blinking cursor, typing dots, and shimmer skeletons while tools run |
| **Live citations** | DeepSearch harvests sources from search/browser tools into numbered, clickable source chips — live while streaming, persisted on the answer |
| **Artifacts & canvas** | Qualifying code blocks (≥8 lines) open in a split canvas panel: code view, sandboxed HTML preview, rendered Markdown, copy + download |
| **Extended thinking** | `[Think]` mode streams Anthropic's real chain-of-thought token-by-token into a collapsible 🧠 Reasoning panel (persisted across reloads) |
| **Vision** | Paste or drop images into the composer — inline base64 flows to OpenAI / Anthropic / Ollama vision formats; `analyze_image` auto-equips |
| **Image generation** | `image_gen` skill: OpenAI DALL-E 3 when keyed, keyless Pollinations fallback |
| **Voice mode** | Hands-free loop: speech-to-text → auto-send → response spoken aloud → listening resumes |
| **Ephemeral chats** | 👻 Temporary threads that never feed agent memory (RAG + self-review skipped) |
| **Message UX** | Regenerate, edit-and-resend (turn rewritten, response rebuilt), per-conversation model switcher (Ollama/OpenRouter/Anthropic/OpenAI + custom id), actionable error hints, automatic retry on transient failures |

## The Moat (what GROK doesn't have)

| Capability | Detail |
|------------|--------|
| **Offices (multi-agent teams)** | Rank + specialty bots with goals, policy, shared office memory — an orchestrator routes your message and runs specialists **in parallel via a DAG** with blackboard sharing and checklist summaries |
| **135+ connector catalog** | Preconfigured MCP servers (GitHub, Jira, Notion, Figma, K8s, AWS/GCP/Azure, Postgres/Mongo, Playwright, …) with per-bot and global assignment, multi-agent batch assign, preset stacks, live connection tests |
| **Own MCP server** | `ravenbot mcp-serve` exposes **every RAVENBOT skill as an MCP tool over stdio** — Claude Code, GROK, or anything MCP-speaking can drive your fleet |
| **Real memory & learning** | Runtime-native `memory_save`/`memory_recall` (vector store), RAG context injection, self-review with memory promotion, skill proficiency tracking |
| **Routines** | Cron-scheduled agent runs (real executor — threads are created and the bot actually runs), enable/disable, run-now, scheduler status |
| **Fleet sync with TOFU** | Ed25519-signed bot bundles (agent + skills + memories); **Trust-On-First-Use** registry with per-bot key binding — tampered or key-swapped bundles are rejected |
| **Sovereign safety** | Sandbox + resource quotas, per-run network policies, global kill switch (also `RAVENBOT_KILL_SWITCH=1` headless), budgets, audit log, prompt version control with diff/rollback |

---

## Own CLI

One binary, dual personality:

```bash
ravenbot                       # the desktop app
ravenbot list-bots             # headless: list agents
ravenbot run --bot RANO --message "Summarize the repo"
ravenbot mcp-serve             # MCP server over stdio
ravenbot --help
```

`RAVENBOT_DB` overrides the database path. Headless tool calls pause when `RAVENBOT_KILL_SWITCH=1`.

### Drive RAVENBOT from any MCP client

```json
{
  "mcpServers": {
    "ravenbot": { "command": "ravenbot", "args": ["mcp-serve"] }
  }
}
```

---

## Quick Start

```bash
npm install
npm run tauri dev      # dev — hot reload, live token streaming
npm run tauri build    # release — 27 MB, LTO, stripped
```

New bots default to **local Ollama (sovereign)** — override with `RAVENBOT_DEFAULT_PROVIDER` / `RAVENBOT_DEFAULT_MODEL`, or switch per-conversation in the header.

| Provider | Key | Local? |
|----------|-----|--------|
| Ollama | none | **yes — zero external calls (default)** |
| OpenRouter | `sk-or-…` | no |
| Anthropic | `sk-ant-…` | no |
| OpenAI | `sk-…` | no |
| Local (candle/llama.cpp) | none | **yes — fully offline** |

---

## Skills (25 built-in)

`web_search` · `tavily_search` · `browser_navigate` (real fetch + readable extraction) · `file_read` / `file_write` / `file_tree` · `code_search` / `code_edit` · `shell_exec` · `git` · `docker` · `db_query` · `http_request` · `memory_save` / `memory_recall` (real vector store) · `todo` · `calendar` · `youtube_transcript` · `arxiv_search` · `screenshot` / `analyze_image` · `image_gen` · `voice_input` / `voice_output` · `delegate` — plus **1,497+ curated community skills** on demand and OpenAPI-imported plugins.

---

## Tests & Quality

- **39 tests** across the stack: runtime integration (kill switch, bad provider, ephemeral, **full E2E happy-path with a scripted MockProvider**), streaming accumulators, MCP resolution (prefix heuristic, cache, per-bot isolation), sync TOFU (roundtrip, tampering, key-swap rejection), scheduler cron, i18n (full key coverage × 6 locales), artifact detection
- `cargo check` / `cargo clippy` — clean on all new code
- `svelte-check` — 0 errors, 0 warnings

---

## Verified Performance

Measured via [`scripts/bench.sh`](scripts/bench.sh) on the release build:

| Metric | Target | Measured |
|--------|--------|----------|
| Release binary | < 40 MB | **12 MB** ✓ (LTO, stripped) |
| Headless cold start (db + migrations + query) | < 300 ms | **75 ms** ✓ |
| MCP roundtrip (initialize + tools/list) | — | **68 ms**, 62 tools exposed ✓ |
| `cargo test` | all pass | **39 passed** ✓ |
| `svelte-check` | 0 errors | **0 errors, 0 warnings** ✓ |
| Tool discovery | once per TTL | **10-min cache** ✓ (no per-message MCP spawns) |

See [`ROADMAP.md`](ROADMAP.md) for the full 21-phase milestone table.

---

## Design

- **Window & shell** — frameless, dark-first, GROK-style prompt bar with DeepSearch/Think toggles
- **Thread view** — live streaming, citation chips, artifact canvas, reasoning panel, attachment thumbnails
- **Command Center** — connectors (135+), API keys, multi-agent assignment, themes, local models
- **Global** — `⌘K` command palette, `⌘,` settings, kill switch UI, Fleet Sync & Backup modal

Design tokens in [`src/lib/styles/tokens.css`](src/lib/styles/tokens.css) — no templated defaults.

## Accessibility & i18n

- Full keyboard navigation, focus trap, visible focus rings
- `aria-label`, `aria-live` announcements, focus management, reduced-motion support
- **6 fully translated locales**: English, Español, Français, Deutsch, 日本語, 中文 — per-locale key coverage enforced by test

---

## Safety

- Per-bot sandbox (OS-level default) + CPU/mem/net quotas
- Per-run network allowlist, confirmation gate on high-risk actions
- Global kill switch — one control pauses all bots and revokes net access
- `audit_log` — every tool call, request, file write, permission decision
- `cargo audit` / `deny` + CycloneDX SBOM per release + signed-update tooling (`scripts/generate-update-keys.sh`)

---

## Development

```bash
make dev          # run dev
make build        # release build
make test         # cargo test + svelte-check
make lint         # clippy + svelte-check -D warnings
make audit        # cargo audit
make deny         # cargo deny check all
make sbom         # CycloneDX
npm test          # frontend test suite (vitest)
```

---

## License

MIT — no account, no required server, no forced telemetry.
