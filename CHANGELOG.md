# Changelog

All notable changes to RAVENBOT are documented here.

## Unreleased

### Rendering and harness parity

- **OpenCode-style prose rendering** — message Markdown now uses the
  session-ui rhythm RAVENBOT was missing: 14px body at 160% line height,
  17/15/14px headings with hierarchical margins, 32px list indents, 8px
  list-item spacing, hover-only link underlines, and quiet 8%-fill inline
  code. The token renderer and its HTML/link safety checks are unchanged.
- **Streaming block memoisation** — completed markdown block tokens are reused
  across each streaming re-lex, so only the growing block (and any block whose
  fade offset changed) re-renders. Link reference definitions still force a
  fresh tree when they change, so a late definition cannot leave a stale href.
- **Off-thread markdown and highlighting** — settled long markdown bodies and
  long code blocks are lexed/coloured in a module Web Worker using the same
  token and highlighter code as the UI thread. Streaming and short bodies stay
  synchronous; worker timeout/crash falls back with bounded, coalesced caches.
  The thread and office views prewarm long bodies so scrolling back is a cache
  hit.
- **Measured timeline virtualisation** — both the 1:1 transcript and the office
  feed now mount only the viewport plus overscan. Variable row heights are
  measured, unmounted rows use a content-aware estimate, and top/bottom spacers
  keep the scrollbar honest. Loading earlier follows the top mounted message by
  key, search/reply/rail jumps mount their target, and the rail reads virtual
  offsets instead of relying on a DOM row being present. The live streaming row
  and approvals stay outside the virtual list.
- **Harness audit** — `docs/OPENCODE-PARITY.md` maps OpenCode's desktop and
  markdown worker/cache system against RAVENBOT. The P0/P1 harness gaps are
  shipped; remaining work is MCP-event jobs, public session sharing, and the
  tool-part renderer cards.

### Persistent 1:1 goal loop

- **Unattended goals in any conversation** — every 1:1 thread now has a Goal
  control. The agent keeps planning, acting and verifying in that same thread
  until the completion contract is met, the round budget is exhausted, the
  goal is blocked, or the user stops it.
- **Same review as an office** — each round runs the normal agent with tools,
  then reuses the office completion judge, `goal_gates`, and deterministic
  quality gates. A red test/build gate overrules a model that claims done.
- **No hidden conversations** — the graph node is pinned to the user's thread,
  so the answer, tool trace and per-round verdict all appear where the user is
  watching. Starting a goal never creates a second thread.
- **Cost ceiling** — a goal can carry an optional token budget (0 means
  unlimited). The banner shows spend against the ceiling, and a paused goal
  can raise its rounds or tokens before resuming; history and spend are kept.
- **Safe scheduling** — goals are read with a thread-only due query, run one
  turn at a time, and yield to any live or user-paused run in the same thread.
  Repeated judge failures park the goal instead of burning tokens forever.
- **Restart-safe** — an interrupted goal round is closed honestly on startup
  and the goal is re-armed for a fresh round; it is never resumed as a raw run
  that would skip the review step. The headless fleet daemon runs the same
  heartbeat, so goals continue without a window.
- **Lifecycle** — start, pause, resume and stop are persisted. Stop also
  cancels the round in flight, and a background round emits a small event so
  the open conversation refreshes its banner and transcript. A goal that
  finishes, parks, or exhausts its budget raises one short OS notification
  with no prompt text, so an unattended run can ask for attention without
  leaking its contents onto a lock screen.

### Snapshot and revert

- **Every mutating run gets a before-image** — the first file-writing or
  shell/git/package tool in a run walks its workspace roots and stores each
  regular file as a SHA-256 blob under `<RAVENBOT_HOME>/snapshots`, with the
  path manifest in SQLite. Read-only runs pay nothing; generated directories
  (`node_modules`, `target`, `.git`, caches) are skipped.
- **Revert a run or one file** — Projects shows a Revert action on every
  snapshotted change, and Runs & Diagnostics previews a whole-run revert before
  changing anything. A run revert restores the files it wrote and deletes the
  files it created; reverting one change restores that path to before its run.
- **Newer work is protected** — a later run's write, a file edited after the
  run, an unavailable before-image, or a symlinked/replaced path blocks the
  revert. The preview names every conflict, and only a second explicit
  confirmation sends `force`. Partial snapshots refuse deletion entirely rather
  than guessing which files existed.
- **Private, inspectable storage** — blobs are content-addressed, deduplicated,
  and written with user-only permissions; symlinks are never followed into the
  store. Startup and run persistence are unchanged; a snapshot survives a
  restart and its manifest is queryable like the rest of the ledger. Shell
  changes that never produced a `file_changes` row are deliberately outside a
  run revert: the ledger is the list of paths a revert may touch.
- **Bounded disk** — manifests older than seven days are pruned by the existing
  audit-maintenance loop, and unreferenced blobs older than a day are removed.
  `RAVENBOT_SNAPSHOT_RETENTION_DAYS=0` keeps them indefinitely;
  `RAVENBOT_SNAPSHOT=0` disables capture on a workspace where a tree walk is
  unwanted.

### In-process patch application

- **`file_edit` / `apply_patch` now accepts a standard unified diff** as well
  as a Codex `*** Begin Patch`. The diff is parsed and applied in-process:
  exact hunk context is required, a mismatch changes nothing, and new files,
  deletions, multi-file diffs, CRLF files, and `\ No newline at end of file`
  are handled. Paths must stay inside the workspace and symlinked/replaced
  parents are refused.
- **Atomic and recoverable** — every file is validated before the first write;
  writes use a temp file plus rename and preserve the existing mode; a failure
  mid-apply restores the files already written. The run snapshot and
  `file_changes` ledger see each file with its absolute path, diff, and
  before/after hashes, so a patch is as revertable as any other write.
- `code_edit` remains available for the system `patch(1)` path, but the common
  edit loop no longer depends on that binary.

### Isolated Git worktrees

- **Opt-in per conversation** — a thread can mark future runs as isolated. The
  first run creates a real `git worktree` under `<RAVENBOT_HOME>/worktrees`;
  later runs in the same conversation reuse it, so a multi-round goal sees its
  own previous work. The main checkout is untouched until the user applies.
  The checkout is built from HEAD; small Git-ignored local files (for example
  an `.env`) are copied in for reading, but ignored changes are not part of the
  apply patch.
- **Strict, fail-closed setup** — isolation requires a Git repository with no
  uncommitted or untracked changes. A dirty or non-Git project stops the run
  with a clear message rather than silently writing to the main folder.
- **Review, apply or discard** — the conversation banner shows an active
  checkout and opens a diff/file-list review. Apply runs a checked
  `git apply` against the original checkout and removes the worktree; a
  conflicting main checkout is refused and the worktree stays available.
  Discard removes the checkout without touching the project.
- **Coherent with goals and snapshots** — goal review, workspace snapshots and
  quality gates run against the checkout that actually produced the round.
  Run snapshots and per-run revert continue to work on the isolated paths.
- Turning isolation off does not throw away an active checkout; the banner
  remains until it is applied or discarded.

### Language diagnostics

- **A real LSP client** — RAVENBOT detects installed `rust-analyzer`,
  `pyright-langserver`, `typescript-language-server`, `gopls`, and `clangd`
  servers, starts them over stdio, opens the files a run changed, and reads
  `textDocument/publishDiagnostics`. It does not bundle or fake a server.
- **Agent tool and goal gate** — the `diagnostics` tool (available with the
  Shell capability) checks named files or the run's changed files. Every 1:1
  goal round checks its changed files; error diagnostics are added to the
  judge's failures so a goal with type errors cannot be marked done. Warnings
  do not block, and a missing server/timeout falls back to the deterministic
  gates.
- **User surface and configuration** — Projects has a Diagnostics button on the
  open file. Settings → Diagnostics → Language servers edits the `lsp_servers`
  JSON map; save `{}` to restore the defaults. `RAVENBOT_LSP_TIMEOUT_SECS`
  controls the deadline (default 12s). See `docs/LSP.md`.

### Platform, remote and product surfaces

- **Headless fleet daemon** — the installed service runs `ravenbot daemon`,
  which owns the database, runtime, scheduler, routine **and workflow** webhook
  execution, scheduled workflows, office loops, audit maintenance and remote
  listener with no window or display. On Linux, lingering keeps it reachable
  after logout. The desktop app and daemon share an exclusive
  `~/RAVENBOT/fleet.lock`; launching the desktop app offers to stop the service
  and take over, and a second daemon exits instead of duplicating schedulers.
  Provider credentials refresh every two minutes so a keychain that unlocks
  after login is picked up. Attention push uses an OS notification when a
  session exists, plus the configured HTTPS webhook and SMTP alert.
  `ravenbot remote status|pair|revoke` administers remote devices without a
  window. Windows gets a per-user Task Scheduler logon task (not a service).
  See `docs/SERVICE.md`.
- **Authenticated remote access with RBAC** — an opt-in axum listener, one-time
  pairing codes, SHA-256 token storage, `viewer` / `operator` / `admin` roles,
  mutation audit, device revocation, self-revocation from the paired device,
  and a mobile web client served by the same listener. Remote management stays
  local; the API exposes conversations, approvals, questions and runs only. See
  `docs/REMOTE-ACCESS.md`.
- **HTTPS for remote access** — the listener can generate and persist a
  self-signed certificate for localhost and the current LAN IP, serve TLS, and
  show its SHA-256 fingerprint in Settings. Encrypts tokens and message bodies
  on a trusted LAN; a private tunnel or reverse proxy remains the recommended
  path for anything public.
- **Attention island** — a compact always-on-top Tauri window that polls pending
  approvals/questions and running runs, shows the newest item, and opens or
  hides itself. Available from the tray.
- **Shared workspaces and roles** — owner/operator/viewer grants across local
  accounts, a server-side fail-closed IPC role gate, a launcher workspace picker,
  a workspace-access card for owners, operator/read-only UI banners, account
  rename/delete, and in-launcher creation of additional accounts.
- **Concurrent account windows and per-account daemons** — separate port blocks
  per workspace, per-account systemd/launchd/task units with their own keychain
  namespace and ports, and no desktop-wide lock.
- **Native notifications without the plugin boot script** — attention
  notifications now use `notify-rust` directly on every platform. This removes
  the Tauri notification plugin's JS initializer, which probed
  `is_permission_granted` on every page and produced a 500 (missing capability)
  plus an invalid-Origin IPC rejection on a webview error page. A regression
  test keeps the plugin dependency from returning.
- **YOLO is the default, destructive operations are blocked** — every existing
  and new agent now runs safe reads, writes, builds, tests, shell work and
  delegation without an approval card. A hard safety policy runs before the
  approval ladder and refuses destructive database SQL/clients, disk and power
  commands, git-history rewrites, workspace-root moves/deletes, `.git` deletes,
  and writes/deletes of RAVENBOT's own database, account, key and configuration
  state. Office, routine, channel and chat-app runs use the same policy.
  `Ask`/`Auto` remain explicit per-agent choices; migration 062 sets existing
  agents to Full.
- **Interrupted work resumes after restart** — startup reconciliation now parks
  interrupted 1:1 agent runs as paused instead of failing them, keeps their
  checkpoint and transcript, and a startup worker resumes them after the UI is
  ready. Interrupted workflows are re-queued and run on the first scheduler
  tick; running channel tasks return to pending. Set
  `RAVENBOT_RESUME_ON_START=0` to disable automatic agent resumes. An office
  turn that was mid-run restores its saved graph: completed nodes keep their
  outputs, and an interrupted node with a checkpoint resumes that exact run
  (same run id, token totals, and in-flight tool results) instead of starting
  over. Pending office messages still run normally.
- **Office work continues across milestones** — an office turn runs up to four
  bounded plan→execute→synthesize cycles (`RAVENBOT_OFFICE_CYCLES`, 1–8), asks
  the lead to review product completion after each cycle, and forces a
  verification cycle after the first software milestone. The planner plans one
  3–5 task milestone per cycle instead of pretending the first plan is the
  whole product, and the default tool-round budget is 40.
- **Long context keeps a structured handoff** — compaction now writes the
  task/goal, current request, file paths, tools/commands, errors, and earlier
  context, then keeps a token-budgeted tail without splitting a tool call from
  its result. This costs no extra model call.
- **Past decisions are searchable** — migration 064 adds FTS5 over the durable
  transcript, and the read-only `session_search` tool returns ranked snippets
  (optionally scoped to one thread), so an agent can recover a decision, path,
  command, or error that compaction summarized away.
- **Compaction is durable** — migration 069 persists the structured handoff
  with the last core message it covers. A later run loads
  `summary + messages after the floor` and replays the summary before the
  surviving transcript, instead of reloading and recompacting the whole thread.
  The floor is derived from a core-message mapping beside the model messages,
  so it is the last message actually summarized. Ephemeral threads and a
  vanished floor fall back to the full transcript.
- **Micro-compaction** — before dropping messages, old tool results outside the
  newest twelve rows are replaced with a one-line recovery pointer. A tool call
  keeps a result, the conversation structure survives, and stale build logs stop
  consuming the window. If stubbing is enough, no summary is created; if not,
  the full handoff runs on the original messages.
- **Real background tasks** — `task_runner` now honors `background: true`,
  returning a process id immediately and capturing a bounded 200k-character
  output tail. `wait` releases on exit, a matching pattern, or a timeout;
  `status`, `processes`, and `stop` manage the registry. A supervisor task owns
  the child with `kill_on_drop`, so app shutdown does not orphan it. Host-tier
  commands no longer apply a user-wide `ulimit -u` that made ordinary forks fail
  on a busy machine.
- **Auxiliary context summary** — a full handoff can be rewritten by a model
  call using a bounded digest of the dropped messages. It uses the bot's
  provider/model by default, can be pointed at a cheaper model with
  `RAVENBOT_CONTEXT_SUMMARY_MODEL=provider/model`, and can be disabled with
  `RAVENBOT_CONTEXT_SUMMARY=off`. The transcript is treated as data, the call
  is recorded in the utility ledger, and any failure, timeout, or empty answer
  keeps the deterministic handoff.
- **Run liveness is progress-based** — migration 065 adds `runs.heartbeat_at`,
  `phase`, `last_tool`, and `last_error`, plus an append-only `work_events`
  ledger. Model, tool, approval, and checkpoint boundaries touch the heartbeat;
  long waits hold a 20-second lease. Office nodes stop on a stall
  (`RAVENBOT_NODE_STALL_SECS`, default 1800) rather than a fixed wall clock,
  with `RAVENBOT_NODE_MAX_SECS` as the final cap.
  `RAVENBOT_NODE_TIMEOUT_SECS` remains a stall-threshold alias.
- **Durable work ledger for office tasks** — migration 066 adds `work_items`,
  task dependencies, comments, and artifacts. Office plans and office-scoped
  workflows seed a work item per graph node; execution claims it atomically,
  records attempts and a 15-minute lease, and mirrors done/failed/skipped
  transitions into `work_events`. A loop iteration or loop-group reset reopens
  the item without consuming an attempt.
- **Task drawer, review and rework** — clicking a card on the office board opens
  its durable record: instruction, result/error, attempts, comments, artifacts,
  and event timeline. It can add a comment, retry a failed or blocked task,
  block/unblock, request review, approve, or request changes. Retry, unblock,
  and rework reset the node and its descendants in the saved graph and resume
  the office; the reviewer's note becomes a `[Rework requested]` message in the
  implementer's thread. A request-changes path is a real graph rerun, not a
  status-only edit. Files the runtime journaled as changed are attached to the
  task as artifacts automatically, so the drawer lists what the run actually
  produced.
- **Generated STATUS.md projection** — after each graph, the ledger's tasks are
  written into the workspace's `Shared/STATUS.md` between markers. Agent notes
  outside the markers are preserved, and the SQLite ledger remains
  authoritative.
- **Quality gates decide what "done" means** — before an office accepts the
  lead's completion verdict, it detects and runs the workspace's own gates:
  `cargo test`, `npm test`, `npm run build`/`check`, or `go test ./...`. Up to
  two run per review with `CI=1` and a 600-second timeout. A red gate vetoes
  completion and its output tail becomes the next cycle's task. Every gate run
  is persisted in `office_gate_runs` and posted to the room, so a false "done"
  cannot slip through on the model's word alone.
- **Persisted completion contracts** — migration 068 adds `goals` and
  `goal_gates`. Each office gets one contract from its mission: outcome, stop
  condition, reviewed-cycle budget, and up to four custom verification
  commands. The contract's turn budget caps the cycle loop. Custom gates run
  through the same hard safety policy as `shell_exec`; destructive commands are
  refused at save and at run. Each reviewed cycle records its verdict, reason,
  and turn count on the goal, and a done goal is not reopened by a late review.
  Office settings has a Completion contract section for editing the outcome,
  stop condition, commands, and cycle budget.
- **Goal budgets and pause/resume** — the office re-reads its contract before
  every cycle. Token usage is measured from the office's linked runs and never
  decreases; token and wall-clock budgets stop the next cycle. Pause and resume
  are persisted goal states, with controls in Office settings; a done, failed,
  or cancelled goal can be reopened for a new mission. The completion
  review now returns a strict `done | continue | blocked | wait` verdict:
  blocked and wait stop without a false finish, and wait pauses the goal until
  it is resumed.
- **Office and agent buttons no longer freeze the UI** — the agent builder's
  capability hydration reset its `capabilityToggles` state and then read it
  back while applying stored permissions. Any agent with a permission entry
  re-triggered the effect on every pass, producing Svelte's
  `effect_update_depth_exceeded` and starving input and paint. The map is now
  built locally and assigned once. Regression tests cover a permission-bearing
  agent, every rail destination, office settings and save, office switching,
  the browser side panel, and the office board.
- **Closing the app does not log out** — the launcher remembers the selected
  account/workspace session in an owner-only `config/last-session.json` and
  extends it to 30 days on startup. Next launch reopens that workspace directly.
  Explicit log out revokes the session, deletes the file, and returns to the
  launcher. In dev, closing the account window now stops the launcher instead of
  bouncing to the login page.
- **The Raven is the default agent** — first-run setup now asks for The Raven
  by name, and incomplete provider/default-agent setup reopens after login even
  if the modal was dismissed. Settings has a dedicated **Default agent** page
  instead of hiding the choice under Models → Office designer. Existing agent
  selection and replacement remain available.
- **Marketplace button opens reliably** — the rail storefront button now
  ignores a stale selected-agent id, falls back to a real agent, and opens the
  catalog even when no agent exists. Equip actions explain that an agent is
  required instead of doing nothing. Regression tests cover the rail callback,
  stale-id resolution, and no-agent catalog mode.
- **Dev launcher keeps Vite alive after login** — in `tauri dev`, the account
  launcher now hides instead of exiting, waits for the account window, and
  reappears when that window closes or logs out. Previously the launcher’s exit
  stopped Vite, leaving the newly spawned account window on a
  `localhost:1420` connection-error page.
- **Dev server ignores runtime data** — Vite no longer watches
  `accounts.sqlite`/WAL/SHM files, `ravenbot.db`, logs, caches, keys, or agent
  workspaces. The checkout doubles as the app data root, so database writes were
  reloading the entire UI dozens of times per second. A regression test guards
  the watch patterns. The debug binary also refuses to launch when the Vite dev
  server is unreachable, printing `npm run tauri dev` instead of opening
  WebKit’s connection-error page.
- **Linux validation in CI** — CI now runs the live Docker sandbox test with
  `alpine:3.20` and `systemd-analyze verify` against the generated unit, so
  container mounts, limits, network isolation, and unit hardening cannot regress
  silently.
- **Hardened Linux daemon unit** — the generated systemd service now applies
  `NoNewPrivileges`, `PrivateTmp`, `ProtectSystem=full`, protected kernel
  interfaces, `RestrictRealtime`, `LockPersonality`, and `LimitCORE=0`, while
  still allowing the RAVENBOT root and workspaces under `$HOME`.
- **Real Linux Docker sandbox tier** — selecting Docker now runs the command in
  a read-only container with workspace mounts, dropped capabilities,
  `no-new-privileges`, CPU/memory/PID/file-size limits, a private `/tmp`, and
  `--network none` when the policy blocks egress. It is no longer a silent
  bubblewrap fallback. Tested live with Docker and `alpine:3.20`.
- **Context breakdown and compaction delta** — the composer meter now shows a
  disjoint system/tools/memory/messages token estimate and the last
  `Compacted 84k → 31k` delta in its title, alongside the percentage and
  compacted badge. The four buckets are persisted with the assistant message,
  so a reload restores the breakdown as well as the total.
- **Generated-image cards and artifacts** — `image_gen` results render as an
  image card with the prompt, provider, requested size and an open link. A live
  card appears while generation runs, and a failed call shows the provider
  error instead of a blank box. Hosted results are downloaded once into
  `cache/generated-images` and attached to the final assistant message, so the
  image survives URL expiry and can be sent through chat adapters.
- **Voice uses the configured key** — microphone transcription and the
  `voice_input` skill prefer the OpenAI key saved in Settings, with
  `OPENAI_API_KEY` as the headless fallback, matching TTS. The `image_gen` skill
  receives the same key, and the Whisper endpoint remains overridable for
  compatible gateways.
- **Persistent context meter** — assistant messages store the estimated prompt
  tokens, model window and whether that turn compacted history. Reloading a
  thread restores the composer meter before the next run; simple conversational
  turns record the same estimate, and an office summary carries the newest node
  context and its four buckets. See `docs/roadmap-p0-p3.md`.
- **Inbound and outbound chat attachments** — all five native adapters send
  assistant screenshots/files back to the chat. Telegram, Slack, Discord and
  WhatsApp upload directly; Teams uses the Bot Framework file-consent card and
  uploads after the user accepts. Inbound files are size-capped and image bytes
  are available to vision models.
- **Per-run actor attribution** — desktop and chat runs record the acting
  account or external sender, role and workspace in `run_actors`;
  shared-workspace user messages carry the actor name, and workspace access is
  audit-logged.
- **Live and platform validation** — ignored live-provider checks for Telegram,
  Slack, Discord, WhatsApp and Teams credentials, an attachment-URL download
  check, and macOS/Windows CI contract jobs for service units and account
  handoff. See `docs/LIVE-CHECKS.md`.
- **Chat approvals and questions** — native interactive cards: Telegram inline
  keyboards, Slack Block Kit buttons, Discord components, Teams Adaptive Cards
  and WhatsApp reply buttons, plus `approve`/`deny`/`answer` fallback commands.
  Actions are checked against the paired conversation before deciding.
- **Per-adapter inbound throttling** — each native chat adapter limits a
  conversation to 30 handled messages per minute, so a paired chat cannot flood
  the local bot.
- **Local accounts** — a real login launcher, Argon2id passwords, expiring
  hashed sessions, login throttling, per-account data roots and keychain
  namespaces, owner migration from the existing single-user database, a desktop
  lock, session revocation checks, and `ravenbot account` administration.
  Shared-workspace roles and per-account daemons remain future work. See
  `docs/ACCOUNTS.md`.
- **Native Microsoft Teams chat app** — Azure Bot/Bot Framework messaging
  endpoint with RS256 OpenID key validation, issuer/audience/`serviceurl` claim
  checks, a mandatory tenant allowlist, app password in the keychain, one-time
  conversation pairing, personal/mention handling, `pair`, `new`, `stop`,
  `status` and `unlink`, connector replies with a cached token, duplicate
  activity idempotency and a Review notice. Runs in the desktop and the headless
  daemon.
- **Native WhatsApp Cloud API chat app** — signed HTTPS webhook with
  `X-Hub-Signature-256` verification, verify-token challenge, loopback listener
  with reverse-proxy guidance, access token/app secret/verify token in the
  keychain, phone-number pairing, `pair`/`start`, `new`, `stop`, `status` and
  `unlink`, reply delivery, duplicate-message idempotency, and a Review notice.
  Runs in the desktop and the headless daemon.
- **Native Discord chat app** — outbound Gateway WebSocket (no public
  interactions endpoint), bot token in the keychain, Message Content intent,
  one-time channel pairing, DM/mention handling, `pair`, `new`, `stop`,
  `status` and `unlink`, mention-safe replies, duplicate-message idempotency,
  and a Review notice when a run needs approval. Runs in the desktop and the
  headless daemon.
- **Native Slack chat app** — Socket Mode (no public URL or open port), app and
  bot tokens in the keychain, one-time channel pairing, DM/mention handling,
  `pair`, `new`, `stop`, `status` and `unlink`, same-thread replies, and a
  Review notice when a run needs approval. Runs in the desktop and the headless
  daemon.
- **Native Telegram chat app** — a bot token, one-time chat pairing, long
  polling, `/start`, `/new`, `/stop`, `/status` and `/unlink`, per-chat thread
  routing, reply delivery, and a Review notice when a run needs approval. Runs
  in both the desktop and the headless daemon; no public webhook port.
  MCP connectors remain available for other services. See `docs/CHAT-APPS.md`.
- **Voice hardening** — local transcription uses a per-call scratch directory
  instead of a shared `ravenbot_stt/input.wav`, so two recordings cannot
  overwrite each other. Local TTS writes its embedded inference script into the
  cache, so bundled builds work without a developer checkout. Both audio tools
  report whether the paid or local engine ran, so a local fallback is not
  charged as a provider call.
- **Accessibility gate expanded** — Settings (providers, local/background
  service, remote access), NotificationSettings, OfficeSettings, BrowserDock,
  WorkspaceBrowser and the attention island now fail on axe violations. Invalid
  tablist/tree roles were replaced with honest group semantics, and the Settings
  dialog has an accessible name.

### Repository integrity

- **Restored the missing `ravenbot-skills` sources** — `safety.rs`, `local.rs`,
  the browser/document/slide modules, and the visuals/slides/fframes workflow
  prompts were referenced by the crate but absent from git. The cause was the
  unanchored `skills/` pattern in `.gitignore`, which also matched
  `crates/skills/`; every new file in that tree was silently ignored. The
  runtime-data patterns are now anchored to the repository root, and the
  missing modules are implemented with tests: the workspace-aware command
  denylist, versioned local skills with rollback/backup, `browser_open` /
  `browser_dom`, Pandoc document export, and the full-bleed image-slide path.
- **The desktop build uses the documented toolchain** — `beforeDevCommand` and
  `beforeBuildCommand` now call `npm`, matching the README and the installed
  environment instead of requiring `bun`.

### Trust and reliability pass

- **Workspace drift fixed** — the default office folder is marked when created,
  so repeated runs no longer receive `office-2`, `office-3`, … and lose their
  files.
- **Office nodes keep their office** — graph node threads are linked to the
  office, restoring goal/policy/roster/office-memory context and delegation.
- **Tool results are read through the right envelope** — file-change journal,
  live citations and tool screenshots work again.
- **Anthropic SSE fixed** — `event:` lines are metadata, not JSON; split UTF-8
  no longer corrupts streamed text.
- **Failed runs finalize** — failures persist a terminal state, clear the
  checkpoint, return the agent to idle and appear in usage.
- **Stop works** — one-to-one runs have a Stop control; offices can cancel the
  active turn and pause their queue.
- **Command execution hardened** — Docker and SSH use argv, validate hosts,
  mounts and privileged flags; sandboxed children no longer inherit provider
  keys; `/`, `$HOME` and the data root are refused as workspaces.
- **Secrets no longer cross IPC** — MCP credential values are write-only;
  custom-provider headers are not returned to the renderer; the database is
  owner-only on Unix.
- **MCP server is safe by default** — high-risk tools require
  `RAVENBOT_MCP_ALLOW_DANGEROUS=1`; file tools are confined to an MCP
  workspace.
- **Memory stops growing quadratically** — self-review only stores facts from
  the current run, caps their size, dedupes them, and stops demoting memories
  that are recalled often.
- **UI correctness** — office navigation has a real hub; onboarding checks keys
  before declaring “ready”; URL team import sends the fetched document; thread
  context no longer leaks drafts/HITL cards; settings links open externally;
  text is selectable; chat/role application a11y cleaned up.
- **Selection quote** — selecting a passage in a message now offers Reply,
  Quote, and Copy. Quote replies to the source and inserts a Markdown blockquote
  into the draft; cross-message and control-spanning selections are ignored.
- **Structured interaction blocks** — `ask_user` now supports `confirm`,
  `choice`, and `form` cards in addition to the original question. Blocks are
  validated in Rust before they are stored, dangerous actions require a hold,
  multi-select is deliberate (submit, not a click), required form fields are
  checked before the run resumes, and old rows fall back to the question card.
  The payload and kind persist across a reload and the answer returns to the
  model as structured JSON.
- **Fleet attention inbox** — a new Inbox destination aggregates pending
  approvals, pending questions, and queued or failed office messages across
  every agent and office. Rows open the owning thread or office, filters split
  the sources, the rail carries a live count, the store refreshes from stream
  events and a bounded poll, and thread focus opens the exact parked
  conversation rather than whichever thread happens to be newest.
- **Composer v2** — `@agent`, `#file`, and `$skill` pickers insert real mention
  tokens. An `@agent` mention dispatches the turn directly to that specialist;
  the lead still coordinates and synthesizes the final answer. The addressed
  agent is persisted through the office queue, so a mention sent while the
  office is busy still reaches the right person. Drafts are keyed per thread and
  per office, debounced to local storage, restored on switch, and put back after
  a failed send. Office queue rows can be reordered with persisted positions or
  promoted with Run next; true mid-turn steer remains future work.
- **Turn anatomy** — the turn trace now lists model, tool, memory, delegation,
  and context spans as steps before its proportional waterfall. A compact office
  task card above the composer shows live plan progress and opens a node's
  thread. An awaiting-replies card names each delegated agent, distinguishes
  working/replied/failed, and opens the child thread. Handoff rows in the feed
  can open the child thread as well.
- **Unified side panel** — Browser, Files, Inbox, and Artifacts share one
  right-hand panel with a tab bar. The active tab is remembered per
  agent/office, the browser stays mounted so an agent's browser request still
  opens it, and the files surface resolves the selected agent/office workspace.
  Artifacts opened from code blocks now live in the panel with a recent list
  instead of a second split view inside the chat. The rail browser button now
  opens this panel instead of a standalone dock.
- **Native browser tabs** — the browser dock now supports up to eight real
  tabs, each backed by its own native child webview. Switching tabs preserves
  history, scroll position, and form state; tabs share the current agent/office
  profile. Tabs can be opened, switched, and closed from the dock chrome;
  target=_blank windows open as new tabs. A profile switch closes the old tab
  set before creating the new profile's first tab. Tab order can be changed by
  dragging, and each profile remembers its tab URLs and active tab across app
  restarts in an owner-only `tabs.json` beside the profile data.
- **Save browser password to vault** — the secure-input card can now save the
  typed password into the local Vault without putting it in the transcript.
  The backend verifies that the live page origin matches the origin shown when
  the card opened, stores through the vault's keychain path, and updates an
  existing login for the same origin and username instead of creating a
  duplicate.
- **Secure browser input + cookie control** — the browser toolbar has a secure
  fill card for passwords and one-time codes. The value is written directly into
  the page through a user-initiated evaluator and is never sent to the model or
  stored. A **Refuse cookies** control clicks reject/only-necessary consent
  actions in the current page. These are user controls; automatic cookie
  refusal remains future work.
- **Local credential vault** — a new Vault tab stores logins, cards and private
  notes. List responses carry metadata only; a secret is fetched only on an
  explicit reveal and clears itself after 20 seconds. Secret payloads use the
  OS keychain when available with the owner-only SQLite fallback, and vault
  items can be deleted without leaving a keychain orphan.
- **Vault request cards** — `ask_user kind: "vault"` asks for a login without
  putting the secret in model context. The card lists metadata, compares the
  item's website to the browser page (including subdomains), fills username and
  password through a user-initiated page script, and returns only the item name,
  origin and result to the model. A mismatch or a missing browser field is
  refused.
- **Scheduled signed audit backups** — choose a folder and a daily or weekly
  cadence. The first backup is a full signed snapshot; later runs export only
  entries since the last success, keeping files bounded. The maintenance loop
  writes a due backup before automatic retention and skips pruning when a due
  backup fails, so entries are not deleted before they are backed up. The last
  attempt and failure reason are shown in the Audit panel.
- **Named browser profiles** — Settings → Browser profiles can create,
  rename, and delete named profiles with their own cookies, logins, and local
  storage. The browser dock selector can pin a named profile per agent/office
  scope or return to that scope's automatic profile. Deleting a profile removes
  its data directory and is refused while it is active.
- **Desktop attention notifications** — opt-in OS notifications for new
  approvals, questions, failed office queue items, and budget alerts. The inbox
  deduplicates by item state, suppresses notifications while the app is focused,
  and never puts prompts, secrets, or budget amounts in the notification body.
  A test button surfaces a missing notification daemon.
- **Email attention alerts** — new approvals, questions, failed office queue
  items, and budget alerts can also be delivered over SMTP. Settings →
  Notifications configures host, port, STARTTLS or loopback-only plain SMTP,
  optional credentials, and from/to addresses, with a test send. The password
  is keychain-backed and write-only. STARTTLS is required for remote hosts;
  plaintext SMTP is refused unless the host is loopback. Messages contain only
  the event category, state, and office/agent name — never prompt text,
  questions, secrets, or budget amounts.
- **Automatic cookie refusal (opt-in)** — Settings → Browser profiles adds a
  switch. After each page load the dock runs a conservative detector at 800 ms
  and 2.6 s that only clicks exact reject-style labels inside a short consent
  banner or dialog mentioning cookies, privacy, or tracking. Accept buttons are
  never clicked. It targets the specific tab, including background tabs, and
  disabling it cancels pending retries. The manual cookie button remains.
- **Planning and synthesis are billed** — office plan generation, blueprint
  design, and final synthesis now record their model usage against the bot's
  budget and the audit log, using the same catalog pricing as normal runs.
  They are no longer invisible free calls.
- **Tool pricing and spend attribution are real** — known paid tools now carry
  conservative provider-price estimates (Tavily search, DALL-E 3 image
  generation), applied only while the paid provider is configured; a user price
  still wins and also covers MCP/plugin tools. Runs record model and tool spend
  separately, utility calls get their own ledger, and Usage shows the three
  amounts with a paid-tool price audit. `analyze_image` now charges its vision
  tokens to the run instead of hiding them.
- **Deterministic agent eval suite** — `cargo test -p ravenbot-runtime --test
  agent_evals` exercises the real runtime with a scripted provider: tool loop,
  truncated stream, workflow group loop, and a denied high-risk approval. See
  `docs/EVALS.md`.
- **Private tool networks are opt-in** — unrestricted agents now block
  localhost, private LAN, and `.local` HTTP targets by default. Settings → Local
  has an explicit allow toggle; a narrowed agent can still be granted a private
  host directly through its network domains. The default is fail-closed.
- **macOS Seatbelt sandbox backend** — on macOS the runner now prefers
  `sandbox-exec` with a default-deny profile: system paths are readable, only
  workspace paths and the user temp directory are writable, and network is
  enabled only when policy allows. Windows now reports honestly that its
  sandbox backend is unavailable; commands remain fail-closed.
- **HTTP clients pin resolved addresses** — arbitrary HTTP tools and MCP HTTP
  transports resolve the host once, reject blocked/private rebinding answers,
  and build a client bound to that address. Redirects are still revalidated.
  This closes the DNS-rebinding window between validation and request.
- **Workflow canvas node types** — tasks now carry an explicit type:
  `trigger`, `agent`, `llm`, `approval`, or `output`.
  - Triggers complete immediately and feed their instruction forward.
  - Output nodes aggregate their upstream inputs without a model call.
  - LLM nodes make one tool-free completion and persist the answer.
  - Approval nodes create a real approval card; allowed continues, denied or
    timed out fails the node.
  - Agent nodes keep the full tool loop.
  Types are validated, persisted, and editable in the Plan editor.
- **Multi-task workflow loop groups** — tasks can share a group name and repeat
  as a unit. The terminal task carries the iteration cap and optional stop
  phrase; the group only publishes to external dependents after its final pass.
  A completed group resets its members and reuses their threads, so each pass
  sees the previous work. The Plan editor exposes group name, terminal marker,
  run count, and stop phrase.
- **Accessibility gate** — axe-core checks now run in the existing component
  tests for question cards, interaction blocks, workflow library, onboarding,
  runs/diagnostics, and chat message rows. Colour contrast stays disabled in
  jsdom and is covered by the token review.
- **Paid tool pricing is configurable and charged** — Settings → Budgets has a
  per-tool price table. A configured price is added to the run ledger, the bot
  budget, and the tool trace at the moment the tool runs. This is honest for
  APIs whose billing is per call rather than token-based.
- **The remediation plan is written down** — `docs/REMEDIATION-PLAN-2026.md`
  sequences the remaining platform and product boundaries, with acceptance
  criteria and explicit non-goals.
- **Per-agent network grants are enforced at the real URL** — a narrowed agent
  is now checked against its configured network domains on every HTTP-capable
  path: `http_request`, API tester requests/collections, browser navigation,
  MCP HTTP servers, web/Tavily/arXiv/YouTube search, and image generation. The existing Settings network-domain
  list is no longer only a tool-level declaration. `None`/empty capability lists
  keep their established unrestricted or deny-all semantics.
- **Reply quotes resolve across pages** — when a reply points at a message older
  than the loaded cursor page, the transcript fetches that target by id and
  caches it. The quote shows the real author and text instead of “unavailable”.
- **Off-screen transcript rows skip layout and paint** — message entries use
  `content-visibility: auto` with an intrinsic height, complementing cursor
  pagination and the bounded render window.
- **One corrupt message row no longer breaks a transcript** — rows are hydrated
  independently. Invalid content becomes a visible placeholder explaining that
  the message could not be decoded; invalid attachments are dropped; both page
  loads and full-thread loads continue. The original row stays in the database.
- **Transcripts load in cursor-paginated pages** — opening a thread now reads
  the newest 50 messages instead of its entire history. “Load earlier” fetches
  the previous page by message cursor, deduplicates it, and preserves scroll
  position. Search still covers the whole thread by loading the full history on
  demand. Office and 1:1 transcripts share the same paging rules.
- **MCP URLs and redirects use the egress guard** — custom MCP server URLs are
  validated before saving (http/https only, no metadata or link-local targets),
  and model/tool/MCP HTTP clients reject redirects into metadata or link-local
  addresses. The shared guard now lives in `ravenbot-core` so every layer uses
  the same rules.
- **Start RAVENBOT at login** — Settings → Local now has an autostart toggle
  backed by the OS login mechanism. It launches the app at sign-in; combined
  with close-to-tray, the fleet can keep running without a visible window.
- **First run can start with local Ollama** — onboarding probes the configured
  Ollama endpoint, offers the first tool-capable installed model, and proceeds
  without an API key. A saved architect is only treated as ready when its
  provider has a key or is local. Cloud/custom setup remains unchanged.
- **Truncated model streams are no longer reported as complete** — providers
  now require their terminal signal (`[DONE]`, `finish_reason`, Anthropic's
  `message_stop`, or Ollama's `done`). A stream that ends early preserves the
  partial text as a `PartialSuccess` answer with an explicit incomplete marker,
  and the graph executor treats that node as not finished. No retry runs after
  text was already shown.
- **Closing the window keeps the fleet alive** — RAVENBOT now installs a tray
  icon and hides the window on close. Offices, routines, scheduled workflows,
  the webhook receiver, and the queue worker keep running until the user chooses
  **Quit RAVENBOT** from the tray. The tray also offers Open, Pause all agents,
  and Resume all agents. `RAVENBOT_CLOSE_TO_TRAY=0` restores close-to-quit. If
  the tray cannot be created, close still quits so the app is never unreachable.
- **Unattended runs use an approval ladder** — office graph nodes, routines,
  and channel tasks now run under **Review**: read-only/low-risk tools proceed,
  high-risk tools park on an approval card even when the bot itself is set to
  `Full`. Delegated children inherit the same policy. New office agents default
  to `Auto` instead of being forced to `Full`, and adding an agent to an office
  no longer rewrites its interactive mode. Headless CLI runs still use the
  explicit allow-all path.
- **HTTP tools refuse metadata and non-HTTP targets** — `http_request` and
  browser navigation validate URLs before any request: only http(s), no embedded
  credentials, no cloud-metadata hostnames, and no link-local/unspecified
  addresses. Public URLs and local-first services such as localhost and private
  LAN APIs still work.
- **Agent commands fail closed without a sandbox** — if bubblewrap is absent
  or the Docker backend is not wired, `shell_exec`, package-manager, and task
  commands now refuse to run instead of silently executing on the host. The Host
  tier remains the explicit per-agent opt-in, with `RAVENBOT_ALLOW_UNSANDBOXED=1`
  as a deliberate global override. App-owned renderers (Pandoc, rsvg-convert)
  still fall back to limits-only so document and slide export keep working.
- **`AskUser` budgets pause for a decision** — when an `AskUser` budget is
  exhausted, the run parks on a confirmation card. Choosing Continue resumes
  the run; Stop, timeout, cancellation, or the kill switch denies it. Headless
  runs fail closed instead of waiting for a user who is not there. Planning and
  synthesis usage remains included.
- **Re-staffing shows a diff and confirms removals** — a preview reports which
  roles create agents, which reuse existing agents, and which current members
  the new blueprint would leave behind. Obsolete members are removed from the
  office only when the user confirms; cancel keeps them. Agents themselves are
  never deleted.
- **Re-staffing an office no longer duplicates agents** — role matching used to
  exclude everyone already in the office, so applying a blueprint twice created
  a second copy of every member. Role assignments are now planned against the
  whole fleet with the office roster included; one agent still cannot fill two
  roles in the same run.
- **Budget behavior is real** — a `Warn` budget now continues past its limit
  with a visible warning instead of hard-stopping; `HardStop` still refuses.
  Settings exposes the behavior selector, and the budget panel warns when the
  threshold is reached.
- **Budget edits update one row** — saving a budget used to insert a new row and
  leave the oldest one active, so limit and behavior changes appeared to do
  nothing. Saves now reuse the bot's row, a migration keeps the newest budget
  per bot and adds a unique index, and a regression test asserts one row with
  the new settings applied.
- **Budget periods are real** — hourly, daily, weekly and monthly budgets now
  accumulate in a dated window and reset when the window rolls over, so a
  monthly ceiling no longer behaves like a lifetime total. Lifetime token and
  cost totals are kept separately for the Usage screen and office baselines;
  resetting a budget clears the current window without erasing history.
- **Startup reconciliation is complete** — interrupted runs and workflow runs
  are closed, pending approvals and questions are expired instead of blocking
  the inbox forever, and queue rows left mid-turn are marked failed. Paused runs
  keep their checkpoint and are now resumable from the Runs panel after a
  restart. Stale office-active markers and agent statuses are cleared.
- **Providers and web tools can no longer hang forever** — every model
  provider and network-backed tool now uses a shared HTTP client with a connect
  timeout and an idle read timeout. Long streams keep running while they produce
  data, but a black-holed socket fails instead of pinning the agent. Both limits
  are environment-tunable, and regression tests use a server that accepts and
  never replies.
- **Workflow template library** — the Plan editor offers four built-in,
  editable starting points: Release audit, Research brief, Content pipeline,
  and Bug triage. Each fills a real DAG with role-aware assignees, dependencies,
  typed JSON outputs, retries, and loops; nothing is hidden or locked.
- **Costs use real model pricing** — model rounds are billed from the built-in
  model catalog (input/output per-million prices) instead of a flat $3/$15.
  Local/Ollama models are free; unknown cloud models keep a conservative
  estimate. The invented $0.001 per tool call was removed; paid third-party
  tool APIs are not priced separately yet.
- **Bounded workflow loops** — a task can run up to 10 times. Each iteration
  reuses the same node thread, sees the previous answer, and either runs every
  time or stops early when the answer contains (or no longer contains) a
  phrase. Only the final answer reaches dependents. The Plan editor exposes the
  run count and stop phrase per task.
- **Workflow runs report real failures** — `execute_graph` now returns a
  `status` and the failed/skipped task list. Plan dispatch and the scheduled/
  triggered runner record failed workflow runs as failed instead of completed.
  Branch-condition skips are treated as intentional, not failures.
- **A node can no longer hang the office** — every early error after a node
  starts now terminalizes it and skips its dependents; a panicked node task is
  failed by the batch watchdog instead of staying `Running` forever.
- **Office queue failures are honest** — an office turn that ends with failed
  or dependency-blocked nodes marks its queue item failed, and an executor
  error is no longer returned as a successful empty turn.
- **Typed workflow outputs** — any workflow task can carry an optional JSON
  Schema. The schema is appended to that node's instruction, the final answer is
  extracted from plain, fenced, or prose-wrapped JSON, validated before the node
  is marked done, and stored on the blackboard as canonical JSON for dependents.
  Invalid output fails the task and uses its configured retries; a retry now
  also restores dependents that were skipped only because of that failure.
  Schemas are validated when a workflow is saved and can be edited per task in
  the Plan canvas.
- **Transcript scroll rail** — office and 1:1 chats show a right-edge rail with
  a viewport band and ticks for the visible user messages. Clicking a tick
  scrolls that message into view. The rail measures the real scroll container
  and updates on scroll/resize; it hides when the transcript has no measured
  height.
- **Per-profile cookie policy** — named browser profiles can inherit the
  global automatic cookie-refusal setting or override it with always/never.
  Automatic per-agent/office profiles use the global default. The dock resolves
  the effective policy for the active profile and reacts to profile and policy
  changes live.
- **UI foundation gate** — `npm run check` now runs a Svelte-aware gate that
  fails on raw hex colors in styles or inline styles, `!important`, off-scale
  text, and growth in native `<select>` use outside `components/ui`. Existing
  native selects are recorded in a baseline; `--update` refreshes only that
  baseline. CI runs the gate with the frontend checks. Three inline color
  literals in the office creator were replaced with theme tokens.
- **Workflow webhook triggers** — office-scoped workflows can be started by a
  loopback POST to `/workflows/<token>` or a Bearer token. The URL is shown once
  when enabled or rotated; SQLite stores only its SHA-256. The receiver routes
  the trigger to the desktop runner, which executes the latest saved version
  through the existing graph executor, queues triggers one at a time, and
  records run history. Triggers are off by default and rotate invalidates the
  previous URL.
- **Per-task workflow retries** — each Plan task can retry 0–3 times after a
  non-permanent failure. Retries run before the skip cascade, so a transient
  failure can still rescue dependents. Cancellation, kill switch, budget, tool
  denial, and configuration errors are never retried. Retry attempts are
  reflected on the office board and are capped at three extra attempts.
- **Transcript day markers and pending-send state** — office and 1:1 chats
  insert local-day dividers (Today, Yesterday, then a date) and show a
  "Sending…" state on optimistic user rows until the backend accepts the
  message and replaces it with the persisted row. Queued messages still move to
  the queue panel.
- **Explicit workflow input ports** — each workflow task can choose which
  upstream dependencies supply context. All dependencies are injected by
  default; unchecking one withholds that result from the node's instruction.
  The selection is preserved in saved workflows, version snapshots, scheduled
  runs, and the Plan editor, and graph validation rejects inputs that are not
  direct dependencies.
- **Conditional workflow branches** — each workflow task can run only when a
  chosen upstream dependency's output contains or does not contain a phrase
  (case-insensitive). Conditions are validated when saving and dispatching,
  edited in the Plan canvas, preserved in versions and scheduled runs, and
  skipped nodes cascade so dependents do not stall. The office board marks
  condition-skipped nodes as skipped.
- **Memory summarization** — select 2–20 agent or office memories, ask the
  configured model to merge them, edit the returned summary, then replace the
  originals in one transaction. The model sees only the selected entries. The
  replacement keeps the highest source importance (and the highest-importance
  source category for office memory), is audited as one creation plus one
  deletion per source, and changes nothing until the user confirms the preview.
- **Memory provenance** — every agent and office memory now records where it
  came from (`user`, `agent`, `self-review`, `import`, or `unknown` for legacy
  rows), the source thread and run when known, and when it was last updated.
  The memory panels show the source label and expose the source thread id as a
  tooltip. Existing rows migrate as legacy memories; bundle imports are marked
  as imported and do not inherit another installation's thread or run ids.
- **Memory duplicate detection** — the agent and office memory panels can scan
  for near-identical entries, preview the group that will be kept, and remove
  the rest after a second confirmation. Similarity uses the stored embeddings,
  the kept entry is chosen by importance, recall count, then age, and each
  removal is audited as a normal memory deletion. The scan is scoped to the
  selected agent or office and never crosses that boundary.
- **Memory retrieval benchmark** — Settings → Memory can run a fixed
  50-fact, 10-query evaluation against the saved embedding backend. It reports
  Recall@5, mean reciprocal rank, vector dimension, and each query's rank or
  miss. The benchmark reads no stored memories and sends only its fixed
  sentences to a remote endpoint.
- **Semantic memory embeddings (opt-in)** — Settings → Memory can switch agent
  and office memory from the offline 128-dimension hash to an Ollama model or an
  OpenAI-compatible endpoint. The endpoint is tested before use, every stored
  memory is re-embedded in bounded batches, and the runtime swaps the provider
  only after the reindex succeeds. Ollama keeps memory text on the machine;
  remote endpoints require HTTPS, are called without redirects, and are shown
  behind an explicit warning. The API key is keychain-backed and never returned
  to the renderer. Config changes emit a `MemoryEmbeddingsUpdated` audit event
  with the provider, model, endpoint and reindexed count, never memory text.
- **Editable agent and office memory** — Settings for each agent now include a
  Memory tab, and the office brain can be managed from the same panel. Facts
  can be added, edited, searched, and deleted; edits re-embed the new text and
  are scoped to the owning agent or office. Manual changes emit a
  `MemoryChanged` audit event with the action and category, never the memory
  content, and embeddings no longer cross the IPC boundary.
- **Remote attention webhooks** — new approvals, questions, failed office
  queue items, and budget alerts can also be POSTed to an HTTPS webhook. URLs
  are keychain-backed, redirects are refused, and the payload is deliberately
  generic: event kind, state, office/agent names, and fixed text only. Prompt
  content, questions, secrets, and budget amounts never leave the machine.
  Settings can save, test, and remove the webhook without ever reading it back.
- **Scheduled workflow runs** — office-scoped workflows can run daily or
  weekly while RAVENBOT is open. A background claimer advances the schedule
  atomically, creates a running run row, reuses the existing graph executor,
  and records completion or failure. Schedules always use the latest saved
  version and show next/last run plus the last error.
- **One-click workflow runs and live run state** — saved templates can be
  dispatched directly from the library after a confirmation. Dispatch opens a
  `running` workflow-run row, then marks it completed or failed with the thread
  and error detail. Startup reconciliation marks interrupted runs failed.
- **Workflow version and run history** — every workflow save captures an
  immutable version snapshot (newest 50 kept), and every dispatched template
  records its version, goal, thread, completion state and error. The plan
  library can load a historical version back onto the DAG canvas and list the
  last runs for each workflow.
- **Reusable office workflows** — the Plan DAG can be saved as a named
  per-office template with its goal, tasks, assigned agents, and dependency
  edges. Templates can be loaded back onto the canvas, edited, deleted, and
  dispatched through the existing graph executor. Missing agents are remapped
  with a visible warning rather than silently dropped.
- **Configurable office budget alerts** — each office can enable or disable
  budget alerts and set its own warning and critical percentages. The defaults
  remain 90% and 100%. The attention inbox uses those thresholds, and invalid
  values (outside 1–100 or critical below warning) are rejected before saving.
- **Office budget alerts** — the attention inbox now includes a computed alert
  when an office has spent at least 90% of its configured budget, escalating to
  a critical alert at 100%. Budget alerts show spend, budget, and percentage,
  open the owning office, and are deduplicated by office. A failed usage query
  cannot hide approvals, questions, or queued work.
- **Per-office cost attribution** — Usage now switches between by-agent and
  by-office breakdowns. A run inherits its office from the office's own
  conversation thread or from a delegated office thread; direct chats stay
  visible as their own group so office totals reconcile with the fleet total.
  Office rows show spend against the office's configured budget. A thread
  linked to more than one office is attributed once, not counted in both.
- **Vault import** — Bitwarden JSON, Bitwarden CSV, 1Password CSV and generic
  CSV exports can be previewed and imported. The backend reads the file and
  returns metadata plus duplicate counts only; secrets never cross IPC and are
  saved through the same keychain path as manual items. Duplicates are skipped
  rather than overwritten. Files are capped at 8 MB and 2,000 items; encrypted
  Bitwarden exports and 1PUX archives are refused with explicit guidance.
- **Per-agent browser profiles** — the browser dock now uses a separate
  persistent data directory per selected agent or office. Switching scope
  destroys and recreates the native webview so cookies, logins and local
  storage do not cross profiles. Profile directories are owner-only on Unix.
  Shared storage remains available when no agent/office is selected.
- **Global audit log** — Settings → Diagnostics now has a fleet-wide audit
  trail with agent, event-kind, time-range and text filters, stable pagination,
  and thread hand-off. Model rounds and user approval decisions are audited
  alongside tool calls, so spend, permission outcomes and tool activity appear
  in one ledger. Tool arguments are never projected to
  the renderer (they can contain secrets); the audit store still records the
  call. Migration 040 adds an indexed `event_kind` column, backfills it from
  existing event JSON, and normalises legacy timestamps to UTC.
- **Signed audit exports and retention** — the audit view can write the current
  filtered trail to a JSON document signed with the persisted Ed25519 identity,
  then verify a chosen export in place. Verification checks the payload
  signature and reports whether the signer is this installation or another
  one. Retention deletes entries older than an explicitly confirmed 30–730-day
  window. Exports are capped at 50,000 entries and fail rather than silently
  truncating. Retention can now be automatic: an opt-in policy (0 = forever)
  runs at startup and every six hours, while saving a finite policy prunes
  immediately and reports what was removed.
- **OS-keychain secret storage** — provider API keys, MCP environment values,
  plugin credentials, and the bundle signing seed now use the platform keychain
  (macOS Keychain, Windows Credential Manager, or Linux Secret Service) when
  available. SQLite stores only a marker; legacy plaintext rows migrate on first
  read. On hosts without a keychain the value falls back to the owner-only
  database, and `RAVENBOT_DISABLE_KEYCHAIN=1` forces that path. Custom-provider
  headers and custom MCP server headers now use the same keychain path, with
  their legacy JSON migrated on read. Routine webhook bearer tokens now use the
  keychain too; SQLite stores only a SHA-256 hash for routing, and legacy tokens
  are migrated and hashed before the webhook listener starts. The bundle
  signer's transient constructor now generates a random seed instead of reusing
  a constant.
- Full analysis and roadmap: `docs/COMPETITIVE-AUDIT-2026.md`; source-level
  OpenBot teardown: `docs/OPENBOT-TEARDOWN-2026.md`.

### One brain, from the provider

- **CLI agent engines removed** — the `ravenbot-engines` crate drove a locally-installed
  agent CLI (Claude Code, Codex, generic ACP) and normalized its protocol into one event
  stream. It is gone, along with `BotConfig.engine` / `engine_model`, the `list_engines` /
  `set_bot_engine` IPC commands, the BotSettings execution-engine picker, the engine
  strings in all six locales, and the engine branch of `Runtime::execute_run`. The drivers
  broke on version-sensitive CLI flags — an office run went 0/7 because the installed
  `codex` no longer accepts `--ask-for-approval` — and they carried per-CLI auth state and
  protocol drift in exchange for models the provider layer already calls directly, minus
  the tool loop, sandbox tier, permission model and streaming the app owns.
  **API providers are now the single source of a bot's brain.**

### Honest failures

- **Fixed a deadlock that surfaced as `database is locked`** — `Runtime::new` re-created
  the plugin and MCP tables from a spawned task "just in case", racing the caller's first
  write for the SQLite lock. On a current-thread runtime (which is what every
  `#[tokio::test]` gives you) the caller's busy-wait blocks the only thread, so the task
  holding the lock can never be polled to completion and the 5 s `busy_timeout` expires
  instead of waiting. Migrations 003 and 006 already own those tables; the redundant DDL
  is gone, and a test asserts the ownership so it cannot move back.

### The office board, flattened

- **Header** — the roster was drawn twice (avatars in the bar, then name chips in a strip
  below); the avatars are now the roster, and they open the same manage sheet the chips
  did. Four equally-weighted bordered buttons became ghost controls, the duplicate
  settings affordance is down to one, the lane no longer wears success-green while idle,
  and the cost pill is neutral and hidden until it has a number.
- **Board** — columns are headings over a stack rather than boxes inside boxes, a card is
  a single tonal surface with state on the icon (tinted only for running and failed), a
  progress hairline replaces reading "0/7", and the column scroller is visible so clipped
  cards are not silently hidden.

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
