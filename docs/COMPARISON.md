# RAVENBOT vs Grok Bot vs OpenBot

> Sources read for this document:
> - **Grok Bot** — `https://www.grokbotexplained.app` (all 7 sections: overview, how-it-works,
>   jobs, avatar-system, getting-started, trust-and-cost, in-the-wild)
> - **OpenBot** — `https://github.com/nightly-labs/openbot` (cloned at `/tmp/openbot`;
>   `packages/brand/src/tokens.css`, `src/renderer/src/`, `packages/ui/src/features/`)
> - **RAVENBOT** — this repo, at commit `7911753`.

---

## 1. TL;DR

RAVENBOT is not behind on *capability* — it is ahead on several axes Grok Bot does not have
at all (real MCP consumption, real sandboxing, Ed25519 fleet sync, offices, a kill switch).
It is behind on **product surface**: Grok Bot ships status, a job library and an avatar as
first-class UI, and OpenBot ships a rendering system with motion and density discipline.

The three superpowers that actually matter, extracted:

| Superpower | Grok Bot | OpenBot | RAVENBOT |
|---|---|---|---|
| **1. Shared always-on computer** — one logged-in machine, all helpers on it, hand jobs off | Core design | — | Per-agent sandbox, in-process office handoff |
| **2. Status as the product** — 6 states, live roster, returns only when it needs you | Core design | Indicator state machine | 6 moods, present, `thinking` folded into `working` |
| **3. Oversight ladder** — Hand over / Review / Approve, per job | Core design | — | Approval mode + per-tool approvals (not surfaced as 3 tiers) |

---

## 2. Grok Bot — the product model

Grok Bot is not a chat app. It is *hiring an assistant*. Everything follows from that framing.

### The five things it claims to be

1. **It works on a cloud computer that stays on.** Always switched on, logged into your
   accounts between jobs, so it resumes instead of starting cold. **Every helper on your
   account shares that one computer**, and each one gets its own screen on it.
2. **You show it once.** Do the job yourself one time while it watches. It remembers the steps
   and repeats them. *No configuration, no prompt engineering.*
3. **It runs on a schedule** — weekday 07:00, *or whenever a certain kind of email lands.*
   You can look back at what it did on any given day.
4. **It learns your way** — your writing, your usual exceptions, who signs off on what, and
   **when to interrupt you versus carry on quietly**.
5. **They work together** — several helpers in one chat, handing jobs between themselves, with
   one keeping the others on track. You step in for the real decisions.

### The six roster states

`Idle` · `Working` · `Waiting` · `Blocked` · `Thinking` · `Done`

This is the primitive of the whole product. Every one is *something a person can do something
about.* A vocabulary that cannot be acted on is decoration.

### The three-tier oversight ladder

| Tier | Meaning | Examples |
|---|---|---|
| **Hand over** | Can run alone | overnight tidying, chore websites, repetitive known jobs |
| **Review** | Drafts, you read, then it goes | customer replies, public posts, legal/medical |
| **Approve** | Nothing happens until you say yes | spending, granting access, deleting, irreversible sends |

Grok's own caveat, worth copying verbatim into our UX copy: *"Treating two helpers as a wall
between jobs — they share one computer and the logins on it."*

### The avatar system (fully specified)

- **8 shapes**: Circle, Pebble, Squircle, Capsule, Triangle, Hexagon, Cloud, Droplet
- **16 expressions**: Neutral, Attentive, Surprised, Excited, Happy, Laughing, Angry, Sad,
  Scared, Suspicious, Confused, Curious, Proud, Shy, Unimpressed, Sleepy
- **12 colours**: Ink `#0a0a0c`, Brown `#8b5e3c`, Red `#e8483f`, Orange `#f08a24`,
  Amber `#f0b429`, Green `#3ecf8e`, Turquoise `#2fbfa0`, Blue `#3b93f0`, Purple `#8b5cf6`,
  Pink `#e152b0`, Grey `#a3a3a3`, Cream `#f1efe9`
- **15 animation states**: Idle, Thinking, Wink, Wide eyes, Alert, Notification, Exclamation,
  Sleep, Egg, Hexagon, Play, Orbit, Swirl, Burst, Comet

**The separation that makes it work**: the *expression* is identity (fixed, from the agent),
the *animation state* is what the engine plays over it. State is how you read the roster.
Orbit = long-running work, Notification = "a blue dot appears, it needs you".

The reference implementation samples 64 points around each body and projects two eyes onto its
surface — which is why any shape/expression/colour combination works, including ones with no
pre-rendered artwork.

### The job library

**56 ready-made helpers**, each with: a **group** (Marketing / Engineering / Sales & GTM …), a
**cadence** (one-off · recurring · monitoring) and an **oversight level** (low / review /
approval). Searchable and filterable. Most popular jobs have *nothing to do with computers* —
sales, hiring, marketing, money.

---

## 3. OpenBot — the render system

SolidJS 2 / Tailwind v4 / Kobalte, Electron. `packages/brand/src/tokens.css` is the single
source of truth; the desktop renderer, web app and mobile app all `@import` it and none
declares a token of its own. A test enforces that.

### Shell
- **CSS grid, not flex**: `grid-template-columns: var(--server-rail-width) var(--left-panel-width,280px) 0 minmax(0,1fr)`
- Rail (64px) → sidebar (280px, resizable 128–400, auto-compacts to 88px) → middle pane
  chosen by a **precedence ladder** — channel > DM > agent conversation
- **Conversation minimum width 424px**; below that the sidebar auto-compacts
- Panel resize animated `cubic-bezier(0.32,0.72,0,1)`; readiness gating in one place

### Palette (always-dark)
canvas `#1a1a1a` · surface `#212121` · raised `#242424` · control-hover `#2a2a2a` ·
glass `rgba(33,33,33,0.76)` · borders `rgba(255,255,255,0.035–0.045)` (near-invisible) ·
text `#fff`/`#dcdcdc`/muted `#979797`/dim `#6a6a6a` · accent blue `#007cf7` ·
status as **roles not shades**: success `#31cf76`, warning `#ff9412`, danger `#fd2f3b`.

Type: Inter Variable, 6-step **12/13/14/16/18/24** paired 1:1 with line-heights 16–30px.
Spacing 2/4/6/8/12/16/24/32. Radii 4/6/8/12/**14 card**/20/**22 bubble**/30 modal/999 pill.
Control heights 24/28/32/36/40. Icon stroke 1.5 (Lucide 24 viewBox).
z: dropdown 40, popover 60, dialog 100, toast 120, dialog-host 160.
Durations 120/160/200/240ms; ease-out `cubic-bezier(0.23,1,0.32,1)`; **hover duration 0ms**.

### Chat
- **One shared `ChatMessageRow`** for agent *and* channel chat — reads no context, everything
  is a prop
- Own messages right-aligned with **no avatar and no name**
- Agent messages left; **avatar + name shown once per consecutive-author run**
  (`chat-grouping.ts`), continuation rows get a **gutter spacer** so bubbles stay aligned
- Bubble variants: user `#242424` · agent-with-code/tool = **ghost** (no bg, no padding) ·
  plain agent = muted `#212121` · media-only = ghost. Radius 22, `max-width: min(80%,720px)`
- **Tool calls are not bubbles** — `ChatActionMarker` rows, 24px tall, 16px mini-avatar,
  labels `queued → … → interrupted`
- **Word-by-word reveal decoupled from token arrival**: 60ms gap, smooth height animation,
  per-word tail fade (400ms, 2px blur), full `prefers-reduced-motion` bypass
- **Queue panel**: reorderable held messages, custom vertical-drag engine, a "steer" action

### Motion
Digit-roll counters (500ms, `cubic-bezier(0.34,1.45,0.64,1)`, 70ms stagger), modal scale
0.96→1 @250ms, panel slide `translateY(100px)` + blur @400ms, input shake, error auto-revert
hold 3s. **Every animation has a `!important` reduced-motion opt-out.**

---

## 4. RAVENBOT — what we actually have

Baseline at `7911753`: `svelte-check` 0 errors/0 warnings, `vitest` 250 tests passing across
20 files, 16 Rust crates.

### Ahead of Grok Bot
- **135+ MCP connector catalog**, per-bot and batch assignment, live connection tests
- **Is itself an MCP server** (`ravenbot mcp-serve`) — external agents can drive the fleet
- **Real sandbox**: resource quotas, per-run network policy, kill switch, audit log,
  prompt version control with diff/rollback
- **Ed25519 fleet sync with TOFU** trust registry and per-bot key binding
- **Offices**: rank + specialty bots, orchestrator, parallel DAG execution, blackboard sharing
- **25 built-in skills + 1,497 community** skills, OpenAPI plugin import
- **Routines**: real cron executor, webhook triggers
- **Voice, vision, image gen, extended thinking, artifacts canvas, DeepSearch citations**

### Behind Grok Bot

| Gap | Grok | Us | Cost |
|---|---|---|---|
| **Avatar completeness** | 8 shapes / 16 expr / 12 colours / 15 anims | 6 / 6 / 10 hues / 8 | **High** — it is the product's face |
| `thinking` as a distinct state | yes, one of six | folded into `working` | Medium |
| Job library with cadence + oversight | 56, grouped, filterable | 25 skills, no cadence/oversight metadata | **High** |
| Oversight ladder surfaced in UI | Hand over / Review / Approve | approval mode + per-tool approvals | **High** |
| Shared logged-in computer | one machine, all helpers | per-agent sandbox | Structural |
| "You show it once" | demonstration capture | memory + RAG + self-review | High |
| Content-triggered jobs | "when a certain kind of email lands" | cron + webhook only | Medium |
| Interrupt-vs-carry-on quietly | explicit | implicit | Medium |

### Behind OpenBot (render)
- `ThreadView.svelte` is **2,416 lines** and `ChatRoomView.svelte` ~2,400 — OpenBot splits by
  concern; these are the two places density regressions hide
- Bubble-variant discipline (ghost for tool/code) — we render tool calls in the message flow
- Per-author grouping with gutter spacer — `chat/grouping.ts` exists, worth confirming it
  covers the run case
- `prefers-reduced-motion` is honoured for the avatar; needs auditing across the app

---

## 5. What gives these bots their superpowers (the extraction)

Three things, and they compound:

1. **A shared, always-on computer with persistent sessions.** Not a sandbox that resets —
   a machine that is *logged in* and stays that way. This is what turns "can it do this job?"
   from a capability question into a memory question. Grok's helpers can do anything a logged-in
   employee can, which is an enormous surface, and it is why they need the oversight ladder.

2. **Status as the primary object, not a side-effect.** Six states, each actionable, on a live
   roster. The user never asks "what's happening?" — they read it. That is why Grok can be
   left alone overnight: the absence of a signal *is* the signal.

3. **An oversight ladder, per job, not per app.** Hand over / Review / Approve. Most jobs are
   Hand over and cost almost nothing to supervise. The expensive attention is spent only on the
   irreversible tier. This is what makes delegation economically sane at scale — and it is the
   single most copyable idea here, because it needs no new backend.

RAVENBOT already has the hard parts (sandbox, approvals, offices, MCP, memory). What is missing
is the **product surface**: the ladder, the job library, the full avatar, and `thinking`.

---

## 6. Plan

### Done

- [x] **Avatar → full Grok spec.** 8 shapes, 16 expressions, 12 named colours, 15 animation
      states, `thinking` as a real mood, legacy silhouette ids mapped forward so stored data
      keeps rendering. `UNREACHABLE_MOTIONS` is down to the three states that genuinely replace
      the outline (`egg`, `hexagon`, `play`), and `rosterSafeMotion()` enforces that at the
      component rather than at every call site.
- [x] **Connector centre: blockers broken down by cause.** "Needs Keys" was one true, useless
      number. It is now four overlapping, actionable ones, classified from the real catalog
      (135 connectors, 116 distinct key names) by `lib/connectors.ts`. This is the oversight
      ladder's shape applied to configuration: name the tier, count it, make it clickable.
- [x] **Measured UI pass** on Settings, the connector centre and the composer — contrast floor,
      hit targets, accessible names, 11px type floor. Harness in `scripts/ui-audit/`.
- [ ] **Job library** — metadata (`group`, `cadence`, `oversight`) over the existing skills, so
      a new agent can be created from a template the way Grok's Marketplace works. The connector
      centre is now the template for this shape.
- [ ] **Render polish** — split `ThreadView` (2,416 lines) and `ChatRoomView` (~2,400), add
      OpenBot's bubble-variant discipline (ghost bubbles for tool and code content).

---

## 7. The connector centre, after design review

A design review said the connective tissue was right and asked for seven
refinements. All seven are in, and the restraint was kept: no bigger cards, no
larger logos, no gradients, no badge inflation, no wider sidebar, no table.

| Asked for | Now |
|---|---|
| Separate status filters from category filters | Two labelled groups: **Status** and **Category** |
| "Needs Keys" is ambiguous | **Needs configuration**, broken down by cause |
| The card menu needs a stronger affordance | Names its card, always visible, hover-fill, `aria-haspopup` |
| "Global" is ambiguous | **All agents**, with the scope in the tooltip |
| "Unverified connector" deserves an explanation | Tooltip says *why*; the line offers **Test to find out** |
| The status strip should be operational | **System status** — connections, models, **last checked** |
| Stronger title↔action relationship | Count under the title, search full width, actions on the filter row |

Plus the one *opportunity* rather than refinement — the thing the review called
the next level:

> The UI makes the user think "I have 135 connectors." It should make them think
> "I know exactly which connectors require my attention."

**Needs configuration (112)** now shows what the 112 are waiting on:

| Cause | Count | What it is |
|---|---|---|
| Missing endpoint | 20 | a URL, URI or connection string to point it at |
| Missing API key | 40 | a service key from the provider's dashboard |
| Missing token or OAuth | 57 | a personal access token, password or OAuth credential |
| Unverified launcher | — | the upstream package was not found in the catalog audit |

Each is clickable and narrows the grid. They deliberately **overlap** — Elasticsearch
wants a URL *and* a key — and the panel says so rather than implying the counts sum.
The 23 keyless local connectors (Filesystem, Git, Docker, Playwright) appear in
none of them, because they need nothing.

### The state machine

The review's closing note asked for
`loading → ready → configured → unverified → error → disabled → assigned → globally assigned → connection testing → configuration`.

Eight of nine existed. The two that did not were the two that mattered:

- **error** — a connector shown as **failed** forgot the moment you dismissed the
  modal, because the result lived only in the modal. It is now remembered per
  connector, and a failure outranks missing credentials: a tested-and-refused
  connection is a fact, and a missing key is a guess.
- **connection testing** — a test in flight now says so, instead of the card sitting
  there looking ready.

`disabled` is deliberately not invented. There is no source of truth for it in the
catalog, and a state that always reads one value teaches people to ignore it.
