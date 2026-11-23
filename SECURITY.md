# Security Policy

## Reporting vulnerabilities

Please use GitHub's **private vulnerability reporting** on
<https://github.com/AetherRavyn/RAVENBOT/security/advisories/new>. Do not open a
public issue for a vulnerability.

Include a description, reproduction steps, the affected version/commit, and
the impact you believe it has. We aim to acknowledge reports within 48 hours
and to publish a fix or mitigation as soon as the report can be verified.

There is no separate security email address. A previous version of this file
listed `security@ravenbot.local`, which is not a real mailbox.

## What is implemented today

This section describes the code in this repository, not the roadmap.

### Secrets

- Provider API keys, MCP environment values, plugin credentials, custom-provider
  headers, custom MCP server headers, and the bundle signing seed are written to
  the OS keychain when one is available: macOS Keychain, Windows Credential
  Manager, or the Linux Secret Service. The database stores only a keychain
  marker. Legacy plaintext rows are migrated on their first read.
- If the platform keychain is unavailable (for example a headless Linux host
  without a Secret Service), the value stays in the local SQLite database as a
  fallback. The database and data directory are owner-only (`0600`/`0700`) on
  Unix. `RAVENBOT_DISABLE_KEYCHAIN=1` forces this fallback.
- The same keychain path has an owner-only SQLite fallback when the platform
  store is unavailable.
- Routine webhook bearer tokens use the same keychain path. SQLite stores only
  a SHA-256 hash for routing incoming requests, so the token itself is never a
  query key in the database. Legacy tokens are migrated and hashed before the
  webhook listener starts.
- MCP credential **values do not cross the Tauri IPC boundary**: the connector
  UI receives key names with blank values, and saving a blank field keeps the
  existing value.
- Custom-provider `headers_json` is not returned to the renderer.
- Vault items keep their metadata in SQLite and their secret JSON in the OS
  keychain when available. List responses never include secrets; reveal is an
  explicit user action.
- An agent can ask for a login with `ask_user kind: "vault"`. The user chooses
  an item on the card, and the password is written directly into the current
  browser page. The model receives only the item name, origin and fill result.
  The item's website must match the page host (including subdomains); a
  mismatch is refused.
- Vault imports are read on the backend. The preview returns metadata and
  duplicate counts only, never a secret; confirmed imports use the same
  keychain-backed write path as manual items. Duplicates are skipped rather
  than overwritten. Files are capped at 8 MB and 2,000 entries; encrypted
  Bitwarden JSON and 1Password 1PUX archives are refused with guidance.
- **Not yet implemented:** SQLCipher encryption and keychain-backed search over
  vault items. Any process running as the user can still read fallback rows when
  no keychain is available; treat `~/RAVENBOT/ravenbot.db` as sensitive.

### Sandboxing

- On Linux with `bubblewrap`, `shell_exec`, task runner and file tools run in a
  filesystem, PID/UTS/IPC and optional network namespace with POSIX resource
  limits.
- The sandbox hides `$HOME`, `~/.ssh`, `~/.aws`, `~/.netrc`, `~/.config/gh`,
  `/etc/shadow` and sudoers.
- Workspace roots are confined; `/`, `$HOME`, the RAVENBOT data root and system
  directories are refused as workspaces.
- On macOS, the runner uses a default-deny Seatbelt profile: system paths are
  readable, only workspace paths and the user temp directory are writable, and
  network is allowed only when the policy enables it.
- On a platform with no backend (including Windows today), agent commands fail
  closed. The `Host` tier is the explicit opt-in, and
  `RAVENBOT_ALLOW_UNSANDBOXED=1` is a global override. A backend that cannot be
  applied is reported honestly; it is never described as isolation it did not
  provide.

### Network

- The sandbox can unshare the network namespace when the policy is set to
  blocked.
- A shared URL guard refuses non-HTTP schemes, embedded credentials, metadata
  hostnames, and link-local/unspecified addresses before a request. It is used
  by HTTP tools, browser navigation, custom MCP URLs, and redirect policies.
- Per-agent network grants are enforced at the actual URL on HTTP tools, browser
  navigation, MCP HTTP calls, and the fixed-host search/image tools.
- Arbitrary HTTP tools and MCP HTTP transports resolve the host once, reject
  private/link-local rebinding answers, pin the resolved address, and revalidate
  redirects. The DNS-rebinding window is closed.
- Private/localhost/LAN tool targets are blocked by default. Settings → Local has
  an explicit allow toggle; a per-agent domain grant still wins.
- The remote-access listener can serve HTTPS with a generated self-signed
  certificate. The certificate is stored owner-only, is stable across restarts,
  and its SHA-256 fingerprint is shown in Settings. A browser warns until the
  user accepts it.
- **Limitation:** self-signed TLS provides encryption, not a publicly trusted
  identity. There is no ACME/Let's Encrypt flow. Use a private tunnel or a
  TLS-terminating reverse proxy for remote access beyond a trusted LAN.

### Approvals and autonomy

- Interactive agents default to `Ask`: high-risk tools park on an Allow/Deny
  card.
- Attended office/routine/channel runs use a per-run **Review** policy: a
  high-risk tool still gates even for a `Full` bot, and delegated children
  inherit the policy. An interactive `Full` bot keeps its explicit choice.
- Office autonomy is opt-in: an office without an explicit toggle does not run
  autonomous cycles.
- The global kill switch pauses all runs. It is in-memory; restarting clears it.
  The headless MCP server has a separate `RAVENBOT_KILL_SWITCH` environment
  variable.

### Background daemon

- The installed service runs `ravenbot daemon`, not a hidden webview. It has no
  display dependency.
- Exactly one fleet owner may run. The desktop app and daemon share an OS file
  lock next to the database; the lock is released automatically on a crash.
- A desktop launch can stop the user service and take over; a second daemon
  exits rather than duplicating schedulers or SQLite writers.
- Linux with `loginctl enable-linger` keeps the daemon and remote listener alive
  after logout. macOS LaunchAgents and the Windows logon task stop with the
  session.
- The Windows target is a per-user Task Scheduler logon task, not a Windows
  service: no administrator rights, hidden, least privilege, single instance,
  restart on failure.
- Daemon-mode high-risk tools still park under the per-run Review policy.
  Approvals can be answered through a paired remote device or by taking over
  with the desktop app. New approvals/questions trigger an OS notification when
  a session exists, plus the configured HTTPS webhook and SMTP alert.
- Scheduled and workflow-webhook workflows execute headlessly through the same
  graph executor. Provider credentials are re-read every two minutes, so a
  keychain that unlocks after login is picked up without restarting.

### Local accounts

- The account registry stores Argon2id password hashes, never plaintext
  passwords. Sessions are random tokens stored as SHA-256 hashes with an
  eight-hour expiry; password changes and disabling an account revoke them.
- Five failed sign-ins for a username within five minutes are throttled.
- Each account has a separate data root and keychain namespace. The owner keeps
  the legacy unprefixed keychain names; additional accounts are prefixed with
  their account id.
- The account process validates its session against the registry at startup and
  every 30 seconds. A revoked or expired session closes the process.
- The registry and account roots are owner-only on Unix. Account windows can run
  concurrently; each workspace gets a separate port block and fleet lock.
- `ravenbot account env` is an explicit headless escape hatch for the same OS
  user; it is not a sandbox against local file access.
- Shared workspaces use a server-side, fail-closed IPC role gate. Owners may
  invoke every command; operators get an explicit run/message/approval allowlist
  plus read-only commands; viewers get only read-only commands. Unknown
  commands are denied for non-owners.
- Workspace grants are checked at startup and every 30 seconds. Revocation,
  role changes, workspace disabling and session expiry close the workspace
  process.
- The workspace owner's keychain namespace is used by shared users, so provider
  and connector credentials continue to work without being displayed. The role
  gate prevents non-owners from reading credential commands or changing them.
- `run_actors` records the acting account or external chat sender, role and
  workspace; workspace access is also written to the audit log.

### Native Telegram

- The bot token is stored through the OS keychain path with the owner-only
  database fallback. It is never returned to the renderer or written to logs.
- Pairing requires a one-time six-digit code generated from the desktop or CLI.
  Only paired chat ids can start runs; `/unlink` removes the link.
- The connector uses long polling, so no public webhook port is opened. The
  request URL contains the bot token, so request errors are logged without it.
- Inbound text is untrusted input. It goes through the normal capability,
  network, approval and budget gates.
- High-risk tools still park under Review. The chat is told to use RAVENBOT or
  the paired remote page to decide; the chat itself cannot approve.

### Native Slack

- Both Slack tokens (app-level and bot) are stored through the OS keychain path
  with the owner-only database fallback. They are never returned to the
  renderer or written to logs.
- Socket Mode means no public request URL and no inbound port. The app-level
  token is only used for `apps.connections.open`; the bot token is used for
  `chat.postMessage`.
- A one-time six-digit code pairs a conversation. Only paired conversations can
  start runs. Messages in a channel are ignored unless they mention the bot, so
  a shared channel is not driven by unrelated chatter. Redelivered socket
  events are claimed once by channel and timestamp.
- Inbound text is untrusted input. It goes through the normal capability,
  network, approval and budget gates.
- High-risk tools still park under Review. Slack cannot approve them; the reply
  points to RAVENBOT or the paired remote page.

### Native Discord

- The bot token is stored through the OS keychain path with the owner-only
  database fallback. It is never returned to the renderer or written to logs.
- The gateway connection is outbound. No public interactions endpoint, signing
  secret or inbound port is opened.
- A one-time six-digit code pairs a conversation. Only paired conversations can
  start runs. Guild messages are ignored unless they mention the bot, so a
  shared channel is not driven by unrelated chatter. Redelivered messages are
  claimed once by channel and message id.
- Replies use `allowed_mentions: []`, so an agent answer cannot ping `@everyone`
  or arbitrary users.
- Inbound text is untrusted input. It goes through the normal capability,
  network, approval and budget gates.
- High-risk tools still park under Review. Discord cannot approve them; the
  reply points to RAVENBOT or the paired remote page.

### Native WhatsApp

- The access token, app secret and verify token are stored through the OS
  keychain path with the owner-only database fallback. They are never returned
  to the renderer or written to logs.
- Every POST is verified with HMAC-SHA256 over the raw body using
  `X-Hub-Signature-256`, with a constant-time comparison. The GET challenge is
  checked against the verify token.
- The receiver binds loopback only. Public exposure is an explicit reverse
  proxy or tunnel decision, and Meta requires a trusted TLS certificate.
- Only paired phone numbers start runs. Redelivered messages are claimed once
  by number and Meta message id.
- Inbound text is untrusted input. It goes through the normal capability,
  network, approval and budget gates.
- High-risk tools still park under Review. WhatsApp cannot approve them; the
  reply points to RAVENBOT or the paired remote page.
- The Cloud API 24-hour customer-service window limits proactive messages.

### Native Microsoft Teams

- The app password is stored through the OS keychain path with the owner-only
  database fallback. It is never returned to the renderer or written to logs.
- Every Bot Framework activity is authenticated before routing:
  - RS256 signature against the published Bot Framework OpenID keys
  - issuer `https://api.botframework.com`
  - audience equal to the configured Microsoft App ID
  - `serviceurl` claim equal to the activity's service URL
- A mandatory tenant allowlist rejects activities from any other tenant.
- The messaging endpoint is loopback only. Public exposure is an explicit
  reverse proxy or tunnel decision, and Teams requires a trusted certificate.
- Channel messages are ignored unless they mention the bot; personal chats are
  accepted. Duplicate activity ids are claimed once.
- Inbound text is untrusted input. It goes through the normal capability,
  network, approval and budget gates.
- High-risk tools still park under Review. Chat approvals are accepted only
  from the paired conversation and only for a request belonging to its thread.
- Connector tokens are cached only until shortly before expiry and are never
  written to disk.

### Remote access

- Remote access is off by default and binds `127.0.0.1` unless the user
  explicitly enables LAN binding.
- Pairing uses a six-digit code that expires after five minutes, is single-use,
  and is rate-limited per IP and globally. Generating a new code invalidates the
  previous one.
- Tokens are 256-bit random values, shown once, and stored only as SHA-256
  hashes. Device rows can be revoked immediately.
- Roles are `viewer` (read), `operator` (send/approve/stop) and `admin`
  (operator plus the remote audit). Pairing codes are generated only from the
  local desktop UI; no HTTP endpoint can mint a token or change a role.
- The API exposes conversations, approvals, questions and runs only. It has no
  shell, filesystem, settings, plugin, or credential endpoints.
- Mutating requests and failures are written to `remote_audit` with the device
  name and role.
- The mobile page has no third-party scripts, uses a strict same-origin CSP, and
  renders user data with `textContent`.
- **Limitation:** the listener is HTTP. LAN mode is cleartext and should be used
  only on a trusted network or behind a private tunnel.

### Command execution

- `shell_exec`, `task_runner`, `docker` and `ssh_remote` are gated by a
  workspace-aware denylist: recursive deletes of a workspace root or outside
  every root, `.git` deletion, symlink escapes, `cd`/`pushd` escapes, shell
  expansion in delete targets, destructive git, disk/partition/power commands,
  privileged Docker and host mounts.
- Docker and SSH use argv, not a local `sh -c`.
- The denylist is defense in depth, not a substitute for the sandbox. It is a
  string matcher and cannot reason about every interpreter or program.

### MCP and plugins

- The desktop connector catalog includes 135 MCP servers. Several upstream
  launcher packages are explicitly marked **unverified**.
- `ravenbot mcp-serve` exposes a safe subset by default. Tools with `High` risk
  and the team control plane require `RAVENBOT_MCP_ALLOW_DANGEROUS=1`. File
  tools run in a dedicated MCP workspace.
- MCP tool risk is currently inferred from tool names for connectors. That is a
  gap: a malicious server can name a write tool `read_*`.

### Browser and computer control

- The browser dock is a native child webview, not an iframe; remote content has
  no Tauri IPC capability.
- The secure-input card writes passwords/OTP values directly into the page.
  Its **Save to vault** path sends a password only to the backend, which
  verifies that the live page origin equals the origin shown when the card
  opened and then stores it through the vault's keychain path. The password is
  never added to the transcript or sent to a model.
- The audit log records model rounds and tool calls. The global audit query
  returns only an allow-list of display fields; raw tool arguments never cross
  IPC and are not searchable. The underlying audit table still stores those
  arguments, so the database remains sensitive.
- Audit rows are append-only. A user can write a JSON export signed with the
  persisted Ed25519 identity and verify it in the app; verification checks the
  payload signature and reports whether the signer is this installation. The
  export is capped at 50,000 entries and fails rather than silently truncating.
  Retention is opt-in: `0` keeps entries forever (the default), and a finite
  policy (1–3650 days) runs at startup and every six hours. Saving a policy
  prunes immediately and reports the deleted count; the manual prune remains
  available. The UI warns before a finite policy is saved.
- Scheduled workflows are opt-in per workflow and only for office-scoped
  definitions. They run while RAVENBOT is open, use the latest saved version,
  and are subject to the existing budget/tool/approval gates. A failed scheduled
  run records its error on both the run and the schedule; one run is claimed at
  a time.
- Desktop attention notifications are opt-in. Their bodies are deliberately
  generic: category plus agent/office name, never prompt text, questions,
  secrets, or budget amounts. Automatic notifications are skipped while the app
  is focused, and the notification bridge is a Rust command, not a JS plugin
  capability exposed to remote content.
- Workflow webhook triggers are opt-in, office-scoped, and loopback-only. The
  URL token is shown once when enabled or rotated; SQLite stores only its
  SHA-256. Requests use `/workflows/<token>` or an `Authorization: Bearer`
  header, the request body is ignored, and the receiver only emits a trigger
  event. Execution happens in the desktop runner using the latest saved
  version. A trigger is acknowledged but lost if the desktop UI is not running.
- Email attention alerts are opt-in and SMTP-only. A remote host must use
  STARTTLS; plaintext SMTP is refused unless the host is loopback. The password
  is stored in the OS keychain when available (owner-only database fallback) and
  is never returned to the renderer. The recipient and sender are parsed as
  addresses, the subject is fixed per event kind, and office/agent names are
  sanitized before they enter the plain-text body. Email bodies contain only the
  event category, state, and office/agent name — never prompts, questions,
  secrets, or budget amounts. Delivery is best-effort with a 15-second timeout.
- Automatic cookie refusal has a global default. Named browser profiles may
  override it with always/never; automatic per-agent/office profiles use the
  global default. The dock resolves the effective policy for the active
  profile before scheduling rejection retries.
- Automatic cookie refusal is opt-in. It runs the same in-page script as the
  manual cookie button through the user-initiated evaluator, only clicks exact
  reject-style labels inside a short consent banner/dialog, and never clicks
  accept. The preference is stored in app settings; disabling it cancels pending
  retries.
- Memory summarization sends only the selected memory entries to the configured
  model provider for the selected agent, or the office lead for office memory.
  The model returns a candidate summary; nothing is changed until the user edits
  and confirms the preview. Applying it inserts the summary and deletes the
  selected sources in one transaction, and the audit log records the creation
  and each deletion without memory text.
- Memory rows record provenance as metadata: source (`user`, `agent`,
  `self-review`, `import`, `unknown`), source thread/run ids when known, and an
  updated timestamp. The text itself remains in the same owner-only database.
  Bundle imports are marked imported and drop foreign thread/run ids.
- Duplicate detection compares only embeddings and metadata within the selected
  agent or office. Removal re-validates that every entry is still similar to the
  kept entry, is capped at 200 entries per action, and writes a
  `MemoryChanged` audit event for each deletion. The scan itself is read-only.
- The retrieval benchmark is read-only with respect to user memory: it embeds a
  fixed set of 50 facts and 10 queries and never reads or sends stored memories.
  With a remote backend, only those fixed benchmark sentences leave the machine.
- Memory embeddings default to the offline local hash. Ollama and
  OpenAI-compatible endpoints are explicit opt-in. The endpoint is tested before
  it is applied; a remote endpoint must use HTTPS (loopback may use HTTP),
  credentials in the URL are rejected, redirects are disabled, and responses are
  capped at 16 MB. The embedding API key is keychain-backed with the owner-only
  database fallback and is never returned to the renderer. Applying a backend
  stages new vectors in a connection-local temporary table and swaps them in one
  transaction, so a provider failure does not leave a half-reindexed store. A
  memory-index lock makes writes wait for that swap, so a fact written
  concurrently cannot keep the previous provider's vector. The audit log records
  the provider, model, endpoint and reindexed count, never memory text. Selecting a remote backend sends memory text to that endpoint;
  Ollama keeps it on the machine.
- Agent and office memories can be edited or deleted only through commands
  scoped to the owning agent/office. Updates re-embed the text on the local
  embedding path. The audit log records a `MemoryChanged` event with the scope,
  action, category, memory id and office id; it deliberately does not store the
  memory text. Embedding vectors are an internal retrieval detail and are not
  returned over IPC.
- Remote attention webhooks are opt-in and HTTPS-only. The URL is stored in the
  OS keychain when available (owner-only SQLite fallback otherwise) and is never
  returned to the renderer; settings show only the host. Requests refuse
  redirects and have a 10-second timeout. Payloads contain the event kind,
  state, office/agent names, and fixed generic text only — never prompts,
  questions, secrets, tool arguments, or budget amounts. Delivery applies to new
  attention items, is best-effort, and does not block the run that raised them.
- Scheduled backups write signed exports to a user-chosen folder. The first is
  a full snapshot; later runs export entries since the last success. A due
  backup runs before retention, and automatic pruning is skipped if that backup
  fails, so a broken backup target cannot silently outrun deletion.
- Browser profiles are per selected agent/office: each has its own data
  directory under `~/RAVENBOT/browser-profiles/<profile>` with `0700`
  permissions. Switching scope destroys and recreates all tabs so cookies and
  sessions cannot leak between profiles. Tabs within one profile deliberately
  share that profile's web context. The tab session (URLs and active index) is
  stored as an owner-only `tabs.json` inside the profile directory; it contains
  no page content or credentials. Named profiles are an explicit shared choice:
  selecting one for multiple agent/office scopes deliberately shares its
  cookies and logins. Deleting a named profile removes its data directory and is
  refused while the dock is using it. This is session separation, not a
  security boundary against another process running as the same OS user.
- Agent DOM evaluation is available on Linux through WebKitGTK. Other platforms
  report it as unavailable.
- Agent input can be paused with **Take over**; the skills check the pause flag.
- Host desktop control is opt-in per agent and fail-closed on Wayland unless
  `RAVENBOT_ALLOW_WAYLAND_CONTROL=1` is set.
- Screenshots, typed text and page content can be sent to the model provider as
  part of a run. Do not type secrets into a computer-control session.

### Releases and supply chain

- `Cargo.lock` and `package-lock.json` are committed.
- CI runs `cargo test --workspace`, the frontend typecheck/tests, `cargo-audit`
  (advisory, non-blocking) and `cargo-deny`.
- **Not yet implemented:** signed installers (macOS notarization, Windows
  Authenticode, GPG checksums), a configured Tauri updater, a generated SBOM,
  Dependabot, and reproducible-build attestation. The release workflow currently
  produces unsigned artifacts.
- MCP launchers use `npx -y` without pinned versions, and skill/plugin catalogs
  are fetched live. Treat installed connectors and community skills as
  third-party code.

## Roadmap

The trust work is sequenced in
[`docs/COMPETITIVE-AUDIT-2026.md`](docs/COMPETITIVE-AUDIT-2026.md):

1. SQLCipher/encrypted fallback and the vault-request UI.
2. Remove forced `Full` from offices; per-run Hand over / Review / Approve.
3. Fail closed when no sandbox backend exists; add macOS isolation.
4. Wire the network policy/SSRF allowlist into every HTTP path.
5. Signed releases, updater, SBOM, and an automatic audit-retention schedule.

## Verification

Until signed releases exist, verify a download only against the checksum
attached to the same GitHub release, and understand that an unsigned checksum
proves integrity against the release page, not publisher identity.

```bash
sha256sum -c SHA256SUMS.txt
```

## Compliance

RAVENBOT stores data locally by default and has no telemetry. That helps with
GDPR/CCPA data-minimisation goals, but it is not a compliance certification.
HIPAA, SOC 2 or similar claims require deployment-specific controls, signed
builds, audit retention and a security review; none of those are currently
certified.

## Security updates

Security fixes are released as patch versions. Critical issues are described in
GitHub Security Advisories after a fix is available.
