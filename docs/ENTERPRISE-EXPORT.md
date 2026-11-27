# Enterprise export

RAVENBOT can stream a redacted metadata record of what its agents do to an
enterprise collector over **OTLP/HTTP JSON logs** (`POST <endpoint>/v1/logs`).
It is opt-in, local-first, and deliberately narrower than the signed audit
export: it is an activity signal for a SIEM or observability pipeline, not a
transcript.

This slice ships **logs only**. It does not ship SSO/OIDC, SCIM, OTLP metrics
or traces, metrics dashboards, a hosted relay, or data-residency controls.
Remote OIDC SSO is a separate, opt-in surface documented in
[`REMOTE-SSO.md`](REMOTE-SSO.md); it is not part of the export plane.

## What is exported

Two OTLP scopes share one resource (`service.name` and `service.version`):

### `ravenbot.audit`

Projected from the same allow-list as the in-app audit viewer:

| Attribute | Meaning |
|---|---|
| `audit.id`, `audit.rowid`, `audit.kind` | Stable audit identity and event kind |
| `bot.id`, `bot.name` | Agent that performed the action |
| `run.id`, `thread.id` | Run and conversation identifiers (not the thread title) |
| `tool.name` | Tool name only |
| `file.path`, `file.size` | File path and write size |
| `http.url`, `http.method` | Network target with query and fragment removed |
| `model.provider`, `model.name`, `model.tokens_in`, `model.tokens_out`, `model.cost` | Model usage |
| `permission` | Granted/denied permission or approval outcome |
| `actor.name`, `actor.role` | Account that started the run, when recorded |
| `memory.scope`, `memory.action`, `memory.id`, `memory.category`, `office.id` | Memory and office metadata |

The log body is the audit kind (for example `ToolCall`); severity is `INFO`.

### `ravenbot.runs`

One record per run that changed since the run cursor. The body is
`run.<state>` (for example `run.completed`).

| Attribute | Meaning |
|---|---|
| `run.id`, `run.bot_id`, `run.thread_id` | Identity |
| `run.state` | Stored lifecycle state (`planning`, `completed`, `failed`, …) |
| `run.outcome` | Variant only: `Success`, `PartialSuccess`, `Failure` or `Cancelled` |
| `run.tokens`, `run.cost_estimate`, `run.model_cost`, `run.tool_cost` | Usage and spend |
| `run.created_at`, `run.updated_at`, `run.completed_at` | Lifecycle timestamps |
| `actor.name`, `actor.role` | Account attribution, when recorded |

`timeUnixNano` is built from the row's RFC 3339 timestamp. An unparsable
timestamp omits the field; RAVENBOT never fabricates one.

## What is deliberately excluded

These fields can carry user content, prompts, file contents or credentials, so
they are never placed in the OTLP body:

- prompts, messages, message bodies and attachments;
- tool arguments and raw tool results;
- file contents and patches;
- the audit `event` JSON;
- **thread titles** (a thread is named from the first user message);
- **free-text reasons and approval/denial notes**;
- run checkpoints, result text and error text;
- API keys, bearer tokens, cookies and other secret values;
- **URL query strings and fragments** — only `scheme://host[:port]/path` is
  exported, because query parameters routinely carry tokens. An unparsable URL
  is omitted rather than exported raw.

The Settings card repeats the short version of this boundary.

## Configuration

Settings are stored in `app_settings`; no migration is required.

| Key | Default | Rules |
|---|---|---|
| `otel_export_enabled` | `0` | `1` enables the background loop |
| `otel_export_endpoint` | empty | Full URL, at most 2048 characters |
| `otel_export_service_name` | `ravenbot` | 1–64 printable characters |
| `otel_export_interval_secs` | `60` | 10–3600 seconds |
| `otel_export_batch_size` | `200` | 1–500 rows per source per request |
| `otel_export_audit_cursor` | `0` | Last delivered `audit_log.rowid` |
| `otel_export_run_cursor` | absent | JSON `{updated_at,id}` |
| `otel_export_last_attempt_at` | absent | RFC 3339 |
| `otel_export_last_success_at` | absent | RFC 3339 |
| `otel_export_last_error` | absent | Clipped, header-redacted text |
| `otel_export_last_audit_count` | `0` | Rows in the last delivered batch |
| `otel_export_last_run_count` | `0` | Rows in the last delivered batch |
| `otel_export_headers` | absent | Marker only; the headers live in protected storage |

### Endpoint and TLS

- A bare origin or `/` path becomes `/v1/logs`; a URL already ending in
  `/v1/logs` is kept, as is any other explicit collector path.
- `http` and `https` are accepted; any other scheme is rejected.
- HTTPS is required for remote hosts. Plain HTTP is accepted only for
  `127.0.0.1`, `localhost` and `[::1]` so a local collector can be tested.
- URLs with a username or password are rejected.
- Fragments are stripped during normalization.

### Headers and the keychain

Headers are a JSON object of header name to value, for example:

```json
{ "Authorization": "Bearer collector-token" }
```

They are validated (name `^[A-Za-z0-9-]+$`, no CR/LF in values, at most 4 KiB
per value and 20 headers) and stored through `crates/db/src/secrets.rs` under
the account `otel-export:headers`. The OS keychain is preferred; when it is
unavailable, the existing sealed-vault fallback is used and the database keeps
only `KEYCHAIN_MARKER`.

The commands never return header values. `get_otel_export_status` reports
`headers_configured: true|false`; saving with `headers: ""` deletes the entry;
saving with `headers: null` leaves it unchanged. A failed delivery may include a
collector response body, so RAVENBOT replaces every configured header value
with `[redacted]` and clips the body to 512 characters before returning or
storing the error.

## Cursors, retries and duplicates

- Audit rows are selected by `audit_log.rowid > cursor` in ascending rowid
  order. Runs are selected by `(updated_at, id) > cursor` in ascending order.
  Each source is limited to the batch size.
- A successful 2xx advances **both** cursors and records the counts, last
  attempt and last success. A failed request, non-2xx response, timeout (10
  seconds), or unreadable configuration advances neither cursor.
- Cursor writes are sequential. A crash between them can re-send a batch on the
  next run; it cannot skip a batch. **Duplicates are the accepted failure
  direction**, so a collector should deduplicate on `audit.id` / `run.id`.
- An empty batch does not open a request. It records the attempt so the
  background cadence does not hammer the database every tick, but it does not
  claim a success or clear a previous error.
- Once a batch is accepted, it is not re-sent by the cursor path.

## Background cadence

`otel_export_loop` runs in both the desktop shell and the headless daemon. It
reads the saved status every 30 seconds; when export is enabled and the
configured interval has elapsed since `otel_export_last_attempt_at`, it runs one
export. Recording the attempt even on failure means a broken collector is
retried on the configured cadence (for example every 60 seconds), not every 30
seconds.

Manual **Export now** and **Test connection** are available even while the
background stream is off, as long as an endpoint is configured. The test sends a
synthetic `ravenbot.export.test` log and does not touch either cursor.

## Commands and roles

| Command | Role |
|---|---|
| `get_otel_export_status` | owner, operator (viewers denied: it exposes endpoint metadata) |
| `set_otel_export_config` | owner only |
| `export_otel_now` | owner, operator |
| `test_otel_export` | owner, operator |

The Settings card lives under Audit log → **Enterprise export**. It shows the
endpoint, enabled state, interval, batch size, whether headers exist, the last
attempt/success, the last exported counts and the last error. Header values are
write-only.

## Evidence and tests

The local mock is the evidence; there is **no live customer collector test**.

```sh
CARGO_TARGET_DIR=target cargo test -p ravenbot-governance
```

The governance suite covers endpoint normalization and TLS rules, header
validation, payload attributes and exclusions, unparsable timestamps, outcome
variant reduction, URL sanitization and error redaction. The integration test
starts an in-process axum collector on an ephemeral loopback port and verifies:

- audit and run records arrive with the expected attributes;
- thread titles, approval notes, tool arguments, outcome text and URL queries
  do not appear in the request body;
- a successful batch advances the cursors and a second export sends nothing;
- batch limits apply;
- a 500 keeps the cursor and the next success sends the same event once;
- a reconciled 500 response still cannot echo a header value;
- the synthetic test log does not move the cursors.

Frontend evidence is `AuditExportPanel.dom.test.ts`, which saves configuration,
saves and clears write-only headers, exercises Test connection and Export now,
and asserts a saved header value is never rendered back.

The manual acceptance pass against a real collector, including payload
exclusions and cursor behaviour on failure, is in
[`docs/LIVE-CHECKS.md`](LIVE-CHECKS.md#manual-enterprise-export-check).

## Limits

- Logs only; no OTLP metrics or traces, and no metrics dashboards.
- No SSO/OIDC in this export plane (remote SSO is separate), no SCIM, egress
  allowlists, data-residency controls or admin API.
- No hosted relay; the collector must be reachable from the machine running
  RAVENBOT.
- No prompts, messages, tool arguments, file contents, thread titles, reasons,
  approval notes, run result/error text, checkpoints or secrets.
- No claim of a live enterprise collector validation. The local mock and unit
  tests are the shipped evidence.
