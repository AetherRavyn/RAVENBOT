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
