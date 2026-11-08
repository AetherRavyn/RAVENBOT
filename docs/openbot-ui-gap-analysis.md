# OpenBot UI and user-friendliness gap analysis

Reviewed from the `nightly-labs/openbot` source at commit
`d9510240d13e012a02c38c80f9d99cd6aa08ce60` (2026-11-04, OpenBot 0.61.0).
OpenBot is PolyForm Noncommercial, so this is a product comparison; no code was
copied.

This file focuses on **what a user feels**: navigation, search, first run, task
visibility, usage, and command discovery. Architecture differences and
provider-native sessions stay in [openbot-gap-analysis.md](openbot-gap-analysis.md).

## Where RAVENBOT is already strong

| Surface | RAVENBOT today |
|---|---|
| Chat | Streaming, reasoning panel, tool rows, turn trace, context meter, compaction, queued office turns, reactions, replies, in-thread search |
| Offices | AI-designed teams, DAG execution, shared board, shared memory, autonomous runs, office queue |
| Tools | 2,530-app OpenAPI directory, real credentials, MCP catalog, local skill library and marketplace |
| Workspace | Projects, team channels, browser dock with take-over, runs, routines |
| Navigation | Rail + sidebar, pinned/hidden agents, waiting-on-you filter, unread counts, drag reorder |
| Safety | Kill switch, budgets, approval cards, sandbox tiers, audit log |
| Accessibility | Focus handling, reduced motion, six locales, type-scale and component-style gates |

## Implemented in this pass

These were the highest-impact day-to-day gaps.

### Global search (⌘K)

A real search overlay across **agents, offices, messages, settings, and
actions**, not just a bot switcher.

- Tabs for All / Agents / Offices / Messages / Settings.
- Message content search with a 220 ms debounce, loading/error/retry states, and
  match highlighting.
- Selecting a message selects its agent, loads the thread, and reveals the exact
  message. A pending hand-off covers the race where the conversation is still
  mounting.
- Selecting a setting opens that Settings page directly.
- Quick actions for new chat, new agent, offices, projects, connectors,
  routines, plugins, skills, browser, and settings.
- Full keyboard navigation, `Enter` to run, `Esc` to dismiss, and a visible
  sidebar search button so the feature is not shortcut-only.

### Searchable settings navigation

The settings sidebar now has its own search field. Typing “theme”, “budget”,
“providers”, “usage”, “runs”, etc. filters the nav immediately, with a clear
empty state.

### Usage and spend

A new **Usage** page in Settings reads the run ledger, not a client estimate:

- 7-day, 30-day, and all-time windows.
- Total cost, tokens, runs, and success rate.
- Per-agent breakdown with a relative cost bar, failure count, and agent
  identity.
- Loading, empty, error, and retry states.

### Message search context

`search_messages` now returns the owning `bot_id` alongside the thread and
message, so a global result can open the right conversation rather than only the
right thread title.

## Remaining gaps, in priority order

### P0 — Unified tasks inbox

OpenBot has a first-class Tasks page with a cross-agent list, task ownership,
approvals, undoable starts, and task conversations. RAVENBOT has office queues
and routines, but no single place to answer “what is waiting on me across every
office?”

**User pain:** queued work, approvals, and delegated tasks are spread across
offices and threads.

**Next slice:** a `tasks` page that aggregates office queue items, pending
approvals/questions, and routine runs; filters by agent and state; opens the
owning thread in one click.

### P0 — First-run flow that finishes the job

RAVENBOT’s onboarding modal asks for a provider key and an office architect.
OpenBot’s first run is a full sequence: language, provider detection/download,
sign-in, model choice, and a clear transition into the workspace.

**User pain:** a new user can skip the modal and land on Home with no provider,
no architect, and no office, without a visible checklist.

**Next slice:** keep the modal for existing users, but add a Home **setup
checklist** (provider → architect → first office) and a full-screen first-run
path with provider detection and a “create my first office” step.

### P1 — Agent templates

OpenBot ships agent templates and a marketplace for complete teammates.
RAVENBOT’s marketplace is skills-first; creating a new agent still starts from a
blank name and description.

**User pain:** blank-page agent creation is the hardest part for a non-expert.

**Next slice:** a small set of real starter templates (Billing & Ledger,
Research, Support, Release Engineer) that prefill instructions, default tools,
and a first prompt. No fake agents: each template creates a real bot.

### P1 — Sidebar organization

OpenBot groups **Pinned**, **Waiting**, custom sections, and hidden chats, with
drag support across groups. RAVENBOT sorts pinned agents first, offers a
waiting filter, and has a hidden toggle, but the groups are not visually
separated.

**User pain:** with more than a handful of agents, the fleet list is a flat
scroll.

**Next slice:** group headings for Pinned, Waiting on you, and All agents;
collapse state per group; keep the existing drag persistence.

### P1 — Keyboard shortcut help

OpenBot exposes shortcut hints in context. RAVENBOT registers useful shortcuts
(⌘K, ⌘N, ⌘B, ⌘,, ⌘⇧B, Alt+↑/↓) but has no cheat sheet.

**User pain:** users do not discover the fast paths.

**Next slice:** a “Keyboard shortcuts” dialog, reachable from the palette and a
`?` shortcut, grouped by Navigation / Chat / Workspace.

### P2 — Attention inbox

The rail already shows working and attention counts. There is no list of the
approvals, questions, and failed runs behind those badges.

**Next slice:** an inbox panel that lists pending approvals/questions and failed
runs, with a jump to the owning office or thread.

### P2 — Files and projects in global search

OpenBot searches files as well as messages and routines. The new RAVENBOT
search covers messages, settings, agents, and offices, but not project files.

**Next slice:** index project files by name/path (not full content first) and add
a Files tab with open/reveal actions.

### P2 — What’s new and recovery surfaces

OpenBot shows release notes and provider recovery guidance. RAVENBOT has
diagnostics and error toasts, but no “what changed in this build” surface.

**Next slice:** a small What’s New overlay backed by the changelog, and a
recovery card for provider auth failures that links straight to the provider
row.

### P3 — UI quality gates

RAVENBOT already enforces the 11 px type floor, component-style boundaries, and
i18n key parity. OpenBot adds Storybook, visual regression, and design-token
linting.

**Next slice:** visual snapshots for the palette, settings, chat, and office
surfaces; a token lint that rejects raw color literals in components.

## Recommended next three slices

1. **Tasks inbox** — the largest remaining usability gap.
2. **First-run checklist + first office** — converts new installs into working
   fleets.
3. **Pinned/Waiting sidebar groups + shortcut sheet** — makes a larger fleet
   feel organized.

## Verification

- Rust workspace: 501 passed, 0 failed, 7 ignored.
- Frontend: 536 passed across 61 files.
- `svelte-check`: 0 errors, 0 warnings.
- Production build: clean.
