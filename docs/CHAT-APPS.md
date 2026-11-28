# Chat apps

Telegram, Slack, Discord, WhatsApp and Microsoft Teams are natively supported.

## Agent messaging tools

An agent can work with the chats you have already paired, without ever seeing a
token:

| Tool | What it does | Risk |
|---|---|---|
| `messaging_list_places` | Lists this agent's paired Telegram chats, Slack channels, Discord channels and WhatsApp numbers, with the connector's enabled state. Read-only. | Read-only |
| `messaging_history` | Reads the **local RAVENBOT conversation** for one paired place, newest last, with optional text filter and a 100-message cap. It is not the platform's full archive. External text is marked untrusted. | Read-only |
| `messaging_post` | Posts one plain-text message (1–8,000 characters) to a paired place. No attachments in v1. | High; approval-gated |

The tools are scoped to the **running agent**: a link belongs to the bot that
paired it. One agent cannot list, read or post to another agent's chat. The
desktop and headless daemon install the same sender, which re-checks the link and
the connector switch before using the existing client. A disabled connector, an
unpaired place, or a headless process with no sender fails closed.

Secure API-key requests are desktop-only. A paired chat is not offered the
password card and cannot answer it with `answer <id> <text>`; the notice asks the
owner to open RAVENBOT, because a platform transcript is not a secure input. See
[SECURE-API-KEYS.md](SECURE-API-KEYS.md).

WhatsApp free-form posts remain subject to Meta's 24-hour customer-service
window; the provider error is returned rather than replaced with a fake success.
`messaging_history` is local conversation history, so it does not fetch messages
that were never routed to RAVENBOT.

## Native Telegram

1. Create a bot with `@BotFather` and copy its token.
2. Settings → Chat apps → paste the token, choose the default agent, save, and
   enable polling. Or, headlessly:

   ```sh
   ravenbot telegram token      # reads the token from stdin
   ravenbot telegram status
   ```

3. Generate a pairing code and send `/start <code>` to the bot from Telegram.
   Headlessly:

   ```sh
   ravenbot telegram pair --bot "The Raven"
   ```

4. Chat commands:

   - `/start <code>` — pair this chat
   - `/new` — start a new conversation with the same agent
   - `/stop` — cancel the active run
   - `/status` — show the linked agent and active run count
   - `/unlink` — remove the pairing

The connector uses **long polling**, not a public webhook, so it needs no open
port. Only paired chat ids can start runs. The bot token is stored in the OS
keychain when available. A high-risk tool still parks under Review; the chat is
shown inline Allow/Deny buttons; the `approve <id>` / `deny <id>` commands remain a fallback. The polling loop
runs in the desktop process or the headless daemon, whichever owns the fleet.
Text and attachment messages are supported. Telegram photos/documents/audio
are downloaded, capped at 20 MB (images 10 MB) with at most four per message;
non-images are stored for the workspace but not sent to the model. Replies also
send assistant images/documents back to the chat. Edited messages and reactions
are ignored. A paired chat is limited to 30 handled messages per minute.

## Native Slack

Slack uses **Socket Mode**. Slack opens a WebSocket to RAVENBOT through the
app-level token, so no public request URL or open port is required.

1. Create a Slack app at api.slack.com/apps.
2. Enable **Socket Mode** and create an app-level token with `connections:write`.
3. Add bot scopes `chat:write`, `app_mentions:read`, `im:history`, `im:read`
   and, for channel use, `channels:history` (plus `groups:history` for private
   channels).
4. Subscribe to the `app_mention` and `message.im` bot events, install the app
   to the workspace, then copy the app-level token (`xapp-`) and bot token
   (`xoxb-`).
5. Settings → Chat apps → Slack, or headlessly:

   ```sh
   printf '%s\n%s\n' "$SLACK_APP_TOKEN" "$SLACK_BOT_TOKEN" | ravenbot slack tokens
   ravenbot slack status
   ```

6. Generate a pairing code and direct-message the bot, or mention it in the
   channel, with `pair <code>`. Prefer a direct message; a code posted in a
   channel is visible to its members:

   ```sh
   ravenbot slack pair --bot "The Raven"
   ```

Commands work in a direct message or when the bot is mentioned:

- `pair <code>` — pair this conversation
- `new` — start a new conversation with the same agent
- `stop` — cancel the active run
- `status` — show the linked agent and active run count
- `unlink` — remove the pairing

Only paired conversations start runs. Messages that neither mention the bot nor
arrive in a direct message are ignored, so a shared channel stays quiet.
Redelivered events are ignored by channel and timestamp, so a reconnect cannot
run the same message twice. The reply is posted in the same thread. The tokens are stored in the OS keychain
when available. Slack also parks high-risk tools under Review and posts Block Kit Allow/Deny
buttons; `approve <id>` / `deny <id>` remain available as text commands. The Socket Mode
loop runs in the desktop process or the headless daemon, whichever owns the
fleet.

Text messages are supported. Attachments, edits and other message subtypes are
ignored for now.

## Native Discord

Discord uses the **Gateway WebSocket**. RAVENBOT opens an outbound connection,
so no public interactions endpoint or signing secret is required.

1. Create an application at discord.com/developers/applications.
2. Open **Bot**, copy the bot token, and enable **Message Content Intent** under
   Privileged Gateway Intents. Without it, message text arrives empty.
3. Invite the bot with the `bot` scope and the **View Channels**, **Send
   Messages** and **Read Message History** permissions.
4. Settings → Chat apps → Discord, or headlessly:

   ```sh
   ravenbot discord token      # reads the token from stdin
   ravenbot discord status
   ```

5. Generate a pairing code and direct-message the bot, or mention it in a
   channel, with `pair <code>`. Prefer a direct message; a code posted in a
   channel is visible to its members:

   ```sh
   ravenbot discord pair --bot "The Raven"
   ```

Commands work in a direct message or when the bot is mentioned:

- `pair <code>` — pair this conversation
- `new` — start a new conversation with the same agent
- `stop` — cancel the active run
- `status` — show the linked agent and active run count
- `unlink` — remove the pairing

Only paired conversations start runs. A guild channel message is ignored unless
it mentions the bot, so unrelated chatter stays quiet. A reconnect resumes the
gateway session, and redelivered messages are ignored by channel and message
id. Replies disable mentions to avoid pinging
users by accident. The token is stored in the OS keychain when available.
High-risk runs still park under Review; Discord posts message-component Allow/Deny buttons; the text commands remain
available as a fallback. The gateway loop runs in the desktop process or
the headless daemon, whichever owns the fleet.

Text messages and Slack file uploads are supported. Files are downloaded with
the bot token, capped at 20 MB (images 10 MB) with at most four per message;
non-images are stored for the workspace but not sent to the model. Replies
upload assistant images/documents back through Slack's external upload flow
(`files:write`). Embeds and edited messages are ignored. A paired conversation is limited to 30 handled
messages per minute.

## Native WhatsApp

WhatsApp uses the **Cloud API**. Meta delivers inbound messages to a public
HTTPS webhook, so this connector is the one native chat adapter that needs an
externally reachable URL. RAVENBOT listens on loopback and expects a reverse
proxy or tunnel in front of it.

1. Create a Meta app, add the **WhatsApp** product and connect a business
   phone number.
2. Copy the **phone number id** and a Cloud API access token. For anything
   beyond testing, use a system-user token with `whatsapp_business_messaging`.
3. Copy **App settings → Basic → App secret**.
4. Choose a verify token. It can be any unguessable string; the same value goes
   into Meta's webhook configuration.
5. Expose the local listener through HTTPS. The default is:

   ```
   https://your-host/whatsapp  ->  http://127.0.0.1:8801/whatsapp
   ```

   Meta does not accept a self-signed certificate. Use a trusted reverse proxy,
   a tunnel, or a certificate from a public CA. Set the webhook URL and verify
   token in Meta, then subscribe to the `messages` field.
6. Settings → Chat apps → WhatsApp, or headlessly:

   ```sh
   printf '%s\n%s\n%s\n' "$WA_TOKEN" "$WA_APP_SECRET" "$WA_VERIFY_TOKEN" |
     ravenbot whatsapp tokens --phone-id 1234567890
   ravenbot whatsapp status
   ```

7. Generate a pairing code and send `pair <code>` from WhatsApp to the
   business number:

   ```sh
   ravenbot whatsapp pair --bot "The Raven"
   ```

Commands are plain text:

- `pair <code>` or `start <code>` — pair this number
- `new` — start a new conversation with the same agent
- `stop` — cancel the active run
- `status` — show the linked agent and active run count
- `unlink` — remove the pairing

Every POST is verified with `X-Hub-Signature-256` using the app secret before
the body is parsed. The GET challenge is checked against the verify token. Only
paired numbers start runs. Meta retries are ignored by message id.

The Cloud API customer-service window applies: replies to a user who just wrote
are allowed, but a proactive approval notice may fail if the 24-hour window has
closed. Text and media messages are supported. Images, documents, audio and
video are downloaded from the Graph media endpoint, capped at 20 MB (images
10 MB); captions are kept, and non-images are stored but not sent to the model.
Replies upload assistant media through the Graph media endpoint. Reactions,
edits and status updates are ignored. A paired number is limited to
30 handled messages per minute.

The local listener can be moved with `RAVENBOT_CHAT_WEBHOOK_BIND` and
`RAVENBOT_CHAT_WEBHOOK_PORT` (default `127.0.0.1:8801`).

## Native Microsoft Teams

Teams uses the **Bot Framework**. Azure delivers activities to the bot's HTTPS
messaging endpoint, and RAVENBOT validates every Bot Framework JWT before it
routes a message.

1. Create an **Azure Bot** (multi-tenant) and copy the Microsoft App ID.
2. Under **Manage Password**, create a client secret. That is the app password.
3. Enable the **Microsoft Teams** channel on the bot.
4. Expose the loopback listener through HTTPS and set it as the Azure Bot
   **messaging endpoint**:

   ```
   https://your-host/teams  ->  http://127.0.0.1:8801/teams
   ```

   The certificate must be trusted by Teams. Use a reverse proxy or tunnel.
5. Add the bot to a Teams app manifest, set `"supportsFiles": true`, and
   install it in the personal, team or group-chat scope you need. Outbound
   files use the Bot Framework file-consent flow, which requires that flag.
6. Add one or more **tenant GUIDs** to the allowlist in RAVENBOT. Requests from
   any other tenant are rejected.
7. Settings → Chat apps → Microsoft Teams, or headlessly:

   ```sh
   printf '%s\n' "$TEAMS_APP_PASSWORD" |
     ravenbot teams configure --app-id 11111111-1111-1111-1111-111111111111 \
       --tenants 22222222-2222-2222-2222-222222222222
   ravenbot teams status
   ```

8. Generate a pairing code and send `pair <code>` in a personal chat, or
   mention the bot in a channel:

   ```sh
   ravenbot teams pair --bot "The Raven"
   ```

Commands work in a personal chat or when the bot is mentioned:

- `pair <code>` — pair this conversation
- `new` — start a new conversation with the same agent
- `stop` — cancel the active run
- `status` — show the linked agent and active run count
- `unlink` — remove the pairing

Every request is checked against the Bot Framework OpenID metadata: the token
must be RS256, signed by a published key, issued by
`https://api.botframework.com`, addressed to the configured Microsoft App ID,
and its `serviceurl` claim must match the activity's `serviceUrl`. The tenant is
checked against the allowlist after the token is valid. Duplicate activity ids
are ignored.

Bot replies use the Bot Framework connector with a cached client-credentials
token. The default token endpoint is the multi-tenant
`botframework.com` endpoint; a single-tenant bot can override it with
`RAVENBOT_TEAMS_TOKEN_URL`. A sovereign cloud can also override
`RAVENBOT_TEAMS_OPENID_URL` and `RAVENBOT_TEAMS_ISSUER`.

Text messages and Teams attachments are supported. Inline base64 files and Bot
Framework content URLs are downloaded, capped at 20 MB (images 10 MB) with at
most four per message; non-images are stored for the workspace but not sent to
the model. Outbound replies post a Teams file-consent card for each assistant
file (up to four); when the user clicks **Allow**, RAVENBOT uploads the bytes in
4 MB chunks and posts the file info card. A declined or expired consent sends
nothing. Cards, reactions and conversation updates are ignored. A paired
conversation is limited to 30 handled messages per minute. A high-risk tool
still parks under Review; Teams posts an Adaptive Card with Allow/Deny and
option actions; the text commands remain available as a fallback.

## MCP providers

Other services are also available through their existing MCP servers. RAVENBOT does not need a separate OAuth app for each
service: you create the bot/app with the provider, paste its token into the
Connector Center, and assign the connector to an agent.

## Setup

1. Open Settings → Connectors.
2. Use the **Chat apps** preset (Slack, Discord, Teams, Telegram, WhatsApp) or
   search for the provider.
3. Paste the bot token. The connector row explains where each token comes from.
4. Enable the connector for the agents that should use it.
5. Keep the agent's approval mode on `Ask` or `Auto` for the first runs.

## Security

- Use a dedicated bot/app token, not a personal user token.
- Grant only the scopes the agent needs (read a channel, post a message). Do not
  grant workspace-admin scopes.
- Treat a channel message as untrusted input. The agent still goes through the
  same capability gate, network grant, approval ladder and budget as any other
  turn.
- Tokens are stored through the existing connector credential path; they are not
  placed in prompts.
- A connector can post as the bot. It cannot impersonate a human user.

## What is not native

The MCP-backed connectors remain outbound integrations: an agent calls the
connector's tools during a run. They do not replace the five native adapters,
and there is no per-provider rich message builder yet.

The **Chat apps** preset is the discoverable entry point; the underlying
connector implementations live in `crates/mcp/src/servers.rs`.
