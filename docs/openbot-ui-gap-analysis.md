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
| Accessibility | Focus handling, reduced motion, twelve locales, type-scale and component-style gates |

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

### P0 — Unified tasks inbox — shipped

OpenBot has a first-class Tasks page with a cross-agent list, task ownership,
approvals, undoable starts, and task conversations. RAVENBOT now has a **Tasks**
rail destination backed by the durable `agent_tasks` ledger: target and owner
identity, filtered states (active/failed/finished), the result or error, Open to
the task thread, cooperative Cancel, a failed-task badge, and live refresh from
`agent-task-changed`. Approvals, questions, office queues and budget alerts stay
in the separate **Inbox** surface because they need a decision; tasks are work
an agent already owns.

**User pain solved:** queued and delegated work no longer exists only as an
event the owner agent can wait for; a person can see it across every agent.

**Next slice:** undoable starts and task templates. The current view is
read/stop, not a planner.

### P0 — First-run flow that finishes the job — checklist shipped

RAVENBOT’s onboarding modal asks for a provider key and The Raven, the default agent.
OpenBot’s first run is a full sequence: language, provider detection/download,
sign-in, model choice, and a clear transition into the workspace.

**User pain solved:** a dismissible modal no longer hides missing setup behind a
clean Home screen. Home now shows a **setup checklist** (provider → architect →
first office) until all three exist. It detects a cloud key or a local Ollama
model, opens provider settings or the existing onboarding for The Raven, and
opens the real office creator for the first office. Completed rows are marked;
only the next incomplete row has an action; the card reloads after Settings or a
bot change and disappears when the setup is complete.

**Still next:** the full-screen first-run sequence with language choice,
provider download/detection, and a single transition into the first office. The
checklist is the persistent safety net, not a replacement for that flow.

### P1 — Agent templates

OpenBot ships agent templates and a marketplace for complete teammates.
RAVENBOT’s marketplace is skills-first; creating a new agent still starts from a
blank name and description.

**User pain:** blank-page agent creation is the hardest part for a non-expert.

**Next slice:** a small set of real starter templates (Billing & Ledger,
Research, Support, Release Engineer) that prefill instructions, default tools,
and a first prompt. No fake agents: each template creates a real bot.

### P1 — Sidebar organization — shipped

OpenBot groups **Pinned**, **Waiting**, custom sections, and hidden chats, with
drag support across groups. RAVENBOT sorted pinned agents first, offered a
waiting filter, and had a hidden toggle, but the groups were not visually
separated.

**User pain solved:** with more than a handful of agents, the list is no longer
a flat scroll. Pinned, Waiting on you, Hidden and Agents are separate headings
with counts; each group collapses and remembers its state; a search temporarily
expands collapsed groups so a match is never hidden. Pinned and waiting rows are
disjoint, and existing drag persistence still writes the visible order.

### P1 — Keyboard shortcut help — shipped

OpenBot exposes shortcut hints in context. RAVENBOT registered useful shortcuts
(⌘K, ⌘N, ⌘B, ⌘,, ⌘⇧B, Alt+↑/↓) but had no cheat sheet.

**User pain solved:** the palette now offers **Keyboard shortcuts**, and
`Shift+?` opens the same sheet outside a text field. It is grouped by
Navigation / Chat / Workspace and lists only implemented bindings, with the
platform modifier glyph.

### P2 — Attention inbox — shipped

The rail shows working and attention counts, and the **Inbox** is the list of
pending approvals, questions, office queue rows and budget alerts behind those
badges, with a jump to the owning office or thread.

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

1. **Agent templates** — starts a new teammate from a role, not a blank name.
2. **Files and projects in global search** — makes the workspace findable
   without leaving the palette.
3. **What's New overlay + provider recovery card** — makes a broken key
   recoverable from the error the user actually sees.

## Verification

- Rust workspace: 1,306 passed, 0 failed, 17 ignored across the current suite.
- Frontend: 1,095 passed across 158 files.
- `svelte-check`: 0 errors, 0 warnings.
- Production build: clean.
