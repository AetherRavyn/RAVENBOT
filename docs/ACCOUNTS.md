# Local accounts

RAVENBOT now starts in an account launcher instead of opening one shared
database. Each account has its own data root, SQLite database, configuration,
workspace folders and keychain namespace. Switching accounts is a full process
hand-off, not a UI profile switch.

## First run

The launcher asks you to create the **owner** account. The owner keeps the
existing RAVENBOT root (`~/RAVENBOT` by default), so an installation that
predates accounts keeps its data, provider keys, chats, workflows and settings.

Every account also owns a workspace. The first account is the owner of the
existing root; later accounts get their own isolated root:

```
~/RAVENBOT/
  accounts.sqlite                 account registry (hashes, sessions)
  ravenbot.db                     owner data
  accounts/<account-id>/
    ravenbot.db                   that account's data
    config/ keys/ logs/ ...
```

Create additional accounts from the launcher with **Create another account**, or
from the CLI:

```sh
printf '%s\n' "$PASSWORD" | ravenbot account create alice
ravenbot account list
```

The launcher lists every account. After a successful sign-in the account and
workspace are remembered, so the next launch reopens that workspace directly;
**Log out** clears the memory and returns to the launcher. The launcher only
hands the account process a scoped session token; the account process validates
it against the registry before it opens the account database.

## Shared workspaces and roles

An account can open another account's workspace when the workspace owner grants
it a role. The grant is stored in the registry and resolved by the launcher:

```sh
ravenbot account grant owner alice operator
ravenbot account grant owner bob viewer
ravenbot account workspaces alice
```

When an account has more than one workspace, the launcher asks which one to
open and shows the role for each. The account process then uses the workspace's
database, configuration, workspace folders and keychain namespace, not the
signing-in account's own root.

Roles:

- **owner** — full control, including settings, credentials, connectors,
  providers, budgets and workspace grants.
- **operator** — can run work, send messages, answer approvals, stop/cancel
  runs, use the browser dock and work with workspace files; cannot change
  workspace settings, credentials, providers, bots or grants.
- **viewer** — read-only: list messages, read files, inspect status. It cannot
  send, run, approve or change anything.

The role check is **server-side** in the Tauri IPC gate. Non-owner roles are
fail-closed: a command must be explicitly allowed for the role. The UI hides
owner-only Settings sections and shows an operator/read-only banner, but the IPC
gate is what enforces the matrix.

### Team panel

The owner manages members in **Settings → Team** (also linked from the profile
card). The panel lists the owner and every grantee, and can:

- add an existing local account with a role;
- create a new local account with a temporary password and grant it in the same
  step (the account is rolled back if the grant fails);
- change a member's role;
- remove a member without deleting their account or files.

New accounts sign in from the launcher with the password the owner set. The
password is not stored in the clear, never enters the audit event, and is not
shown again. Membership changes write a `WorkspaceMembershipChanged` audit row
with the actor, workspace, member, role and action. Each run also records the
acting account in `run_actors`, so the Runs panel can show **by alice
(operator)**. The audit log joins the same table, shows the acting account on
every run-backed row, and its search box matches the account name; a tool call
is attributable to the person who started the run, not just to the agent.

Grants are checked at startup and every 30 seconds. Revoking a grant, changing
its role, disabling the workspace or expiring the session closes the workspace
process on the next check.

## Security model

- Passwords are hashed with **Argon2id** (default parameters) and a random salt.
- Sessions are 256-bit random tokens, stored only as SHA-256 hashes, expire after
  eight hours, and are revoked by password changes and account disabling.
- Five failed attempts for a username within five minutes are throttled.
- The registry and account roots are owner-only (`0700`/`0600` on Unix).
- Account processes receive an isolated keychain namespace. The owner keeps the
  legacy unprefixed key names so existing keys continue to work.
- The account process validates the session again every 30 seconds and exits if
  the session is revoked or expires.
- Account windows can run concurrently. Each workspace is assigned a port
  block: the owner keeps `8800/8801/8790`, and additional workspaces start at
  `8810/8811/8812` and move in steps of ten. A workspace is still protected by
  its own fleet lock, so two windows cannot write the same database at once.

## Isolation audit

The cross-tenant invariants — grants, revocation, sessions, per-workspace
databases, remote device tokens and thread shares — are executable:

```sh
cargo test -p ravenbot --test tenant_isolation
```

The threat model, the invariant → test table and the honest limits (accounts
on one Unix user share that OS account) are in
[TENANT-ISOLATION.md](TENANT-ISOLATION.md).

## CLI

```sh
ravenbot account list
ravenbot account create <username>       # password from stdin
ravenbot account passwd <username>       # revokes existing sessions
ravenbot account disable <username>
ravenbot account enable <username>
ravenbot account revoke <username>       # revoke all sessions
ravenbot account rename <old> <new>      # revokes sessions
ravenbot account delete <username> [--purge]  # non-owner only
ravenbot account grant <workspace> <account> owner|operator|viewer
ravenbot account ungrant <workspace> <account>
ravenbot account workspaces <account>     # list own + shared workspaces
ravenbot account grants <workspace>
ravenbot account env <username>          # headless isolation exports
```

`account env` is also the way to install a per-account daemon:

```sh
eval "$(ravenbot account env alice)"
ravenbot service install
ravenbot service status
```

The generated unit includes the account root, keychain namespace, service name
and the three port settings, so it can run alongside the owner daemon.

`account env` is for headless tooling and per-account CLI work:

```sh
eval "$(ravenbot account env alice)"
ravenbot run --bot "The Raven" "status"
```

It intentionally bypasses the desktop login gate. Anyone who can run commands
as the same OS user can already read the account files, so the app-level account
boundary is not an OS sandbox.

## Staying signed in

Closing the app does not log out. The selected account, workspace and session
token are remembered in `config/last-session.json` (owner-only `0600` on Unix),
and the session is extended to 30 days on each successful startup. The next
launch reopens the same workspace directly.

Explicit **Log out** revokes the session, deletes the remembered file, and
returns to the account launcher. A revoked, expired or deleted-account session
is also discarded and falls back to the launcher. The remembered token is a
bearer credential: use **Log out** on a shared machine.

## Backup and restore

Back up the whole RAVENBOT root: `accounts.sqlite`, `ravenbot.db`, `accounts/`,
`config/`, `keys/` and workspace folders. Restoring the root restores every
account and the owner session/account records. Do not copy only an account's
`ravenbot.db` without its keychain namespace; encrypted provider keys and
connector tokens will be missing.

## Current limits

- The account boundary is an application login gate, not an OS sandbox.
- Shared-workspace owner/operator/viewer grants, per-account daemon units and
  concurrent desktop account windows are implemented.
- The headless daemon remains per OS user; it does not become a root system
  daemon.
- Launch at login reopens the remembered workspace when its session is valid;
  otherwise it shows the launcher.
- Rich account administration (rename, delete, in-app account creation for
  additional accounts) is CLI-first for now.
