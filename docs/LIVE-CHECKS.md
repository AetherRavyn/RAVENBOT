# Live and platform checks

The normal test suite runs without network credentials and without a graphical
session. These are the opt-in checks that need real accounts or a real OS.

## Live provider checks

`src-tauri/tests/live_providers.rs` contains ignored tests. Each test skips when
its environment variable is absent and never writes a chat message.

| Provider | Variables | Check |
| --- | --- | --- |
| Telegram | `RAVENBOT_LIVE_TELEGRAM_TOKEN` | `getMe` |
| Slack | `RAVENBOT_LIVE_SLACK_BOT_TOKEN` | `auth.test` |
| Discord | `RAVENBOT_LIVE_DISCORD_TOKEN` | `/users/@me` |
| WhatsApp | `RAVENBOT_LIVE_WHATSAPP_TOKEN`, `RAVENBOT_LIVE_WHATSAPP_PHONE_ID` | phone-number lookup |
| Teams | `RAVENBOT_LIVE_TEAMS_APP_ID`, `RAVENBOT_LIVE_TEAMS_APP_PASSWORD` | Bot Framework client-credentials token |
| Attachments | `RAVENBOT_LIVE_ATTACHMENT_URL` | download, size cap and local storage |

Run one provider:

```sh
RAVENBOT_LIVE_TELEGRAM_TOKEN=... \
  cargo test -p ravenbot --test live_providers telegram_bot_token_is_valid \
  -- --ignored --nocapture
```

Run every configured provider at once:

```sh
RAVENBOT_LIVE_TELEGRAM_TOKEN=... \
RAVENBOT_LIVE_SLACK_BOT_TOKEN=... \
RAVENBOT_LIVE_DISCORD_TOKEN=... \
RAVENBOT_LIVE_WHATSAPP_TOKEN=... \
RAVENBOT_LIVE_WHATSAPP_PHONE_ID=... \
RAVENBOT_LIVE_TEAMS_APP_ID=... \
RAVENBOT_LIVE_TEAMS_APP_PASSWORD=... \
RAVENBOT_LIVE_ATTACHMENT_URL=https://example.com/test.png \
  cargo test -p ravenbot --test live_providers -- --ignored --nocapture
```

Missing variables produce a skip message, not a failure. Tokens are read from
the environment and are never printed.

## Manual chat-card check

The card flows need a person to click a button. Use a test workspace and a test
bot, not a production channel:

1. Pair a Telegram DM, Slack DM/channel, Discord DM/channel, Teams personal
   chat or WhatsApp number.
2. Trigger an action that parks under **Review**.
3. Confirm the notice is a native card:
   - Telegram: inline keyboard
   - Slack: Block Kit buttons
   - Discord: message components
   - Teams: Adaptive Card
   - WhatsApp: reply buttons
4. Click Allow or Deny; confirm the run resumes and the decision appears in
   RAVENBOT.
5. For a question with options, click one option. For a free-text question,
   send `answer <id> <text>`.
6. Confirm a second click cannot decide the same request twice.

## Manual proactive-messaging check

CI covers the bridge contract, per-agent link scoping, send-once and the
connector-disabled failures. A release should also send one real message to a
test workspace:

1. Pair a test Telegram DM, Slack channel, Discord channel or WhatsApp number
   with a test agent, and enable that connector in Settings → Chat apps.
2. Ask the agent to call `messaging_list_places`. Confirm it lists only that
   agent's places, with the connector switch, and no token or other agent's
   chat.
3. Ask for `messaging_history` on the place. Confirm it returns the local
   RAVENBOT conversation newest last, says it is not the platform archive, and
   marks external text as untrusted.
4. Ask for `messaging_post` with a short message. Under Review, confirm the
   approval card names the place and text. Approve it once and confirm exactly
   one plain-text message arrives, with no attachment or duplicate.
5. Ask to post to a place paired with another agent and to an unpaired id.
   Confirm both are refused before any network call.
6. Disable the connector and retry. Confirm the post fails with the Settings
   message, then re-enable it.
7. For WhatsApp, try outside Meta's 24-hour customer-service window. Confirm the
   provider error is shown; it must not claim the message was sent.
8. Repeat one post from the headless daemon (desktop closed) and confirm it
   still uses the same link and connector.

## Manual browser DOM depth check

Use a local test page with a select, a checkbox, a hover target and a result that
appears after a delay:

1. Open the fixture in the browser dock and call `browser_dom` action `snapshot`.
2. Select a visible option with `select_option`; confirm the page's change
   handler runs. Try a single-select with two values and confirm it is refused.
3. Toggle a checkbox with `set_checked`; confirm the native click path runs the
   page's handler and the returned checked state matches.
4. Hover the target and confirm a script-visible hover handler runs (CSS `:hover`
   itself may not apply to synthetic events).
5. Call `wait_for` with the delayed result's text or selector; confirm it waits
   and returns `matched: true`. Use a missing condition and a short timeout and
   confirm the failure is honest.
6. Run a `run_steps` list that fills, selects, checks and waits; confirm the steps
   run in order and stop at a deliberately broken selector.
7. Try `run_steps` with `eval`, `open` or a nested `run_steps`; confirm each is
   refused before evaluation. Try 201 steps and confirm the cap.
8. Press **Take over** and run any action; confirm it is blocked until release.

## Manual secure API-key check

1. In a normal desktop conversation, ask the agent to call `save_api_key` with
   `{"name":"openai-test","label":"OpenAI test key","purpose":"Live check"}`.
2. Confirm the card shows a password field, the purpose and **Save key**, with no
   list of other Vault items and no text box that would send the value as an
   answer.
3. Enter a clearly fake value such as `sk-live-check-not-real` and save.
4. Confirm the run resumes and the agent receives only `name`, `label` and
   `path`. The value must not appear in the transcript, the tool card, the
   audit log or the model response.
5. Inspect the stored row and file:
   - `vault_items.kind = 'api_key'`, `key_name = 'openai-test'`, and the answer
     column contains `{name, path}` but not the value;
   - `~/.ravenbot` is not used: the file is under
     `$RAVENBOT_HOME/keys/api-keys/openai-test` (default
     `~/RAVENBOT/keys/api-keys/openai-test`);
   - `stat -c '%a %n'` shows `700` for `api-keys` and `600` for the file;
   - `cat` the file once to confirm it contains the fake value, then do not paste
     it into the conversation.
6. Call `list_api_keys`; confirm it returns the name, label, path and
   `file_exists: true`, and no value. Use the returned path in a harmless
   command, for example
   `wc -c < "$HOME/RAVENBOT/keys/api-keys/openai-test"` (this prints a length,
   not the key).
7. Call `remove_api_key` with the same name; confirm the row and file are gone
   and a second call reports that no key is stored.
8. From a paired Telegram/Slack/Discord chat, trigger an API-key request. Confirm
   the chat says to open RAVENBOT and that `answer <id> <text>` is refused.
9. As a non-owner workspace role, confirm `submit_api_key_answer` is refused.

## Manual voice-key check

Transcription and speech both prefer the OpenAI key saved in Settings, then
`OPENAI_API_KEY` for headless runs:

1. Save an OpenAI key in Settings and unset `OPENAI_API_KEY` in the shell that
   launched RAVENBOT.
2. Record a short phrase in voice mode; confirm it transcribes and that the
   saved key was used.
3. Remove the saved key and try again; confirm the error names Settings,
   `OPENAI_API_KEY`, or local faster-whisper instead of claiming success.
4. Confirm neither the UI nor the logs print the key.

Whisper calls are paid; use a short recording and a test key. Set
`RAVENBOT_OPENAI_AUDIO_URL` to a compatible transcription endpoint when routing
through a gateway or proxy.

## Manual hands-free voice-mode check

This needs a real microphone and a graphical session:

0. On Linux/PipeWire, confirm the default source is not muted:
   `wpctl status`, then `wpctl set-mute @DEFAULT_AUDIO_SOURCE@ 0` if needed.
   A muted source records silence even though capture succeeds.
1. Open a thread, grant microphone permission, and enable voice mode.
2. Say a short phrase and stop speaking. Confirm the recorder stops after about
   1.5 s of silence (or at the 25 s cap) and the phrase appears as your message.
   With no OpenAI key, this uses local faster-whisper; if PyAV 19 is installed,
   the embedded script decodes through ffmpeg.
3. Confirm the agent replies. If Piper, Hugging Face, OpenAI or the local
   SanoTTS model is configured, confirm the reply is spoken and listening
   resumes without another click. With no TTS engine, confirm the text reply
   still appears and the missing-engine error is logged instead of pretending
   to speak.
4. Turn voice mode off with the composer control; confirm the active recording
   stops, any playback stops, and the microphone indicator goes away.
5. Deny microphone permission and try again; confirm the app reports that voice
   mode needs microphone access rather than showing a fake listener.

This is a chat voice loop. It does not join a meeting or record a call.

## Manual meeting-recording import check

Use a short, non-sensitive recording because OpenAI Whisper is paid:

1. Prepare a 5–30 s audio file in each format you intend to claim: WAV, MP3,
   OGG, M4A, WebM and FLAC. Include one `.WAV` uppercase extension.
2. Open a thread and click the meeting-notes button in the composer. Confirm the
   dialog states the limits and that Transcribe and Use in composer are disabled
   before a file is chosen. Choose a file in the picker and confirm only the
   basename is shown.
3. Transcribe. Confirm the returned preview is readable, the character count
   matches the transcript, `duration_secs` is plausible, and a short recording
   shows no truncation warning. Confirm the dialog does not send anything.
4. Click Use in composer. Confirm the prompt appears in the composer, the thread
   draft is saved, focus returns to the composer, and no message was sent. Check
   the prompt contains a summary, decisions, action items with `owner`/`due`,
   open questions, the untrusted-data warning, the file name, and the begin/end
   delimiters.
5. Try a directory, a missing path, an MP4, a `.txt` file, a file under 100
   bytes, and a file over 25 MiB. Confirm each is refused with its specific
   error and that no transcription call is made.
6. With a saved OpenAI key and `OPENAI_API_KEY` unset, confirm Whisper uses the
   saved key; remove both keys and confirm the dialog names Settings,
   `OPENAI_API_KEY`, or local faster-whisper. Confirm the key never appears in
   the UI, logs or returned error.
7. Confirm the command is denied for a viewer and allowed for an operator.
8. Do not record a pass for live meeting join, diarization, video import, phone
   calls or automatic sending: those are not implemented.

## Manual Teams outbound-file check

Teams file delivery needs a real user click and an app manifest with
`"supportsFiles": true`:

1. Ask a paired agent to take a screenshot or write a file.
2. Confirm the reply includes a file-consent card with the file name and size.
3. Click **Allow**; confirm the file-info card appears and the downloaded file
   matches the artifact.
4. Repeat and click **Decline**; confirm nothing is uploaded and the chat says
   the file was not sent.
5. Confirm a second click on an old card cannot upload the same file again.

## Manual agent email check

Agent email is covered in CI by an in-process loopback SMTP server: creating a
draft never connects, an allowlist rejection happens before any connection, and
an explicit send through the path `send_email_draft` uses reaches the local
server with the expected To, Cc, subject and plain-text body. The Mail panel
suite covers account save, confirmations, draft editing, status transitions and
failed-send recovery. A release should also exercise one real SMTP account
through **Settings → Mail**:

1. Use a test mailbox and a test SMTP credential, not a production inbox.
2. As the workspace owner, open **Settings → Mail**. Fill the host, port,
   StartTLS, username, password and sender; set a narrow allowlist (for example
   one address); then press **Save account**. Confirm the badge is
   **Configured**, the password field is blank again, and the saved-password
   hint is visible.
3. Set **Test recipient** to an allowlisted address and press **Send test
   email**. Confirm the confirmation appears before anything is sent, exactly
   one plain-text test message arrives, and it has no attachment or HTML part.
4. Change the test recipient to an address outside the allowlist and press the
   button again. Confirm it fails with a clear allowlist error and the SMTP log
   shows no connection.
5. Ask an agent to use `email_draft` for an allowlisted address. Confirm the
   reply says the draft was saved and nothing was sent, and that the mailbox
   receives nothing.
6. Open the draft in the Mail list. Confirm the row shows the status, To,
   subject and created time; the editor shows the full body; the list response
   itself has no body.
7. Press **Send**, check the confirmation shows the recipients and subject,
   then confirm. One message arrives, the draft becomes **Sent** with a sent
   time, a second send is disabled, and the audit viewer shows `email_send`
   with only a draft id and recipient count.
8. Configure a second draft with a deliberately bad SMTP password, press
   **Send**, and confirm the draft returns to **Draft** with a short error that
   does not contain the password or the body; fix the account and retry.
9. While a send is in flight, confirm the draft shows **Sending…** and its
   edit/send/discard controls are disabled. A crashed send is recovered by the
   next draft-list refresh after ten minutes with the verify-before-retry
   warning.

See [`AGENT-EMAIL.md`](AGENT-EMAIL.md).

## Manual commerce check

CI covers the tool, policy validation, transactional daily limit, owner gates
and the Commerce panel. A release should also exercise one real external
checkout handoff with a merchant page you control; do not complete a purchase
unless it is a sandbox cart.

1. As the workspace owner, open **Settings → Commerce**. Confirm commerce is off
   by default and the panel says RAVENBOT never checks out or stores card data.
2. Ask an agent to use `spend_request` while commerce is off. Confirm it fails
   with a Settings → Commerce message and the request list stays empty.
3. Set currency `USD`, a per-request limit of `2000`, a daily limit of `5000`,
   and allowlist `example.com`. Save and confirm the saved values come back.
4. Ask for a `1500` request at `https://shop.example.com/checkout/test`. Confirm
   the reply says **pending**, **approved: false**, **purchased: false**, and the
   panel shows the title, merchant, amount in minor units, reason and URL.
   Nothing opens and nothing is purchased.
5. Ask for `2500` and for `https://other.example/checkout`; confirm both are
   refused (per-request limit and merchant allowlist) and no row is added.
6. Approve the `1500` request. Confirm the status becomes **Approved**, the
   browser does not open automatically, and the audit viewer gets an
   `ApprovalDecision` row. Press **Open checkout** and confirm your browser opens
   the exact stored HTTPS URL while the app window stays put.
7. With `1500` already approved today, approve a second `4000` request. Confirm
   it is refused with the committed total and the daily limit, and remains
   **Pending**.
8. Use a sandbox cart to complete checkout, then press **Mark purchased** and
   confirm the status and purchase time. Separately, deny one request and
   cancel another; confirm each is terminal and cannot be approved afterwards.
9. Confirm the card shows no card number, CVV, expiry, order id, tax, shipping
   or refund fields, and that no agent tool can approve or purchase.
10. Sign in as an operator and a viewer; confirm neither can read or change
    commerce settings or requests.

See [`COMMERCE.md`](COMMERCE.md).

## Manual enterprise export check

The export loop is covered by an in-process collector in CI. A release should
also exercise the real Settings card against a collector you control. Nothing is
sent until the card is enabled or **Export now** / **Test connection** is pressed.

1. Start a throwaway collector that records OTLP bodies:

   ```sh
   python3 - <<'PY'
   from http.server import BaseHTTPRequestHandler, HTTPServer

   class Handler(BaseHTTPRequestHandler):
       def do_POST(self):
           body = self.rfile.read(int(self.headers.get("content-length", 0)))
           with open("/tmp/otlp-body.json", "ab") as out:
               out.write(body + b"\n")
           self.send_response(200)
           self.end_headers()

       def log_message(self, *args):
           pass

   HTTPServer(("127.0.0.1", 4318), Handler).serve_forever()
   PY
   ```

2. In Audit log → **Enterprise export**, set the endpoint to
   `http://127.0.0.1:4318`, add a header such as
   `Authorization: Bearer test-token`, and press **Test connection**. Confirm the
   card reports success and `/tmp/otlp-body.json` contains `ravenbot.export.test`.
3. Trigger a tool call, then press **Export now**. Confirm the audit record has
   the kind, bot, tool name and ids, and that the request contains no prompt,
   thread title, approval note, tool arguments, URL query string or run
   result/error text.
4. Stop the collector and press **Export now**. Confirm the card shows an error,
   neither cursor advanced, and no header value appears in the error.
5. Start the collector again and press **Export now**. Confirm the same event is
   delivered. A crash between the two cursor writes can duplicate a batch;
   deduplicate on `audit.id` / `run.id`.
6. Clear and save the headers. Confirm the card says headers are not configured
   and the collector receives no `Authorization` header.
7. If the OS keychain is unavailable, confirm the card still reports whether
   headers exist and delivery either uses the sealed-vault fallback or fails
   honestly.

Record the collector version (or the script above), the exported counts and
whether the exclusion checks passed in the release notes.

## Platform contracts

Linux CI runs `systemd-analyze verify` against the generated hardened unit and
executes the live Docker sandbox test with `alpine:3.20`. It does not perform a
full systemd install/start/stop lifecycle in a hosted runner; that remains a
manual check. The `platform-contracts` job runs on `macos-15-intel` and
`windows-latest`:

- **macOS** executes the service/account/environment contracts and the live
  Seatbelt test. The test reads a marker inside the workspace, asserts a real
  file outside it cannot be read, asserts https egress is blocked, and bounds
  the command so a bad profile fails instead of hanging. macOS 26 needs three
  things the older profile missed: the root directory must be readable, dyld
  needs `file-map-executable`, and the per-user process/address-space `ulimit`s
  must be left alone (the dyld shared cache counts against `RLIMIT_AS`, and
  `RLIMIT_NPROC` is per user, not per sandbox).
- **Windows** compiles the Tauri contracts and **runs** the Tauri-free
  `ravenbot-sandbox` contract, which pins the fail-closed refusal. It cannot
  execute the Tauri lib test harness: a Windows test binary has no application
  manifest, so `comctl32` v6 never loads and the process exits with
  `STATUS_ENTRYPOINT_NOT_FOUND` before a test starts (tauri-apps/tauri#14580).

The job does not install a LaunchAgent or a Task Scheduler task in a hosted
runner; those remain manual checks:

- macOS: `ravenbot service install`, `start`, `status`, `stop`, `uninstall`
  with a throwaway `RAVENBOT_HOME`, then confirm the LaunchAgent appears in
  `launchctl print gui/<uid>/dev.ravenbot.background`.
- Windows: install the per-user task, sign out and back in, confirm the daemon
  runs hidden, then uninstall.

Record the OS build, the command output and whether the daemon survived a
crash/restart in the release notes.

## Manual OIDC SSO check

The remote SSO suite uses a loopback mock IdP. That is CI evidence, not proof
against a real provider. Run this pass before calling remote SSO live-verified.
Use a test workspace, a public/native OIDC client and a throwaway account.

1. Register `https://<host>/api/sso/callback` as a public client with
   authorization-code + PKCE and no client secret. Confirm the provider issues
   RS256 ID tokens and exposes `issuer`, `authorization_endpoint`,
   `token_endpoint` and `jwks_uri` in discovery.
2. Open **Settings → Remote → Single sign-on (OIDC)** as the workspace owner.
   Enter the exact issuer (no trailing slash), client id, redirect URL,
   `openid profile email` and the claim that holds the local RAVENBOT username,
   then enable SSO and save. Confirm the form keeps the returned values. Saving
   does not contact the provider and there is no test-connection button; the
   first request happens in step 4.
3. Start remote access over HTTPS or a private tunnel. Pair a phone. Confirm the
   login page shows **Sign in with SSO** and that the username/password form is
   still visible and can sign in.
4. Press SSO, authenticate at the provider, and confirm the browser returns to
   the redirect origin with `?sso=ok`, enters the chat, and `/api/status` reports
   the mapped local account. Confirm the query is removed from the address bar.
5. Confirm a `remote_audit` row has method `SSO`, path `/api/sso/callback`,
   status `200`, the device name, account name and effective role. Confirm no
   authorization code, access token, ID token, nonce or verifier appears in the
   audit table, database text columns or logs.
6. Re-load the callback URL from step 4. Confirm it shows the generic error and
   does not bind again (state replay).
7. Cancel at the provider. Confirm the page shows the generic error, not the
   provider's `error_description`.
8. Sign in with an IdP identity that has no RAVENBOT account, then with an
   account that is disabled, then with an account whose workspace grant was
   revoked. Each must show the same generic `reason=account` error and leave the
   device unbound.
9. With a paired device, alter `remote_sso_redirect_url` to a different origin
   that still ends in `/api/sso/callback`, restart remote access, and repeat a
   callback while spoofing `Host`. Confirm the `Location` origin follows the
   configured URL, not `Host`.
10. Record the provider, issuer, claim, account roles, audit rows and the
    browser/OS build in the release notes. Do not claim SCIM Groups, group
    mapping, auto-provisioning or OIDC logout.

## Manual real-IdP SCIM check

The SCIM suites use a loopback listener, a mock provisioner and direct
account-registry calls. That is CI evidence, not proof against a real identity
provider. Run this pass before calling SCIM live-verified. Use a test workspace,
a throwaway member account and an IdP that can reach the listener (HTTPS or a
private tunnel with a certificate the IdP trusts).

1. Start remote access over HTTPS and confirm the IdP can reach
   `https://<host>/scim/v2/Users`. A loopback-only listener is not enough.
2. Open **Settings → Remote → SCIM provisioning** as the workspace owner. Choose
   the default role, press **Generate token**, copy the one-time plaintext, then
   enable SCIM and save. Confirm the token is shown only in the read-only field,
   the status shows enabled/active, and no token or hash appears in the card.
   Use **Replace token** and confirm the old bearer token now gets `401` while
   the new one works. Restart the app and confirm the token still works.
3. At the IdP, create a SCIM 2.0 **Users** integration with the listener base URL
   and bearer token. Do not enable Groups or bulk. Push one user whose
   `userName` follows RAVENBOT's rules (3–64 characters, starts alphanumeric,
   letters/numbers/`.`/`_`/`-`; no `@`).
4. Confirm the user appears in Settings → Team with the configured default role,
   not owner. Confirm a second push of the same `userName` or `externalId`
   returns a conflict and does not create a duplicate.
5. Confirm the new account has no usable password: attempt text login and expect
   failure. Sign in through the separately configured OIDC SSO path and confirm
   the account resolves to the SCIM `userName`.
6. Rename the user at the IdP and confirm `GET`/list reflects the new name and
   the old name is gone. Disable the user at the IdP and confirm `active: false`,
   the member is disabled, and an existing session/device fails closed within the
   revalidation window. Re-enable and confirm the same account returns.
7. Delete the user at the IdP. Confirm the response is `204`, the account remains
   present but disabled, and its data root and workspace grant are untouched.
   Reactivate it with `PATCH` `active: true` or a `PUT` and confirm the same
   account id and external-id mapping return. A fresh `POST` with the same
   `userName` is expected to return `409` (duplicate by design).
8. Confirm the workspace owner is absent from the SCIM list and that pushing the
   owner's username returns `403` `mutability`. Confirm a SCIM token for another
   workspace cannot see or change this workspace's members.
9. Grant the same member to a second workspace. Confirm SCIM rename and delete
   now return `403` `mutability` and the host-level account is unchanged.
10. Send a wrong token (expect `401` with `WWW-Authenticate: Bearer`). After a
    successful read, confirm the card shows a **Last used** time. Press
    **Revoke token**, confirm the warning, and expect every `/scim/v2` path to
    return `404` even with the old token; the card shows no token and **Never
    used**. Confirm `remote_audit` has `SCIM` rows for create/patch/put/delete
    and auth failures.
11. Confirm `/api/status` and the mobile page expose no SCIM fields and that a
    paired-device token cannot call `/scim/v2`.
12. Record the provider, SCIM base URL, default role, audit rows, list/filter
    behaviour and the browser/OS build. Do not claim Groups, bulk, sort, role
    mapping, auto-provisioning without SCIM, or live evidence for a provider you
    did not test.

## Manual OpenBot avatar check

CI renders the still fallback in jsdom. Canvas, materials and live transitions
need a real display, so a release should check them by hand:

1. Open an agent's avatar picker and choose **OpenBot character**. Confirm the
   studio lists 23 characters and the strip shows Idle, Thinking, Working,
   Waiting, Blocked and Done in that order.
2. Choose a character and colour, set volume, pointer-follow and a finish, then
   apply. Confirm the agent shows the character, and reopening the picker
   restores the same options.
3. Run a task in a thread. Confirm the face moves through thinking/working,
   shows waiting while an approval is queued, blocked after a failed run and
   done after a completed run. The sidebar, office header, chat gutter and
   thread header should agree.
4. Start a plan with one finished and one failed node. Confirm each node's face
   reflects its own state rather than one fleet-wide mood.
5. Turn on the OS reduced-motion setting. Confirm the character stops moving
   while the pose still changes with the work state.
6. Try flat, fluffy and glass. On a machine without WebGL2, confirm fluffy and
   glass draw flat instead of showing a broken body.
7. Reload the app. Confirm the character, colour, volume, pointer-follow and
   finish all return.

Record the display/WebKit version, the states observed, and whether reduced
motion and the material fallback behaved.

## Browser Teach and Replay

The recorder and replay assets run in jsdom in CI, but the WebKit dock needs a
real display. This is the manual acceptance pass for a release:

1. Serve a local fixture with a text field, a checkbox, a select and a submit
   button over `http://127.0.0.1:<port>` (the dock accepts http/https only).
2. Open the fixture in the browser dock and press **Teach**. Fill the text
   field, choose a select option, tick the checkbox, submit, and press **Stop**.
3. Confirm the review dialog lists those steps, the values are present for
   non-sensitive fields, and **Save & install** writes a skill with a
   `demo.json` companion.
4. Press **Replay** from Skills. Confirm the form is filled and submitted, the
   report says `ok`, and the page URL is the post-submit URL.
5. Change the fixture's selector (keep the label), replay again, and confirm
   the report counts a repaired step instead of claiming the old selector.
6. Remove the control entirely, replay, and confirm the report names the page,
   the attempted selectors and a repair hint, and the skill was not modified.
7. Add an `input[type=password]` to the fixture, record a fill, and confirm
   `demo.json` has no value. Replay must stop with `needs_input` and must not
   type an empty value.
8. Record a task that navigates to a second page. Confirm the replay includes
   the `navigate` step and stops honestly if an event in the last 200 ms was
   lost. Re-record if the review shows a missing step.

Record the app build, page URLs and replay report in the release notes. Do not
call the feature live-verified until this pass succeeds on a real display.
