# Remote SCIM 2.0 user provisioning

RAVENBOT's remote listener can expose a small, opt-in SCIM 2.0 **Users**
endpoint so an identity provider can provision and deprovision members of one
workspace. SCIM is **off by default**, has its own bearer token, and never
widens a role or creates a workspace grant beyond the configured default.

This slice ships the backend, the SCIM API, and the account-registry write path.
The desktop Settings card is a follow-up; the owner-only Tauri commands and the
status fields it will use are already available.

## Endpoints

All paths are relative to the remote listener (`http://host:port` or the HTTPS
listener / reverse proxy). Responses use `application/scim+json`.

| Method | Path | Behaviour |
|---|---|---|
| `GET` | `/scim/v2/Users` | List workspace members with optional `filter`, `startIndex`, `count` |
| `GET` | `/scim/v2/Users/` | Same as the collection path (trailing slash tolerated) |
| `POST` | `/scim/v2/Users` | Create a member |
| `POST` | `/scim/v2/Users/` | Same as the collection path |
| `GET` | `/scim/v2/Users/{id}` | Read one member |
| `PATCH` | `/scim/v2/Users/{id}` | Replace `active` / `userName` |
| `PUT` | `/scim/v2/Users/{id}` | Minimal replace of `userName` and `active` |
| `DELETE` | `/scim/v2/Users/{id}` | Soft deprovision (`active: false`) |

SCIM routes are mounted on the remote listener with their own bearer middleware.
They are **not** reachable with a paired-device token, and they are not part of
the mobile `/api/status` response. When SCIM is disabled or has no valid token,
every `/scim/v2` path returns `404` — including a request with a formerly valid
token.

### Authentication

```http
Authorization: Bearer <64 hex characters>
```

Only the SHA-256 hash of the token is stored, in this workspace's
`app_settings`. The plaintext token is generated once by the desktop and
returned once; it is never written to `app_settings`, the audit log, `tracing`
output or any status response. A missing, malformed or wrong token returns
`401` with `WWW-Authenticate: Bearer`.

### Filters and pagination

Only these filters are accepted (field and `eq` are case-insensitive):

```text
userName eq "alice"
externalId eq "00u123"
id eq "6f1c..."
```

The quoted value supports `\"` and `\\` escapes. Any other attribute, operator,
bare value or trailing text returns `400` with `scimType: "invalidFilter"`.

`startIndex` is 1-based and defaults to `1`. `count` accepts `0`–`1000` and
defaults to `100`; the response is a SCIM `ListResponse` with `totalResults`,
`startIndex`, `itemsPerPage` and `Resources`. A page never contains more than
1000 users.

### User resource

```json
{
  "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
  "id": "…",
  "userName": "alice",
  "externalId": "00u123",
  "active": true,
  "meta": {
    "resourceType": "User",
    "created": "2026-11-27T10:00:00+00:00",
    "lastModified": "2026-11-27T10:00:00+00:00"
  }
}
```

`externalId` is omitted when unset. `meta.location` is deliberately omitted: it
would have to be derived from the request Host, which the listener does not
trust. Emails, phone numbers, groups, passwords and enterprise attributes are
never returned.

### Create

`userName` is required. `active` defaults to `true`; `externalId` is optional.
All other attributes are ignored. A successful create returns `201` with a
relative `Location: /scim/v2/Users/{id}`.

- Duplicate username or external id → `409`, `scimType: "uniqueness"`.
- Invalid username or malformed body → `400`, `scimType: "invalidValue"`.
- The workspace owner's username → `403`, `scimType: "mutability"`.

### Patch and put

`PATCH` accepts `Operations` with `op: "replace"`, plus `op: "add"` only for
`active`. Supported forms:

```json
[
  { "op": "replace", "path": "active", "value": false },
  { "op": "replace", "path": "userName", "value": "alice" },
  { "op": "replace", "value": { "active": false, "userName": "alice" } }
]
```

Any other path or operation returns `400`, `scimType: "invalidPath"`. `PUT` is a
minimal replace: `userName` is required, `active` defaults to `true`, and
immutable/unknown fields are ignored. Neither endpoint validates the full SCIM
schema.

### Delete

`DELETE` is a **soft deprovision**: it sets `active: false` and revokes the
account's sessions. It keeps the account, its workspace grants and its
`scim_identities` mapping so a later `PATCH active=true`/`PUT` (or a desktop
action) can restore it. A fresh `POST` with the same `userName` is rejected as a
duplicate by design. It returns `204`; deleting an already inactive user also
returns `204`. SCIM never purges an account's data root.

## Configuration

Stored in the workspace `app_settings`:

| Key | Meaning |
|---|---|
| `remote_scim_enabled` | `0`/`1`; the saved checkbox |
| `remote_scim_default_role` | `viewer` (default) or `operator` |
| `remote_scim_token_hash` | SHA-256 hex of the bearer token; empty when none |
| `remote_scim_last_used_at` | RFC 3339 of the last authenticated SCIM request; empty when none |

A valid active configuration requires `enabled`, a 64-character hex hash and a
valid role. A malformed saved value disables SCIM with a warning and **never
stops the listener**. The desktop `RemoteStatus.scim` summary always returns the
saved values (`enabled`, `token_configured`, `default_role`, `last_used_at`)
plus `active`, which is true only when the running listener loaded a valid
configuration. It never contains the token or its hash.

### Owner-only commands

| Command | Effect |
|---|---|
| `configure_remote_scim(enabled, default_role)` | Validate the role; enabling requires a saved token; save; restart a running desktop listener |
| `create_remote_scim_token()` | Generate 32 random bytes as 64 hex, save only the hash, clear last-used, restart a running listener, return `{ token, status }` |
| `revoke_remote_scim_token()` | Clear the hash, set `remote_scim_enabled=0`, clear last-used, restart a running listener, return the status |

`create_remote_scim_token` is the only time the plaintext exists. Generating a
new token invalidates the previous one immediately. Creating a token does not
enable SCIM; call `configure_remote_scim` afterwards. The commands are
owner-only in the same Tauri IPC gate as `configure_remote_sso`.

The desktop Settings card is **not part of this slice**; these commands and
fields are its backend contract.

## Security model

### One workspace, one token

A SCIM token is scoped to exactly one workspace. Every list, get, filter, patch,
put and delete proves that the target account holds a grant on that workspace.
Accounts that exist only in another workspace are never listed, returned,
renamed, disabled or deleted. A cross-workspace id returns `404`, not a partial
resource.

The workspace **owner** is not SCIM-managed. The owner is excluded from lists,
`GET` returns `404`, and creating the owner's username is refused with a
`mutability` error. `PATCH`, `PUT` and `DELETE` cannot target the owner because
the owner never resolves as a SCIM user.

### Shared accounts

The account registry is host-level: one account can hold grants on several
workspaces. A SCIM token is workspace-scoped, so it must not change an account
another workspace also relies on.

- `create` rejects an existing username, so a SCIM-created account starts with
  only this workspace's grant.
- `PATCH`/`PUT` that would rename an account, and `DELETE`, first check whether
  the account holds a grant on any **other** workspace.
- If it does, the request returns `403` `mutability` and the host-level account
  is left untouched. A host owner must manage shared accounts.
- The account's own workspace is not a grant and does not trigger this rule.

### Roles and grants

SCIM can create exactly one grant: the configured default role, `viewer` or
`operator`. `owner` is rejected. No `groups`, `roles` or IdP claim is read or
mapped. The IdP never widens a role, adds a second workspace, or makes an
account an owner.

### Password and sign-in

A SCIM-created account receives a random 32-byte password. The plaintext is
generated inside the account registry and is **never shown, returned, logged or
persisted**. It exists only in memory long enough to create the Argon2id hash.

There is therefore **no usable text/password login** for a SCIM-created account
until an owner sets a password in the desktop. The intended sign-in path is OIDC
SSO configured with `preferred_username` (or another claim) that maps to the
SCIM `userName`. SCIM does not configure OIDC and does not require it; an owner
may set a password later if the account should use password sign-in.

### Audit

Every SCIM mutation and every authentication failure writes a
`remote_audit` row:

- `device_id`: `NULL`
- `device_name`: `SCIM`
- `account_name`: the target `userName`, when known
- `method`: `SCIM POST`, `SCIM PATCH`, `SCIM PUT` or `SCIM DELETE`
- `path`: the request path
- `status`: the HTTP status

Reads are not audited. Authenticated requests update
`remote_scim_last_used_at`; this is the only write a read performs. Internal
errors are logged locally and returned as generic SCIM errors, never with SQL,
file paths or account-registry detail.

### Rate limits

- 600 requests per minute per source IP as seen by the listener (a reverse
  proxy collapses clients onto its own address).
- 3000 requests per minute globally across the listener.

The limiter runs before token lookup and before any database work. Exceeding it
returns `429`. The router's existing body limit (64 KiB) applies to SCIM
requests as well.

## IdP setup outline

1. Enable remote access and put the listener behind HTTPS (direct TLS or a
   reverse proxy). An IdP must be able to reach the listener; a loopback-only
   listener is not reachable from a hosted IdP.
2. Create a SCIM token in the desktop and configure `remote_scim_enabled=1`
   with the intended `remote_scim_default_role`.
3. At the IdP, use the base URL `https://your-host/scim/v2` and the bearer
   token. Enable **Users** provisioning only.
4. Map the IdP username to a RAVENBOT-valid `userName`: 3–64 characters,
   starting with a letter or number, using letters, numbers, `.`, `_` or `-`.
   Email-shaped values containing `@` are rejected by the host's existing
   account rules.
5. Configure OIDC SSO separately if those users should sign in. SCIM alone does
   not create a usable password.

## Limits and non-goals

- **Users only.** No SCIM `Groups`, `EnterpriseUser`, bulk operations, sorting,
  `attributes`/`excludedAttributes`, ETags or `If-Match`. `externalId` is set on
  create; `PATCH`/`PUT` do not change it.
- **No group or role mapping.** No IdP group/claim grants a workspace or role.
- **No auto-grant beyond the default role.** Existing accounts added out of band
  keep their existing grants; SCIM only creates the one default grant for a new
  account.
- **No auto-provisioning without SCIM.** OIDC SSO never creates an account or a
  grant; only a SCIM `POST` or a desktop action does. SCIM does not configure
  SSO.
- **No owner management.** The workspace owner is invisible to SCIM.
- **No hard delete.** `DELETE` is a soft deprovision and never purges data.
- **No live-IdP evidence.** The test suite uses a loopback mock provisioner to
  prove HTTP, auth, filtering, pagination, audit and limits, and account-registry
  tests to prove tenant isolation and rollback. A real IdP SCIM pass is a manual
  release check (see [`LIVE-CHECKS.md`](LIVE-CHECKS.md)); CI evidence is not
  live-provider evidence.
- **No hosted relay.** The IdP must reach this listener or a reverse proxy in
  front of it.
