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

### Task claims

Any owner or operator can claim a task from its record in the task drawer. The
claim records the account username in `work_items.owner_account` and the time in
`work_items.claimed_at`, so the team can see who is handling the task. A viewer
cannot claim or release, and the IPC gate enforces that even if a button is
stale.

Claiming is coordination only: it does not change the task status, spend a retry
attempt, take the graph executor's lease, or rewrite the instruction and result.
The claim is resolved by a single conditional `UPDATE` in the database, so two
people racing for the same task cannot both believe they own it. The loser is
told who holds the claim. Re-claiming your own task is idempotent and keeps the
original `claimed_at`; only the current owner may release, which clears both
fields and lets somebody else claim it.

Every claim and release appends a `claimed` or `released` event with the account
(and the previous owner where relevant) to the task timeline. Releasing a claim
does not release a running graph: the execution lease remains the executor's.

## Remote sign-in

A phone or browser that uses remote access still pairs with a one-time code,
which fixes the device's maximum remote role. When the host runs an account
workspace it also requires a local RAVENBOT account sign-in before the page
shows conversations:

1. Pair the device (Settings → Remote access → generate code).
2. The page calls `POST /api/login` with the device bearer token and the
   account's username and password.

The device token is required, but the device does not need to be bound yet.
Before sign-in, only `GET /api/status`, `POST /api/login` and
`POST /api/device/revoke` are reachable; every other protected request returns
`401` with `code: "login_required"`. A successful sign-in stores the account on
the device row and attributes every later audit row to `@username`.

The effective remote role is the **lower** of the paired device role and the
account's workspace grant:

| workspace grant | paired device role | effective remote role |
| --- | --- | --- |
| owner | admin | admin |
| owner | operator | operator |
| owner | viewer | viewer |
| operator | admin | operator |
| operator | operator | operator |
| operator | viewer | viewer |
| viewer | any | viewer |

`owner` maps to the remote `admin` role; `operator` and `viewer` keep their
names. A viewer account therefore cannot write even if the device was paired
as admin.

The host revalidates a bound account at most once per 60 seconds. Disabling or
deleting the account, revoking its grant, or disabling the workspace makes the
next request fail closed with `401` and `code: "account_revoked"`; the device
binding is cleared and the page asks for credentials again. The device token
itself stays valid, so the same device can sign in with another account.
Sign-in attempts use their own per-IP and global rate limits, separate from
pairing, and wrong credentials return one generic message. Remote access never
stores a password or a desktop session token, and neither appears in logs.

A legacy single-user host has no account registry to consult, so its remote
page keeps the device-role-only flow (`account_login: false` in the pairing
response).

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
- Remote device account bindings are revalidated at most once a minute; a
  disabled, deleted or no-longer-granted account fails closed within 60 seconds.
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
