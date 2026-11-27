# Remote access and roles

RAVENBOT can expose a small authenticated API so a phone or tablet on the same
network — or through a private tunnel — can read conversations, send a message,
answer approvals and questions, and stop a run.

It is **off by default**. Local desktop access is not affected by these roles.

## Threat model

The API is intentionally narrow:

- **No** shell, filesystem, settings, plugin, MCP credential, or bot-management
  endpoints.
- **No** token minting over HTTP. Pairing codes are generated only from the
  local desktop UI.
- Tokens are 256-bit random values, shown once, and persisted only as SHA-256
  hashes.
- Pairing codes are six digits, expire after five minutes, work once, and are
  rate-limited to five attempts per minute per IP (30 globally).
- A new pairing code invalidates the previous unused one.
- Revocation is immediate: the next request fails because the device row is
  checked on every call.
- When the host runs an account workspace, a paired device must also sign in
  with a local RAVENBOT account. The account's workspace grant caps the device's
  paired role; it never raises it.
- A bound account is revalidated at most once per minute. Disabling or deleting
  the account or revoking its grant fails the next check closed with
  `401 account_revoked` and clears the binding.
- Reads are available to `viewer`; every mutating method requires `operator` or
  `admin` in the **effective** role.
- `/api/remote/audit` requires `admin` in the effective role.
- Mutating requests and failures are written to `remote_audit` with the device
  name, effective role and, for a signed-in device, the account name. Reads are
  not persisted, so polling does not create a write every three seconds.

### Transport

Loopback (`127.0.0.1`) is the default. **Allow other devices on this network**
binds `0.0.0.0`.

Two encryption choices:

1. **Use HTTPS (self-signed)** — the listener generates a certificate for
   `localhost`, `127.0.0.1`, `::1` and the current LAN IP, stores it under
   `~/RAVENBOT/config/remote-tls`, and serves TLS. The certificate is stable
   across restarts; the Settings card shows its SHA-256 fingerprint. A browser
   warns until the certificate is accepted. This protects the token and message
   body from passive sniffing on a trusted network. The certificate is
   regenerated when the LAN IP used in its SANs changes; accept the new
   certificate after such a change. The certificate and key live in
   `~/RAVENBOT/config/remote-tls/`; the key is `0600` and the directory `0700`
   on Unix.
2. **A private tunnel or reverse proxy** — Tailscale Serve, an SSH
   local-forward, or a TLS-terminating reverse proxy. This is the recommended
   path for anything beyond a trusted LAN because it provides a real
   certificate and its own access control.

Self-signed TLS is encryption, not a public identity. Do not expose the port
directly to the public internet.

## Roles

| Role | Read | Send | Approve / answer | Stop run | Audit |
|---|---|---|---|---|---|
| `viewer` | yes | no | no | no | no |
| `operator` | yes | yes | yes | yes | no |
| `admin` | yes | yes | yes | yes | yes |

A role is chosen when the pairing code is generated. That is the device's
ceiling, not the final role: when account sign-in is enabled, the effective role
is the lower of the paired role and the account's workspace grant. `owner` maps
to `admin`; `operator` and `viewer` keep their names, and an unknown role is
treated as `viewer`. Local desktop UI remains the owner and can revoke any
device.

## Setup

1. Settings → Remote access.
2. Set a port (default `8790`). Enable **Use HTTPS (self-signed)** if the phone
   will connect over the LAN; then start the server.
3. For a phone, choose a role and generate a one-time code.
4. On the phone, open the URL shown in Settings, accept the certificate warning
   if HTTPS is on, then enter the code and a device name.
5. If the host runs an account workspace, sign in with the local RAVENBOT
   username and password. A legacy single-user host skips this step.
6. The phone keeps the device token, signs in, and polls every three seconds
   while a conversation is open.

With no desktop UI, pair from a terminal:

```sh
ravenbot remote pair --role operator
ravenbot remote status
```

To keep remote access reachable with no window, install the background daemon
(Settings → Local → Background service) and hand off. On Linux, enable lingering
so the daemon survives logout. Launching the desktop app later offers to take
over and stop the service.

The mobile page is served by the same listener at `/`. It has no third-party
scripts, no CDN, and a strict same-origin CSP. It renders message text with
`textContent`, never `innerHTML`. The token is kept in browser `localStorage` so
the device stays paired; **Forget device** revokes the token on the host before
clearing it locally, and also tells the user to revoke it from Settings if the
revoke request could not reach the host.

## API

All authenticated routes require `Authorization: Bearer <token>`.

| Method | Path | Role | Purpose |
|---|---|---|---|
| `GET` | `/api/health` | public | liveness |
| `POST` | `/api/pair` | public | exchange a one-time code for a token |
| `POST` | `/api/login` | paired device | bind the device to a local account |
| `POST` | `/api/sso/start` | paired device | start an optional OIDC flow and return the authorization URL |
| `GET` | `/api/sso/callback` | public | provider callback; consumes state and binds on success |
| `GET` | `/api/status` | paired device | device, account, login and SSO state, and counts |
| `GET` | `/api/bots` | viewer | agents |
| `GET` | `/api/threads?bot_id=` | viewer | conversations |
| `GET` | `/api/threads/:id/messages?limit=` | viewer | latest messages |
| `POST` | `/api/threads/:id/messages` | operator | send a message and start a run |
| `GET` | `/api/attention` | viewer | pending approvals and questions |
| `GET` | `/api/approvals` | viewer | pending approvals |
| `POST` | `/api/approvals/:id` | operator | `{allow, note?}` |
| `GET` | `/api/questions` | viewer | pending questions |
| `POST` | `/api/questions/:id` | operator | `{answer}` |
| `GET` | `/api/runs/:id` | viewer | state and spend |
| `POST` | `/api/runs/:id/cancel` | operator | cooperative cancel |
| `POST` | `/api/device/revoke` | any | a device revokes its own token |
| `GET` | `/api/remote/audit` | admin | recent remote actions |

`POST /api/threads/:id/messages` returns `202` with `run_id` and `message_id`;
the run executes in the background, so the HTTP request does not stay open for
minutes. If the conversation already has a non-terminal run, the request is
rejected with `409`.

When account sign-in is enabled, a paired but unbound device can reach only
`GET /api/status`, `POST /api/login`, `POST /api/sso/start` and
`POST /api/device/revoke`; every other protected route returns `401` with
`code: "login_required"`. `GET /api/status` reports
`{device, account, login_required, sso_enabled}`; `POST /api/login` takes
`{username, password}` and returns the effective `role`. A revoked account
returns `401` with `code: "account_revoked"` and the page asks for credentials
again. Sign-in attempts have their own per-IP and global rate limits, separate
from pairing.

When the owner configures an OIDC provider, the login page also shows
**Sign in with SSO**. The flow is optional and never replaces the password form.
See [`REMOTE-SSO.md`](REMOTE-SSO.md) for the flow, configuration, account
mapping and limits. The mobile status exposes only the `sso_enabled` boolean.

Approvals and questions still obey the agent's own approval mode. A remote
operator can answer them; a viewer can see them and is told to use the host.

## Storage

Migration 51 adds the three remote tables; migration 77 adds the account
binding; migration 78 adds pending OIDC attempts:

- `remote_devices` — name, role, token hash, account id/name/workspace role,
  created/last-seen/revoked
- `remote_pairings` — one-time code hash, role, expiry, used timestamp
- `remote_audit` — device attribution, effective role, account name, method,
  path, status, timestamp
- `remote_sso_attempts` — hashed state, device id, nonce, PKCE verifier,
  discovered endpoints, claim name, expiry; deleted on use or expiry

No plaintext token, pairing code or password is stored. The SSO verifier and
nonce are the two short-lived values that must be recoverable at the callback;
they are deleted when the attempt is consumed or expires. Authorization codes,
access tokens and ID tokens are never stored.

## Limits

- Remote access is tied to one account workspace. Pairing picks the device
  ceiling; signing in picks the person. It does not create separate desktop
  profiles or a shared-host relay.
- Remote SSO is optional, uses existing accounts only, and supports RS256 ID
  tokens from a provider reachable by the host. It does not add SCIM, group
  mapping, auto-provisioning, client secrets or OIDC logout. The desktop
  Settings form is pending.
- No WebSocket/SSE streaming yet; the mobile page polls.
- No attachment upload, voice capture, or settings management over the API.
- The self-signed certificate is generated locally; there is no ACME/Let's
  Encrypt flow. Use a reverse proxy if you need a publicly trusted certificate.
- Remote access is the control surface when the headless daemon owns the fleet.
  On Linux with lingering enabled it stays reachable after logout; on macOS the
  LaunchAgent stops with the user session.
- Workflow-webhook execution and desktop notifications still require the
  desktop UI; approvals and questions can be answered remotely.
