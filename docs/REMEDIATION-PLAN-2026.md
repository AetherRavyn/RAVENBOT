# RAVENBOT remediation plan (2026)

This is the execution plan for the remaining platform and product boundaries.
It is deliberately ordered by **risk reduction first**, then product surface.

Guiding rules:

1. No fake completion. A feature is done only when it has an acceptance test or a
   documented, reproducible manual check.
2. Platform code is not claimed as verified without a real macOS/Windows run.
3. Paid or destructive actions are measured, attributable and auditable.
4. Local-first behavior remains the default; hardening must not silently turn a
   local integration into a remote dependency.

## Status

| Workstream | State | Next acceptance criterion |
|---|---|---|
| Transcript pagination, reply quotes, malformed rows, off-screen rendering | **Done** | Paged load + reply target fetch + placeholder + `content-visibility`; tested |
| Per-agent network grants | **Done** | Actual URL checked against `Permission::Network` on HTTP/browser/MCP/search/image tools |
| Approval ladder for unattended runs | **Done** | Review blocks high-risk tools; delegated runs inherit |
| Budget periods, `Warn`, `AskUser` | **Done** | Window rollover, warn continuation, approval card |
| Model + utility + paid-tool accounting | **Done** | Catalog pricing, plan/synthesis usage, configurable per-call tool prices |
| Startup reconciliation and tray/autostart | **Done** | Orphans failed, paused preserved, tray + login autostart |
| Multi-task workflow loops / richer canvas nodes | **Done** | Trigger/agent/LLM/approval/output nodes persisted, executed, and tested |
| Linux Docker / macOS Seatbelt / Windows sandbox | **Docker + Seatbelt implemented; Windows fail-closed** | Validate Seatbelt on a real Mac; implement Windows AppContainer/Job Object |
| DNS pinning / default private-network policy | **Done** | Private tool networks are opt-in; resolved addresses are pinned |
| systemd/launchd/Windows task | **Headless daemon implemented; Linux install/restart/stop verified** | Verify logout, macOS LaunchAgent and Windows logon task on real machines |
| Agent-task eval suite / axe gate | **Done** | Settings, office settings, browser panes and the attention island covered |
| Remote/mobile + RBAC | **Done foundation** | Authenticated API, one-time pairing, roles, audit, mobile page, optional self-signed HTTPS |
| Local STT/TTS | **Done** | Whisper/TTS use the saved provider key with an env fallback; per-call temp isolation fixed |
| Chat apps | **All five native** | Telegram, Slack, Discord, WhatsApp and Microsoft Teams |
| Dynamic island | **Implemented** | Always-on-top attention window; manual Wayland/macOS check pending |
| Multi-user desktop accounts | **Done** | Launcher, Argon2id sessions, isolated roots and keychain namespaces, shared-workspace roles, concurrent windows |
| Native chat-app adapters | **Done** | Telegram, Slack, Discord, WhatsApp and Teams native adapters with cards and attachments |

## Phase 1 — Workflow engine completeness

Scope:

- Multi-task loop groups: **done**. A bounded group repeats as a unit, with a
  terminal task carrying the cap and optional stop phrase.
- Canvas node types: **done**. Trigger, agent, LLM, approval, and output are
  persisted, validated, and rendered; each has its own execution semantics.
- Existing single-task loops and typed outputs remain.

Acceptance:

- [x] A group runs 1–10 times, stops early, and only publishes after the final pass.
- [x] Dependents never see a partial iteration (readiness checks the group first).
- [x] Validation rejects duplicate/missing terminals and out-of-range caps.
- [x] Rust graph/executor tests and Plan editor type checks pass.
- [x] Canvas node types persisted and rendered, with executor tests for
  trigger/output, LLM, and approval nodes.

## Phase 2 — Platform sandboxing

Scope:

- Linux: Docker tier now runs a real container with a read-only root, workspace
  mounts, dropped capabilities, resource limits, and `--network none` when the
  policy blocks egress. **Implemented and live-tested with Docker.**
- macOS: Seatbelt (`sandbox-exec`) profile with read-only system paths,
  workspace-only writes, and network only when policy allows. **Implemented**;
  profile tests run in CI, execution awaits a real Mac.
- Windows: fail closed with a clear reason until a real backend exists; then
  AppContainer/Job Object implementation. **Fail-closed reporting done.**
- Report the effective backend in `SandboxReport`.

Acceptance:

- [x] Profile denies by default, opens only workspace writes, and includes
  network only when enabled.
- [x] Missing backend fails closed unless Host tier or explicit opt-in.
- [x] Windows reports that its sandbox backend is unavailable.
- [x] Linux Docker tier builds a locked-down command and runs a real container
  with workspace mounts and blocked network.
- [ ] Execute the macOS profile on a real Mac and confirm a secret directory
  under `$HOME` is absent.
- [ ] Implement and validate Windows AppContainer/Job Object.

## Phase 3 — Egress and pricing hardening

Scope:

- Resolve hostnames once, pin the verified IP, and use it for the request.
  **Done** for arbitrary HTTP tools and MCP HTTP transports.
- Add a default private-network policy separate from per-agent grants. **Done**:
  localhost/private/LAN tool targets are blocked unless Settings → Local allows
  them or the agent has an explicit domain grant.
- Provider-price adapters for known paid tools; user-configured per-call prices
  still win and also cover MCP/plugin tools. **Done** for Tavily search and image
  generation, with a result-level fallback marker so keyless runs are free.
- Surface unpriced paid tools in Usage. **Done**: the Usage breakdown lists each
  active paid tool with its effective price and whether it is a user price or an
  estimate.

Acceptance:

- [x] Resolve-once pinning is covered by a client test; the address is bound
  before the first request, so a later resolver change cannot redirect it.
- [x] Metadata/link-local remains denied through redirects and MCP.
- [x] Usage shows model, utility, and tool spend separately.

## Phase 4 — Service lifecycle

Scope:

- Linux systemd user unit, macOS LaunchAgent and Windows Task Scheduler logon
  task, installed from Settings. **Implemented** in
  `src-tauri/src/service.rs`; see `docs/SERVICE.md`.
- The unit/task runs `ravenbot daemon`, a headless fleet owner with no window or
  display: database, runtime, scheduler, routine **and workflow** webhooks,
  scheduled workflows, office loops, audit maintenance, provider-credential
  refresh, attention push and remote access.
- Exclusive `~/RAVENBOT/fleet.lock`; a desktop launch can stop the service and
  take over, and a second daemon exits instead of duplicating work.
- Linux is `WantedBy=default.target`; with `loginctl enable-linger` it survives
  logout. macOS remains a per-user LaunchAgent and stops with the session.
- Keep tray/autostart as the simple desktop path; installing the service
  disables the plain login item.

Acceptance:

- [x] Unit/plist generation, quoting/escaping, idempotent writes, unsupported
  platforms reported honestly.
- [x] Settings install/uninstall/handoff UI and commands.
- [x] Restart-on-failure is explicit; a clean exit stays quit.
- [x] Headless daemon runs with no `DISPLAY`/`WAYLAND_DISPLAY` and exits cleanly
  on SIGTERM.
- [x] Fleet lock denies a second owner and releases after a crash.
- [x] Generated unit passes `systemd-analyze verify`; the LaunchAgent parses as
  a plist; the Windows task XML has logon/limited-user/restart settings.
- [x] Linux unit applies `NoNewPrivileges`, `PrivateTmp`, `ProtectSystem=full`,
  protected kernel interfaces, `RestrictRealtime`, `LockPersonality`, and
  `LimitCORE=0`; CI runs `systemd-analyze verify`.
- [x] CI runs the live Docker sandbox test with `alpine:3.20`.
- [x] Live Linux install: start, crash restart after `kill -9`, clean stop, and
  uninstall were exercised with a temporary `RAVENBOT_HOME`.
- [ ] Logout/login on a real Linux desktop (would end this agent session).
- [ ] Live LaunchAgent check on macOS and logon-task check on Windows.

On Linux, lingering plus the headless daemon is the supported after-logout path.
macOS after-logout operation would need a LaunchDaemon and is not claimed.

## Phase 5 — Quality gates

Scope:

- Deterministic agent-task eval suite: **done** for tool loop, denied
  high-risk approval, truncated stream, and workflow group loop. See
  `docs/EVALS.md`.
- Axe accessibility gate for core primitives and chat cards. **Done** for
  question cards, interaction blocks, workflow library, onboarding, runs, and
  chat rows.
- Axe coverage now includes Settings (providers, local/background service,
  remote access), NotificationSettings, OfficeSettings, BrowserDock,
  WorkspaceBrowser and the attention island.

Acceptance:

- [x] Evals run in CI with fixed seeds and no network.
- [x] Axe violations fail CI on the gated components.
- [x] Settings, office settings, browser panes and the attention island are
  gated.
- [x] Invalid tablist/tree roles replaced with honest group semantics; the
  Settings dialog has an accessible name.

## Phase 6 — Product surfaces

Status:

1. **Remote/mobile + RBAC** — **done foundation**. An opt-in axum listener,
   one-time pairing codes, SHA-256 token storage, viewer/operator/admin roles,
   mutation audit, a mobile web client, optional self-signed HTTPS with a
   fingerprint, and Settings management. See `docs/REMOTE-ACCESS.md`.
2. **Local STT/TTS** — already implemented through `ravenbot-vision` and the
   voice skills. The local transcription scratch directory is now per-call, so
   concurrent recordings cannot overwrite each other.
3. **Chat apps** — **all five are native**. Telegram uses bot-token long
   polling, Slack uses Socket Mode, Discord uses the Gateway WebSocket,
   WhatsApp uses a signed Cloud API webhook and Microsoft Teams uses the Bot
   Framework. All five have one-time pairing, thread routing, reply delivery,
   lifecycle commands and a Review notice, and run in the desktop or daemon.
   `docs/CHAT-APPS.md` covers all five.
4. **Dynamic island** — implemented as a compact always-on-top Tauri window
   that polls pending approvals/questions and running runs, with Open and Hide
   actions.

5. **Headless workflows and attention push** — done. Scheduled and webhook
   workflows execute in the daemon; new approvals/questions trigger an OS
   notification when a session exists, plus the configured HTTPS webhook and
   SMTP alert. Provider credentials refresh every two minutes so a keychain
   that unlocks after login is picked up.

Still separate, not faked:

- publicly trusted TLS/ACME (self-signed HTTPS is implemented; a reverse proxy
  remains the way to get a real certificate);
- multi-user desktop profiles (the current RBAC scopes remote devices only);
- real-machine validation of the macOS LaunchAgent and Windows logon task.

## Phase 7 — Accounts and native chat apps

Multi-user accounts now include isolated local accounts with a login launcher,
Argon2id passwords, expiring sessions, per-account roots and keychain
namespaces, owner migration, login throttling, rename/delete, in-launcher
creation, shared workspaces, cross-account grants and a server-side
owner/operator/viewer IPC gate. Per-run actor attribution, per-account daemon
units, concurrent desktop workspace windows with per-workspace ports and rich inbound and outbound chat attachments are implemented across all five
adapters; Teams outbound uses the Bot Framework file-consent flow. Interactive approval/question cards
are implemented across the five native adapters, with explicit text commands as
a fallback. Phase 7.2's provider coverage is complete and per-adapter inbound
throttling is implemented.

### 7.1 Multi-user accounts and roles

- Server-side accounts with Argon2id password hashing, short-lived sessions,
  refresh/revocation, and login throttling. A PIN over one shared database is
  not multi-user.
- Per-account data roots and keychain namespaces; no shared SQLite file.
- Roles: owner, operator, viewer. Every mutating IPC/HTTP action checks the role
  on the server side, not in the UI.
- Per-account audit attribution for runs, approvals, settings, token issuance
  and revocation.
- Vault and provider credentials isolated per account; the owner can grant or
  revoke access without revealing a secret.
- Remote/mobile tokens bound to an account and subject to the same role matrix.
- Migration: the existing single-user database is adopted as the owner account;
  new accounts get new roots and no history unless an explicit export/import is
  performed.
- Acceptance: login/logout, role denial matrix, session revocation, data
  isolation, audit attribution, backup/restore, throttling, and a remote token
  that cannot exceed its account's role.

### 7.2 Native chat-app adapters

Provider-specific adapters, not a generic webhook that ignores signatures:

- **Telegram** — **implemented**: bot token, long polling (no public port),
  `/start <code>` pairing, allowlisted chat ids, one thread per chat, `/new`,
  `/stop`, `/status`, `/unlink`, and an approval notice.
- **Slack** — **implemented**: Socket Mode (no public port), app and bot
  tokens, one-time channel pairing, DM/mention gating, `pair`, `new`, `stop`,
  `status`, `unlink`, thread replies and an approval notice.
- **Discord** — **implemented**: outbound Gateway WebSocket (no public
  interactions endpoint), bot token, one-time channel pairing, DM/mention
  gating, `pair`, `new`, `stop`, `status`, `unlink`, mention-safe replies,
  duplicate-message idempotency and an approval notice.
- **Microsoft Teams** — **implemented**: Bot Framework RS256 OpenID key
  validation, issuer/audience/`serviceurl` claim checks, mandatory tenant
  allowlist, personal/mention gating, `pair`, `new`, `stop`, `status`,
  `unlink`, connector replies with a cached token, duplicate-activity
  idempotency, an approval notice, inbound attachments, and outbound files
  through consent cards and chunked uploads. Requires a trusted HTTPS reverse
  proxy or tunnel in front of the loopback listener, and `supportsFiles: true`
  in the Teams app manifest.
- **WhatsApp** — **implemented**: Meta webhook HMAC verification, verify-token
  challenge, phone-number allowlist, `pair`/`start`, `new`, `stop`, `status`,
  `unlink`, reply delivery, duplicate-message idempotency and an approval
  notice. Requires a trusted HTTPS reverse proxy or tunnel in front of the
  loopback listener.
- All five implement duplicate idempotency, reply routing, inbound and outbound
  attachments, native approval cards, per-conversation throttling and the
  existing capability/approval/budget gates. Approvals route to the desktop or
  paired remote page as well.
- Tokens live in the OS keychain. An inbound message is untrusted input; it does
  not get a special tool grant or bypass approvals.
- Acceptance: a valid inbound message creates a run and returns the final
  answer; duplicate delivery is ignored; an invalid signature is rejected; an
  unpaired chat is ignored; a high-risk tool still parks for Review; tests run
  against a mock provider API.

## Explicit non-goal

A hand-written measured DOM virtualizer is not planned. Transcripts are bounded
by cursor pagination (50), a 50/30 render window, and `content-visibility` for
off-screen rows. A custom virtualizer would add scroll-anchoring and dynamic
height risk without improving the bounded-row case.
