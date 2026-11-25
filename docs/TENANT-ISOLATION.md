# Tenant isolation audit

RAVENBOT's local accounts and shared workspaces are an **application-level**
tenant boundary. This document records the assets, the threat model, and the
executable evidence for that boundary. It does not claim an OS sandbox.

Run the audit:

```sh
cargo test -p ravenbot --test tenant_isolation
```

The seven tests in `src-tauri/tests/tenant_isolation.rs` create a throwaway
registry under a unique temporary root per case, so they never read or write
the user's real `RAVENBOT_HOME`. They call the production registry, database
and token-lookup functions rather than a test-only shortcut.

## Assets and trust boundaries

### Account registry — `accounts.sqlite`

The registry lives at the root of the RAVENBOT tree and is the identity
authority for every local account. It contains:

- `accounts`: username (`UNIQUE COLLATE NOCASE`), Argon2id password hash,
  per-account `data_root`, `disabled` flag and the owner flag;
- `sessions`: SHA-256 hashes of random 256-bit bearer tokens, with expiry and
  revocation timestamps;
- `workspace_grants`: `(workspace_id, grantee_id) → owner | operator | viewer`.

A workspace **is** an account's data root. The owner account keeps the legacy
root (`~/RAVENBOT`); later accounts get `accounts/<account-id>/`. Sharing a
workspace does not merge databases: it changes which data root the signed-in
actor's process opens. The registry is the only place that knows the mapping
from identity to workspace and from workspace to filesystem root.

Trust boundary: the launcher/CLI that writes the registry, and the registry
itself. Account processes receive a session token and an account/workspace id;
they re-validate both against the registry at startup and every 30 seconds.

### Per-workspace databases — `ravenbot.db`

Each data root has its own SQLite database containing threads, messages, run
and audit history, secrets, remote devices and thread shares. The signed-in
process opens exactly one database: the one under the selected workspace's
`data_root`. There is no cross-database join and no code path that opens a
second workspace's database for a request.

Trust boundary: the process environment (`RAVENBOT_HOME`, `RAVENBOT_DB`,
`RAVENBOT_WORKSPACE_ID`). The launcher derives it from the registry, and
`detect_startup` refuses a workspace the actor no longer has access to.

### IPC role gate — `src-tauri/src/access.rs`

The Tauri command surface is guarded server-side, not in the UI. The gate is
fail-closed:

- `owner`: every command;
- `operator`: an explicit run/message allowlist plus read-shaped commands;
- `viewer`: read-shaped commands only, minus credential and administrative
  reads (MCP env, vault reveal, remote devices, grants, audit export, service
  status).

A role that is not one of those three is denied. The UI banners and hidden
buttons only reflect the gate; they do not enforce it.

### Remote device tokens

A paired remote device is a row in the **workspace's** `remote_devices` table
containing a SHA-256 token hash and a `viewer`/`operator` role. The remote
listener that serves a workspace opens that workspace's database, so a bearer
token can only authenticate against the tenant that minted it. Revocation is a
local update in that same table; the raw token is never stored.

### Thread-share capabilities

A thread share is a row in the workspace's `thread_shares` table containing a
SHA-256 token hash, a snapshot timestamp, an expiry and an optional revocation
timestamp. The link is read-only and is resolved by the listener serving that
workspace's database. Revoking or expiring it only affects that workspace.

### Filesystem permissions

On Unix the registry and account roots are `0700`; the registry database, the
workspace database and the remembered-session file are `0600`. This is
defence-in-depth against other OS users, not the tenant boundary between
RAVENBOT accounts (see limits).

## Threat model

| Adversary or event | Attempt | Result and enforcement |
|---|---|---|
| Another local account, signed in as itself | Open or enumerate a workspace it was not granted | `workspace_for_actor` returns `None`; `list_workspaces_for_account` returns only the actor's own workspace plus its grants, and no IPC query opens another tenant's database. Proved by `grants_resolve_only_for_the_workspace_they_target`. |
| A member whose grant was revoked | Keep using a workspace after the owner removes the grant | The role is read from `workspace_grants` on every resolve; the next `open_workspace`/startup/watch check fails. The identity session stays valid, but it grants no workspace. Proved by `revoking_a_grant_removes_access_immediately_and_leaves_the_session_valid`. |
| A stolen session token | Act as the victim, or use the token to enter an unrelated workspace | The token authenticates only its own account; workspace access is still a separate grant lookup. A valid session for Carol does not open Bob's workspace. Proved by `a_valid_session_identifies_an_account_but_grants_no_workspace`. |
| A stolen remote device token | Read or steer another workspace | The token row lives in one workspace database and is resolved there only; another workspace's database returns `None`. Revocation is local to the minting workspace. Proved by `remote_device_tokens_are_workspace_scoped`. |
| A leaked share link | Read beyond the shared thread, or reach another workspace | The capability is one row in one workspace database, bound to one thread, read-only, expiring and revocable. Another workspace's database cannot resolve it. Proved by `thread_share_tokens_are_workspace_scoped`. |
| A duplicated username | Create a look-alike account (`Alice` vs `alice`) or shadow an existing identity | Usernames are unique case-insensitively in the schema and the create/rename paths reject a collision before insert. Proved by `duplicate_usernames_are_rejected_and_disabled_accounts_cannot_enter`. |
| A disabled account | Sign in, keep an old session, or use a stale grant | Authentication is refused, existing sessions are revoked and filtered by `validate_session`, and `workspace_for_actor` refuses a disabled actor even if a grant row still names it. Proved by the same test. |
| A database copied between tenants | Read tenant A's rows while running tenant B | Each root opens its own `ravenbot.db`; IDs from A do not resolve through B's bot, thread, message or audit queries. Proved by `each_account_root_holds_an_independent_database`. |

## Invariant → evidence

| Invariant | Test |
|---|---|
| A grant resolves only the granted workspace and appears only in that workspace's member list. | `grants_resolve_only_for_the_workspace_they_target` |
| Revocation takes effect on the next resolve without invalidating the identity session or the actor's own workspace. | `revoking_a_grant_removes_access_immediately_and_leaves_the_session_valid` |
| A valid session identifies an account but is not a workspace capability. | `a_valid_session_identifies_an_account_but_grants_no_workspace` |
| Each account root has an independent database; A's bot/thread/message/audit ids are not readable through B. | `each_account_root_holds_an_independent_database` |
| Remote device tokens resolve only in the workspace database that stored their hash; revocation is tenant-local. | `remote_device_tokens_are_workspace_scoped` |
| Thread-share tokens resolve only in the workspace database that minted them; revocation is tenant-local. | `thread_share_tokens_are_workspace_scoped` |
| Usernames are case-insensitively unique; disabled accounts cannot authenticate, keep a session, or resolve into any workspace. | `duplicate_usernames_are_rejected_and_disabled_accounts_cannot_enter` |
| IPC commands are denied unless the effective role allows them. | `access::tests` (unit tests in `src-tauri/src/access.rs`) |

The `access::tests` row runs in the library unit suite
(`cargo test -p ravenbot --lib access::tests`), not in the integration binary.
The integration audit proves the identity/workspace resolver and data paths
that the IPC handlers call; the role matrix itself is a unit test because the
`access` module is private to the crate.

## Honest limits

- **One Unix user, many accounts.** Any process running as the same OS user
  can read every account root directly; `0700`/`0600` do not stop it, because
  that process is the owner. The tests prove the registry, database and token
  isolation paths, not an OS sandbox. Running mutually hostile tenants safely
  needs separate OS users or hosts.
- **`account env` is a deliberate bypass.** It exists for headless tooling and
  per-account daemons and does not perform the desktop login gate. It is
  documented in [ACCOUNTS.md](ACCOUNTS.md#cli).
- **Remote account login is not implemented.** Remote access pairs a device
  with a role inside one workspace; it does not sign a remote user in as a
  registry account, and the shared-host relay is separate work. A paired
  device therefore sees only the workspace whose listener it paired with.
- **The IPC test coverage is unit-level.** The integration audit exercises the
  resolver and query layer the IPC handlers call. The role matrix itself is
  covered by `access::tests`; the webview bridge, UI hiding and a live remote
  listener are not end-to-end tests here.
- **Token theft is credential theft.** A live stolen session or device token is
  valid until it expires or is revoked. The audit proves it cannot be widened
  into a different workspace, not that theft is harmless.
- **No encryption at rest for tenant data.** SQLite files hold threads,
  messages and audit rows in the clear. Provider keys and connector secrets
  use the OS keychain/namespace, which is separate from this audit.
- **Backups and the registry are shared.** A backup of the whole root contains
  every account and the registry; restore it as one unit, and protect it
  accordingly.
