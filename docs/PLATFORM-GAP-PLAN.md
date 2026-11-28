# Platform gap execution plan — visuals and tools

**Source studied:** `nightly-labs/openbot` **0.80.0** at commit `444a421`
(2026-11-27). OpenBot is PolyForm Noncommercial; this plan is a clean-room
product comparison and no OpenBot code may be copied into RAVENBOT. Its avatar
dependency `@norbert_bodziony/bloub` is MIT, as is our existing
`open-bot-avatar` package.

**Scope:** the visual-rendering and tool gaps that fit RAVENBOT's local-first,
MIT, no-paywall product. Cloud accounts, credits, Stripe, VPS hosting, mobile
apps, remote desktop, marketplace payouts and a hosted relay are explicitly out
of scope.

**Status legend:** `planned` · `in progress` · `shipped` · `deferred`

---

## Guardrails

Every slice must satisfy all of these before it is called done:

- **No paywall, billing or card path.** No payment SDK, licence gate, trial,
  plan limit or checkout form. The `check:no-payments` gate stays green.
- **No OpenBot source copied.** Re-implement behaviour in Svelte/Rust against
  our own contracts. MIT dependencies may be used with attribution.
- **Local first and fail closed.** No cloud dependency for a feature that can be
  local. A missing engine, file or permission produces an honest error.
- **No fake capability.** If a platform or provider cannot do something, the UI
  says so.
- **Tests and docs ship with the code.** Focused unit/DOM tests, all twelve
  locales, `docs/` update, `npm run check`, `npm test`, `cargo test --workspace`,
  `npm run build`, signed commit and push.
- **Security review for every new sink.** URLs, files, HTML, IPC and agent tools
  are validated at the boundary; no raw HTML insertion; no secret in model
  context.
- **Accessibility.** Keyboard reachable, labelled controls, no colour-only
  state, reduced-motion honoured.

---

## Priority and order

| Order | Slice | Value | Effort | Risk | Status |
|---|---|---|---|---|---|
| 1 | Custom avatar upload/replace/remove | High | S | Low | shipped |
| 2 | Visual page → composer actions | High | M | Medium | shipped |
| 3 | Proactive messaging tools | High | M | Medium | shipped |
| 4 | Browser action depth | Medium | M | Medium | shipped |
| 5 | Secure API-key request | Medium | M | Medium | shipped |
| 6 | Activity-aware avatars | Medium | S/M | Low | planned |
| 7 | Chat backdrop (image/video) | Medium | M | Medium | planned |
| 8 | Avatar-aware attention island | Medium | S | Low | planned |
| 9 | Agent tasks and event waits | High | L | High | shipped |
| 10 | Agent/profile/routine tools | Medium | M | High | shipped |
| 11 | Theme palette files | Low | S | Low | planned |
| 12 | Avatar candidate shuffle | Low | S | Low | planned |
| — | Full agent dock, crew pile, server plush | Low for us | L | Low | deferred |
| — | Hosted sites, CRM, credits/media, workflow canvas | Out of scope | — | — | deferred |

---

## Phase 1 — Avatar identity

### 1.1 Custom avatar upload, replace and remove — `shipped`

**Why:** OpenBot accepts a local photo, validates its bytes and keeps it as the
agent's identity. RAVENBOT only accepts a URL today. This is the clearest visual
gap.

**Behaviour**

- A person can choose a PNG, JPEG, WebP or GIF up to **512 KB**.
- The file is validated by magic bytes, not only the browser MIME string:
  PNG (`89 50 4E 47`), JPEG (`FF D8 FF`), GIF (`GIF8`), WebP
  (`RIFF…WEBP`). SVG and unknown bytes are refused.
- The image is read as a data URL and kept in the existing `avatar_url` column
  with `avatar_style = "custom"`. No schema change, no network fetch.
- The preview updates immediately; **Use Selected Avatar** commits it.
- A stored custom image can be replaced or removed. Removing it restores the
  generated/DiceBear look and does not leave a broken image.
- A corrupt or undecodable image falls back to the generated face through the
  existing `RavenAvatar` `imageFailed` path.
- The app reports too-large, wrong-type and read failures in the picker, in the
  current locale.

**Files**

- `src/lib/components/AvatarPicker.svelte` — file input, validation, upload,
  replace, remove, error copy.
- `src/lib/i18n/locales/*.json` — `avatar.uploadImage`, `avatar.removeImage`,
  `avatar.uploadHint`, `avatar.uploadTooLarge`, `avatar.uploadType`,
  `avatar.uploadFailed`.
- `src/lib/components/AvatarPicker.upload.dom.test.ts` — new suite.
- `docs/OPENBOT-AVATARS.md` — storage, limits, fallback.

**Acceptance**

- A valid PNG/JPEG/WebP/GIF becomes the preview and, after confirm, an
  `onSelect(dataUrl, "custom")` call.
- A >512 KB file, a text/SVG file and a file with a forged MIME but wrong magic
  bytes are refused with the correct message.
- Reopening the picker with the stored data URL shows the custom image.
- Remove restores a generated/DiceBear choice and confirm no longer sends the
  data URL.
- No network request is made for the image.
- All twelve locales have the six keys.

**Risk:** a 512 KB data URL travels in the bot list payload. Cap it at 512 KB
now; if a real fleet shows payload pressure, move the bytes to
`~/RAVENBOT/cache/avatars/` in a follow-up with an `asset:` URL and a migration.

**Shipped evidence:** `AvatarPicker.upload.dom.test.ts` covers a valid PNG,
forged bytes, a >512 KB file, remove and the generated fallback; the six keys
exist in all twelve locales; the full frontend suite passes at 1,017 tests.

### 1.2 Activity-aware avatars — `planned`

- Pass the current tool activity (`searching`, `browsing`, `reading`, `running`,
  `editing`, `delegating`, `reviewing`) from `ThreadView` into the OpenBot
  avatar.
- Only apply an activity face while the mood is `working`; hold it for ~1.4 s
  so rapid tool calls do not flicker.
- Add a pure mapping in `src/lib/openBotAvatar.ts` and test it.
- Reduced motion forces a still, attentive face.
- If the renderer cannot express a distinct face for an activity, reuse the
  nearest state and document the limitation instead of pretending.

### 1.3 Avatar candidate shuffle — `planned`

- Add **Surprise me** to `OpenBotStudio`: choose a character and curated colour
  that differ from the current one, keep depth/follow/finish, update the hero.
- Never commit until Apply.
- Test that two consecutive clicks change the selection and that Apply sends the
  chosen URL.

### 1.4 Theme palette files — `planned`

- Read validated `<name>.json` files from `~/RAVENBOT/themes/`, list them under
  Settings → Appearance, hot-reload on rename/write.
- Validate `#rrggbb`/`#rrggbbaa`, `light|dark`, required tokens, filename id.
- Invalid files are ignored with a console warning and never replace the saved
  theme.
- No remote theme download.

---

## Phase 2 — Visual interaction

### 2.1 Visual page → composer actions — `shipped`

OpenBot pages can call `openbotPage.sendMessage(text)` and `setContext(text)`.
Our pages can report height, follow theme, open links and go fullscreen, but they
cannot hand a choice back to the composer.

**Design**

- Page API:
  - `window.ravenbot.compose(text)` — places text in the composer for review.
  - `window.ravenbot.setContext(text)` — attaches a short replaceable quote to
    the composer, not a sent message.
- Both require a real pointer/key press inside the page (the bridge's
  `userActed` gate) and enforce length caps (compose 10,000; context 2,000).
- The iframe→host message is validated in `VisualBlock.svelte` and forwarded as
  a typed window event; `ThreadView` and `ChatRoomView` own the draft, replace
  one context block and focus the composer.
- Nothing is sent automatically. The composer is focused and the user presses
  Send. Editor-only context markers are stripped before sending.
- `visual` pages remain on `ravenvisual://` without same-origin access.

**Tests:** `visualComposer.test.ts` covers limits, replacement and marker
stripping; `Visuals.dom.test.ts` covers compose, context, clear, oversize and
non-string messages. `visuals.md` tells the agent the API exists.
**Shipped evidence:** the full frontend suite passes; the page still cannot send.

### 2.2 Chat backdrop — `planned`

- Per-thread backdrop: none, built-in image, uploaded image or muted looping
  video (≤100 MB), with a clear/undo action.
- Store the choice in a thread/app setting; keep files local.
- Backdrop sits behind the transcript, respects theme/readability and does not
  intercept pointer events.
- Start with image/video; a sandboxed HTML backdrop is a later slice because it
  needs its own storage and permission model.

### 2.3 Avatar-aware attention island — `planned`

- Extend the attention summary with the latest item's agent id, name and avatar
  URL/style.
- The island shows the agent's real avatar and state dot instead of only text.
- Per-agent colour/shape comes from the same avatar resolver as the roster.
- No new window; reuse `AttentionIsland`.
- Test the summary mapping and the rendered avatar.

### 2.4 Full agent dock — `deferred`

The draggable dock with seated avatars, per-seat chat and voice is a project of
its own. Revisit after the island, backdrop and messaging tools ship.

---

## Phase 3 — Proactive messaging tools — `shipped`

**Why:** our connectors could receive and route chat messages, but an agent had
no direct tool to list linked places, read history or post proactively. OpenBot
has `messaging_list_places`, `messaging_history` and `messaging_post`.

**Shipped**

- Three skills:
  - `messaging_list_places` — this agent's linked Telegram chats, Slack channels,
    Discord channels and WhatsApp numbers, with the connector switch. Read-only.
  - `messaging_history` — the **local RAVENBOT conversation** for one place,
    newest last, optional text filter, 100-message cap and 24,000-character cap.
    It explicitly says this is not the platform's full archive.
  - `messaging_post` — one plain-text message (1–8,000 characters), high risk and
    approval-gated. No attachments or replies in v1.
- Per-agent scoping: a link belongs to the bot that paired it; another agent
  cannot list, read or post through it.
- The desktop and headless daemon install the same `chat_bridge` sender. The
  handler re-reads the link and connector switch and calls the existing
  Telegram/Slack/Discord/WhatsApp client. No new network stack, no token in the
  skill or the result.
- A disabled connector, unpaired place or missing bridge fails closed. WhatsApp
  free-form posts still obey Meta's 24-hour window and return the provider error.

**Tests:** core bridge contract; six skill tests including cross-agent refusal
and send-once; four connector wrapper tests that fail before any network call;
`cargo test --workspace` passes.

**Still open:** attachments/replies, platform-side history and a live post to a
real external workspace (manual check).

---

## Phase 4 — Browser action depth — `shipped`

`browser_dom` now supports, after snapshot:

- `hover`, `select_option` (by value or visible label) and `set_checked` (native
  click path so page handlers run).
- `wait_for` with selector/text/url conditions, `visible`/`hidden`/`attached`
  state and a 30 s cap. It refuses a blind wait.
- `run_steps` — 1–200 steps, 20,000-character payload, 120-second total budget,
  stops on the first failure. `open`, `eval` and nested `run_steps` are refused.
- The result reports the completed count and the last step result, not every
  intermediate value.

Every action reuses the existing page-origin evaluator and returns a structured
status; a missing selector never becomes a fake success. **Still open:** tab
list/close, downloads/uploads and a real-display Playwright/browser-dock run.

---

## Phase 5 — Secure API-key request — `shipped`

`ask_user` now has a Vault `api_key` subtype, and `save_api_key` is the
convenience request tool.

- The card accepts a machine-safe `name`, `title`, `site`, `placeholder`,
  `purpose` and `submit_label`. The owner enters the value in a password field;
  the save path calls `submit_api_key_answer`, never `answer_question`. Cancel
  is a bare `{"action_id":"cancel"}` object with no other fields.
- The value is stored in the Vault (`kind = 'api_key'`, unique `key_name`) and
  written to an atomic `0600` file in a `0700` `keys/api-keys` directory. The
  agent receives only `{name, path}`.
- `list_api_keys` returns metadata and `file_exists`; `remove_api_key` deletes
  both copies; `save_api_key` refuses any value in its arguments. Tool arguments
  are redacted before they are emitted or audited.
- Paired chats cannot answer an API-key question, and headless runs skip it.
  `submit_api_key_answer` is owner-only.
- Linux bubblewrap and Docker mount `keys/api-keys` read-only, and the macOS
  Seatbelt profile allows reads but not writes, so a shell can use a key
  without being able to rewrite or delete the owner's copy.
- Tests: the value never enters the question row, model tool result, audit
  event or list output; invalid names and whitespace values are refused; file
  and directory permissions, symlinked-directory refusal, atomic replacement,
  remote refusal and the owner-only gate are covered.
- Docs: [SECURE-API-KEYS.md](SECURE-API-KEYS.md).

**Still open:** per-process isolation of the key file, rotation history/expiry,
and provider-specific environment injection. The value is protected from the
model, not from the owner's OS account; an agent with shell access can read the
path by design.

---

## Phase 6 — Agent tasks and event waits — `shipped`

`agent_tasks` and `agent_waits` now hold durable background work and one-shot
waits. The host task loop (`src-tauri/src/agent_tasks.rs`) claims queued tasks
under dependency/resource rules, runs them on a visible task thread, and fires
waits on task transitions or messages; a timer wait is a deadline.

- Tables: `agent_tasks` (owner, target, task thread/run, state, instruction,
  expected result, lock resources, dependencies, depth, approval policy,
  result/error, timestamps) and `agent_waits` (bot, run, thread, event kind,
  exact filter, deadline, state, bounded payload).
- Tools: `start_task`, `message_task`, `close_task`, `await_event`,
  `list_waits`, `close_wait`. All High risk; task target permission uses the
  same `delegate_to`/office-roster rule as `delegate`.
- One owner; `browser` and `workspace:<absolute path>` are explicit locks. The
  workspace lock is not a filesystem grant, and the target's real workspace
  still comes from the task thread/office. At most four tasks run per process.
- A wait fires only from `state = 'waiting'` and is consumed only from
  `state = 'fired'`, so duplicate/racing events wake a run once. The task row
  and event payload are bounded; a missing/foreign/failed dependency fails the
  dependent task with a reason.
- `await_event` returns `expired`, `cancelled` or `fired` honestly, and fires
  immediately when the named task is already terminal (including after a
  restart). A message to a run outside a tool boundary is recorded in the
  thread and reported as `delivery: "thread"`, not as steered.
- Crash recovery marks `starting`/`running` tasks failed, expires unconsumed
  waits, excludes task threads from 1:1 resume, and fails their runs; the task
  loop repeats the recovery defensively on its first tick.
- Docs: [AGENT-TASKS.md](AGENT-TASKS.md).

**Still open:** MCP/webhook events as wait sources, a task board/read tool,
platform-side delivery guarantees for `message_task` outside a tool boundary,
and more than one fleet-owner process.

---

## Phase 7 — Agent/profile/routine tools — `shipped`

The ten tools are added and role-gated against the same IPC boundary:

- `list_agents`, `read_agent`, `create_agent`, `update_agent`,
  `interrupt_agent`.
- `list_routines`, `create_routine`, `update_routine`, `delete_routine`,
  `test_routine`.
- Read tools allow owner/operator/viewer; interrupt and test allow
  owner/operator; create/update/delete configuration is owner-only. A paired
  chat and any unknown actor are denied.
- **No actor means read-only.** Headless routines, office graph nodes and legacy
  runs cannot claim owner configuration powers. Delegation, background tasks
  and routine tests copy the caller's actor onto the child run.
- `create_agent` starts with no permissions, no delegation, no host control,
  Ask approvals and only permission-free memory/todo/ask skills. `update_agent`
  changes identity/model fields only; capabilities remain owner-only Settings.
- `create_routine` starts paused unless explicitly enabled; `test_routine` does
  not advance `last_run_at` or the schedule and keeps the normal Review gates.
- Routine deletion also removes its MCP event triggers.
- Tests: the full role matrix, no-actor fail-closed behavior, safe agent
  defaults, inability to add privileges through update, paused-by-default
  routines, test-without-schedule-change and the IPC command mapping.
- Docs: [AGENT-ADMIN.md](AGENT-ADMIN.md).

**Still open:** `delete_agent`, section/folder organization, avatar/identity
editing beyond name/rank/specialty, and a generic tool-discovery bridge (still
deliberately rejected).

---

## Deferred or rejected

| Item | Decision |
|---|---|
| Credits, Stripe, billing, paywall, subscription plans | Rejected: our product is MIT and has no paywall. |
| Hosted account API, VPS server, mobile apps, remote desktop | Deferred: outside the local-first desktop scope. |
| Marketplace payouts, creator revenue | Rejected. |
| Server plush logos, crew-pile physics intro, full agent dock | Deferred: delightful but not core. |
| Workflow canvas, site publishing, CRM | Deferred: different product surfaces; revisit only with a concrete user need. |
| `danger-full-access`/never-approve defaults | Rejected: our approval and sandbox model is safer. |
| Copying OpenBot PolyForm code | Rejected: clean-room only. |

---

## Release gates for every slice

1. Focused unit/DOM tests, plus the full frontend suite.
2. `npm run check` — svelte-check, UI foundation, aria, no-payment.
3. `npm run build`.
4. `cargo test --workspace` and `cargo fmt --check`.
5. `git diff --check`; twelve-locale parity.
6. Docs updated with the real limits, not the intended ones.
7. Signed commit and push; clean tree.
8. A manual live check in `docs/LIVE-CHECKS.md` where hardware/network is
   required.
