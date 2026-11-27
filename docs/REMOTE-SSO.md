# Remote single sign-on (OIDC)

RAVENBOT can let a paired phone or browser sign in to the remote surface with
an existing OIDC identity provider. SSO is **off by default** and additive: the
local username/password form stays visible and keeps working.

What ships today:

- the backend authorization-code + PKCE flow for the paired-device listener;
- a **Sign in with SSO** button on the mobile login page when the host has a
  valid configuration;
- an owner-only **Settings → Remote → Single sign-on (OIDC)** card, backed by
  the `configure_remote_sso` Tauri command and the non-secret SSO summary in
  `RemoteStatus`.

What deliberately does **not** ship:

- SCIM Groups, group mapping, role mapping, or auto-provisioning from an OIDC
  claim (SCIM **Users** is a separate, off-by-default surface; see
  [`REMOTE-SCIM.md`](REMOTE-SCIM.md));
- creating a RAVENBOT account from an IdP identity in the SSO flow;
- OIDC logout, back-channel logout, or provider-side session management;
- confidential clients or client secrets;
- a hosted SSO relay.

## Security model

SSO proves an identity. It does not create or grant an account.

1. The device must already be paired, and `/api/sso/start` requires its bearer
   token.
2. The browser is sent to the provider with a 256-bit `state`, a 256-bit
   `nonce`, and a PKCE S256 challenge. The verifier is 64 hex characters.
3. The public callback consumes the state atomically. A state is single-use,
   expires after ten minutes, and is stored only as a SHA-256 hash.
4. The ID token must be **RS256**. `none`, HMAC and every other algorithm are
   rejected before a key is selected. The signature is checked against the
   provider JWKS; `iss`, `aud`, `exp`, optional `nbf`, `sub` and `nonce` are
   checked. When `azp` is present it must name the same client. Clock leeway is
   60 seconds.
5. The configured claim (default `preferred_username`) is mapped
   case-insensitively to an **existing** local RAVENBOT account through
   `AccountAuthenticator::account_by_identity`. An unknown, disabled,
   deleted or workspace-grant-less account returns the same generic
   `reason=account` as any other mapping failure. There is no account
   enumeration in the browser response.
6. On success the device is bound with the existing
   `RemoteAccessQueries::bind_account`, the account is cached, and an audit row
   with method `SSO` records the device, account and effective role. The
   effective role is still the lower of the paired device role and the
   account's workspace grant.
7. The page never receives a token in a URL. The device token stays in
   `localStorage` as before; the callback only redirects with `?sso=ok` or
   `?sso=error&reason=...`.

### Public client

The client is public and uses PKCE. There is no client-secret setting, no
client-secret parameter in the token exchange, and no code path that reads one.
Register the remote callback as a **public/native** client at the IdP.

### Account mapping

The username claim must be a **top-level string claim**. Its trimmed value is
compared case-insensitively with existing RAVENBOT **usernames**, not display
names, email addresses or IdP group names. If the IdP only exposes an email,
either configure `email` as the claim and use matching RAVENBOT usernames, or
keep password sign-in. Mapping a value never creates an account or a grant.

### Transport

HTTPS is required for the issuer, authorization endpoint, token endpoint,
JWKS and redirect URL. Plain HTTP is accepted only for the exact loopback hosts
`127.0.0.1`, `localhost` and `[::1]`, so the test IdP can run on the same
machine. URL credentials, query strings and fragments are rejected where they
do not belong.

The post-callback redirect uses the **origin of the configured redirect URL**.
It never uses the request `Host` header or any user-supplied value.

### Network bounds

- 10-second timeout per discovery, JWKS and token request.
- 256 KiB maximum response body, enforced while streaming.
- At most two redirects; every redirect target is revalidated against the same
  URL rules.
- Callback rate limit: 30 per minute per IP and 300 per minute globally.
- At most 5 outstanding attempts per device and 500 globally. Excess requests
  get `429`.

### Secrets and logs

The authorization URL contains a single-use state. Treat it as sensitive for
its ten-minute life and do not share it: it is delivered only to the device
that called `/api/sso/start`.

Authorization codes, access tokens, ID tokens, nonces and verifiers are never
written to the audit log or `tracing` output. The token endpoint response's
`access_token` is ignored. Provider `error_description` is never forwarded or
logged; the browser gets one fixed reason.

A pending attempt stores the nonce and PKCE verifier in SQLite because the
callback needs them. They live for at most ten minutes and the row is deleted
on first use. The state itself is stored only as a hash. This is deliberate and
is the one place a short-lived credential is persisted.

## Configuration

Stored in workspace `app_settings`; none of it is a secret and none of it needs
the keychain.

| Key | Meaning | Validation |
|---|---|---|
| `remote_sso_enabled` | `0`/`1` | Enables the flow |
| `remote_sso_issuer` | Issuer URL | No trailing slash, max 2048, HTTPS (loopback HTTP only) |
| `remote_sso_client_id` | Public client ID | 1–256 printable ASCII characters |
| `remote_sso_redirect_url` | Exact registered callback | Must end in `/api/sso/callback`; max 2048; HTTPS (loopback HTTP only) |
| `remote_sso_scopes` | Default `openid profile email` | Must include `openid`; max 1024 |
| `remote_sso_username_claim` | Default `preferred_username` | `^[A-Za-z0-9_.:-]{1,64}$` |

An invalid saved configuration disables SSO and logs a warning; it never stops
the remote listener. The mobile `/api/status` exposes only `sso_enabled`. The
desktop `RemoteStatus` returns the saved summary (`issuer`, `client_id`,
`redirect_url`, `scopes`, `username_claim`) even while SSO is disabled or the
values are invalid, so the owner can see and repair them. `sso_enabled` is the
live, validated flag, not merely the saved checkbox.

### Desktop form

Settings → Remote shows **Single sign-on (OIDC)** after the remote-server card
and before pairing. It is owner-only, like the rest of Settings. The form has:

- an enable checkbox;
- issuer URL, client ID and exact redirect URL;
- scopes (default `openid profile email`) and username claim (default
  `preferred_username`);
- a short security block explaining the public client + PKCE model, existing
  accounts only, RS256-only ID tokens, no group/role mapping or
  auto-provisioning, and that saving restarts a running listener.

Saving validates on the host, writes the settings, updates the form from the
returned `RemoteStatus`, and restarts a running desktop listener so the new
snapshot applies. A headless daemon reads the new settings on its next start.
Disabling works even with blank or stale values.

**Saving does not contact the IdP and there is no "Test connection" button.**
Discovery, JWKS and the token endpoint are first contacted when a paired device
presses **Sign in with SSO**. The provider must therefore be reachable from the
host before that step; a green save only means the configuration passed local
validation.

If a saved value was invalid, the host disables SSO and the status summary is
`null`, so the form opens empty. Re-enter the values and save to repair it.

The same save is available through the owner-only Tauri command:

```ts
invoke("configure_remote_sso", {
  enabled: true,
  issuer: "https://idp.example.com",
  clientId: "ravenbot-remote",
  redirectUrl: "https://bot.example.com/api/sso/callback",
  scopes: "openid profile email",
  usernameClaim: "preferred_username",
});
```

Equivalent SQL for a headless/development setup:

```sql
INSERT INTO app_settings (key, value, updated_at) VALUES
  ('remote_sso_enabled', '1', datetime('now')),
  ('remote_sso_issuer', 'https://idp.example.com', datetime('now')),
  ('remote_sso_client_id', 'ravenbot-remote', datetime('now')),
  ('remote_sso_redirect_url', 'https://bot.example.com/api/sso/callback', datetime('now')),
  ('remote_sso_scopes', 'openid profile email', datetime('now')),
  ('remote_sso_username_claim', 'preferred_username', datetime('now'))
ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at;

-- Restart remote access afterwards so the listener reloads the snapshot.
```

## Flow

1. `POST /api/sso/start` (Bearer device token; reachable before account
   binding, like `/api/login`).
   - SSO off → `409`.
   - Fetch `<issuer>/.well-known/openid-configuration`.
   - Require the discovery `issuer` to equal the configured issuer exactly.
   - Require `authorization_endpoint`, `token_endpoint` and `jwks_uri` to pass
     the URL rules.
   - If `response_types_supported` is present it must contain `code`; if
     `grant_types_supported` is present it must contain `authorization_code`;
     if `code_challenge_methods_supported` is present it must contain `S256`.
   - Store the attempt and return `{ "url": "..." }` with `response_type=code`,
     `client_id`, `redirect_uri`, `scope`, `state`, `nonce`,
     `code_challenge` and `code_challenge_method=S256`.
2. The browser authenticates at the IdP and returns to
   `GET /api/sso/callback?code=&state=` (public).
   - Rate limit, then consume the state.
   - Unknown, expired or replayed state → `reason=state`.
   - Provider `error` present → `reason=provider`.
   - Exchange the code at the stored token endpoint with
     `grant_type=authorization_code`, `code`, `redirect_uri`, `client_id` and
     `code_verifier`. A non-2xx response or missing `id_token` →
     `reason=exchange`.
   - Verify the ID token → `reason=token`.
   - Extract the configured claim and map it to an existing account →
     `reason=account`.
   - Bind, cache, audit `SSO` and redirect `?sso=ok`.
3. The page clears the query with `history.replaceState`, then calls
   `/api/status` and enters the chat. On an error it shows one short generic
   message and leaves the username/password form available.

Fixed reasons: `state`, `provider`, `exchange`, `token`, `account`, `config`.
Authentication failures audit `401`; host/provider failures audit `500`.
Nothing provider-supplied is reflected in the redirect, audit row or logs.

## Audit and storage

- `remote_sso_attempts` (migration 78) holds one row per pending flow:
  hashed state, device id, nonce, verifier, provider endpoints, claim name,
  timestamps. Expired rows are pruned on listener start and on every flow.
- `remote_audit` gets a row with method `SSO`, path `/api/sso/callback`, the
  device name, account name and effective role on success.
- `remote_devices` gets the existing `account_id`, `account_name` and
  `account_role` binding. Account revalidation (disabled/deleted/grant
  revoked) keeps working exactly as it does for password sign-in.
- No authorization code, access token or ID token is stored.

## Limits

- **Existing accounts only.** An IdP identity never creates an account, a
  workspace grant, or a role. A person must already be a member.
- **RS256 only.** Providers that sign ID tokens with another algorithm are not
  supported. Discovery must advertise a compatible code flow and S256.
- **No group/role mapping or auto-provisioning in the SSO flow.** Removing
  someone at the IdP does not, through SSO alone, disable their local account;
  disable it in RAVENBOT or revoke its workspace grant. A separately configured
  SCIM **Users** client can soft-deprovision them (see
  [`REMOTE-SCIM.md`](REMOTE-SCIM.md)). Revoking the grant fails the device
  closed within the existing one-minute revalidation window.
- **No OIDC logout or session management.** "Forget device" revokes the
  RAVENBOT device token; it does not end the IdP session.
- **The host must reach the IdP.** Discovery, JWKS and the token endpoint are
  server-to-server requests from the RAVENBOT host, not from the phone. An IdP
  reachable only from the phone will not work.
- **Loopback HTTP is test-only.** Use HTTPS for every real deployment. Do not
  expose the listener directly to the internet; use a private tunnel or a
  TLS-terminating reverse proxy.
- **Saving the form does not verify the IdP.** Local validation only; the first
  network request happens when a paired device starts sign-in. There is no
  test-connection button because the backend has no such command.
- **No live-IdP evidence is claimed.** CI uses a loopback mock IdP. A release
  must follow the manual check in
  [`LIVE-CHECKS.md`](LIVE-CHECKS.md#manual-oidc-sso-check) against a real
  provider.

## Tests

- `crates/remote/src/sso.rs` unit tests cover config validation, URL rules,
  state/nonce/verifier/challenge construction, claim extraction and signing-key
  selection.
- `crates/remote/src/lib.rs` unit tests cover single-use consumption, expiry,
  pruning and per-device attempt caps.
- `crates/remote/tests/sso.rs` runs a loopback axum IdP with a committed
  **test-only** RSA key. It covers the start URL, a successful bind and audit,
  replay, wrong nonce/issuer/audience/signature/expiry, `none`/HS256,
  bad PKCE, unknown/no-longer-authorized accounts, a missing claim, a revoked
  device, SSO disabled, a spoofed `Host`, provider error text and password
  login with SSO configured.
- `src-tauri/src/accounts.rs` covers case-insensitive identity mapping and
  rejects unknown, disabled and grant-less accounts.
- `src-tauri/src/access.rs` proves `configure_remote_sso` is owner-only.
- `src/lib/components/RemoteSsoCard.dom.test.ts` covers the form, camelCase IPC
  arguments, the returned status, rejected saves, disabling and the absence of
  a fake test/connect button. `Settings.dom.test.ts` renders the card in the
  remote section without axe violations.
