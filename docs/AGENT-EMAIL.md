# Agent email — outbound drafts only

RAVENBOT's agent email surface is deliberately narrow: **an agent may create a
draft, and only a person may send it.** There is no `email_send` tool, no
auto-send setting, and no background sender.

This slice ships the backend and the agent-facing draft tool. The
**Settings → Mail** screen is a follow-up; it is not part of this change. Until
it ships, the owner-only commands below are the only way to configure the
account, review drafts and send one.

## Product rule

1. The agent calls `email_draft` and gets back a draft id. The tool cannot reach
   SMTP, HTTP or any other network.
2. The draft is stored in `email_drafts` with status `draft`.
3. A person reviews and edits it (through the follow-up Mail UI or the
   commands). Only an owner can send or discard it.
4. `send_email_draft` is the only code path that opens an SMTP connection, and
   it is invoked by a person. A successful send records the audit event
   `email_send` with the draft id and recipient count — never the body or the
   addresses.

## Configuration

Agent email uses its **own SMTP account**, separate from attention alert email.
The alert recipient is fixed, so reusing it would silently make the alert inbox
the agent's identity.

Settings are stored in `app_settings`:

| Key | Meaning |
|---|---|
| `agent_email_host` | SMTP host. STARTTLS for a public host; loopback only for `none`. |
| `agent_email_port` | 1–65535. Defaults to 587 when unset. |
| `agent_email_security` | `starttls` or `none`. `none` is accepted only for `localhost`, `127.0.0.1` or `::1`. |
| `agent_email_username` | Optional. When set, a password is required. |
| `agent_email_password` | Keychain marker; the secret itself lives in the OS keychain or encrypted sealed vault. |
| `agent_email_from` | The sender address. Validated as a mailbox. |
| `agent_email_allowlist` | Comma-separated exact addresses and/or domains. Empty means any recipient. |

The password is write-only. `set_agent_email` accepts it as `password`:
omitted/`null` keeps the saved value, a non-empty string writes it, and an empty
string clears it (clear the username first, because a username without a
password is rejected). Responses expose only `has_password`. The password is
never logged, returned, stored in the draft, or included in an SMTP error.

### Allowlist

- An entry containing `@` is an exact address, matched case-insensitively
  (`boss@example.com` does **not** cover `other@example.com`).
- Any other entry is a domain and covers that domain and its subdomains:
  `example.com` covers `example.com` and `team.example.com`, but not
  `notexample.com`.
- Wildcards, schemes, ports and bare single labels are rejected.
- The list is validated when saved and re-checked when a draft is created,
  edited, tested or sent. Removing an entry after a draft exists blocks that
  draft until it is edited or the entry is restored.
- Entries are capped at 200; each field is capped at 20 recipients.

## Storage

Migration `079_agent_email_drafts.sql` adds `email_drafts`:

| Column | Notes |
|---|---|
| `id` | Draft UUID. |
| `bot_id`, `thread_id`, `run_id` | The run that created the draft. |
| `to_addresses`, `cc_addresses` | JSON arrays of validated strings. |
| `subject`, `body` | Plain-text content. |
| `status` | `draft`, `sending`, `sent` or `discarded`. |
| `error` | Sanitized last send error while the row stays `draft`. |
| `created_at`, `updated_at`, `sent_at` | RFC 3339 timestamps. |

A send claims the row atomically with `begin_send`: only
`WHERE status = 'draft'` can become `sending`, so two concurrent invocations
cannot both open SMTP. The winner then records `sent` (only `sending -> sent`)
or returns the row to `draft` with a sanitized error (`sending -> draft`).
`update_content` and `mark_discarded` remain `draft`-only, and `sent` and
`discarded` are terminal. A failed send therefore stays retryable, and a second
send or discard cannot act on a finished or in-flight row.

If the process dies between claiming a draft and recording the outcome, the row
is left in `sending`. `list_email_drafts` first calls
`recover_interrupted_sends(600)`: any `sending` row older than ten minutes (SMTP
times out after 15 seconds) returns to `draft` with
**“The send was interrupted. Verify with the recipient before retrying.”**
Recovery never touches `sent` or `discarded`. List responses use
`EmailDraftSummary`, which has no `body` field at all.

## Commands

All commands are owner-only at the IPC gate. The read-shaped ones are in
`VIEWER_DENY`, so operators cannot read the SMTP configuration or drafts either.

| Command | Purpose |
|---|---|
| `get_agent_email` | Safe view: configured, host, port, security, username, from, allowlist, has_password. |
| `set_agent_email(input)` | Validate and save the account; write/keep/clear the password; return the view. |
| `clear_agent_email` | Remove the configuration and password. |
| `test_agent_email(to)` | Validate the recipient and allowlist, then send a fixed plain-text test in `spawn_blocking`. |
| `list_email_drafts` | Recovers interrupted sends older than ten minutes, then returns up to 100 summaries, newest first, no bodies. |
| `get_email_draft(id)` | The full draft, including its body. |
| `update_email_draft(id, to, cc, subject, body)` | Revalidate limits and allowlist; only while `draft`. |
| `discard_email_draft(id)` | Only while `draft`. |
| `send_email_draft(id)` | Only while `draft`; revalidate allowlist; claim `sending`; send plain text; mark sent; audit. Failures return the row to `draft`. |

## Tool contract

`email_draft` is `SkillRisk::Low` with no permissions and description
**“Create a reviewable email draft. Never sends.”**

Input:

- `to` — required, 1–20 comma/semicolon-separated addresses.
- `cc` — optional, 0–20 addresses.
- `subject` — 1–200 characters, no control characters.
- `body` — 1–50,000 characters; tabs and newlines allowed, other control
  characters (including NUL and carriage return) rejected.

Each address must contain exactly one `@` with text on both sides, contain no
whitespace or control characters, and be at most 320 characters. The result is
`{ draft_id, status: "draft", recipients, cc, sent: false, message }`.

If the run has no database attached, the tool fails with a clear error rather
than trying a global path.

## Audit and privacy

A successful send writes one `ToolCall` audit row named `email_send` with:

```json
{ "draft_id": "<uuid>", "recipient_count": 2 }
```

The row carries the draft's `bot_id`, `run_id` and `thread_id`. It never
contains the subject, body, recipient addresses or SMTP password. A failed send
writes a sanitized error to `email_drafts.error` (single line, ≤300 characters,
password replaced with `[redacted]`) and leaves the draft retryable.

## Limits

This slice does **not** provide:

- inbound email, IMAP or a mailbox;
- attachments, HTML bodies, inline images or rich text;
- automatic sending, scheduled sending, or an `email_send` agent tool;
- provider OAuth (Gmail/Graph API); only plain SMTP with STARTTLS or a loopback
  test server;
- a Mail/Settings UI (follow-up);
- per-agent SMTP accounts (one workspace account for now);
- bounce handling, delivery receipts or retries beyond pressing Send again.

The account is a single workspace-level SMTP identity. An allowlist narrows who
a draft may address; it is not a substitute for reviewing the draft.

## Evidence

- `cargo test -p ravenbot-db` covers create/list/get, body-free summaries, the
  atomic `draft -> sending` claim (including two concurrent callers), failed
  sends returning to draft, `sent` as terminal, and stale-send recovery.
- `cargo test -p ravenbot-skills` covers draft creation with run context,
  missing database, recipient/subject/body limits, allowlist rejection and a
  listener that proves `email_draft` opens no SMTP connection.
- `cargo test -p ravenbot --lib` covers the config rules, plain-text To/Cc
  message, password redaction, owner-only access, allowlist rejection before a
  connection, and a loopback SMTP send through the helper the send command
  uses.
- A real-provider SMTP check is manual; see
  [`LIVE-CHECKS.md`](LIVE-CHECKS.md).
