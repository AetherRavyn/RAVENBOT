# Secure API keys

An agent can ask for an API key without the value entering the model's
context. The owner types it into a secure card, RAVENBOT stores it in the local
Vault, writes an owner-only file, and the agent receives only a name and a path.

This is the coding-task companion to the login Vault request. It is local-first
and has no provider account, billing or cloud key escrow.

## The flow

1. The agent calls `save_api_key` (or `ask_user` with a `vault`/`api_key`
   payload) with a machine-safe name and an optional label, website and purpose.
2. The run parks. The desktop card shows a password field, the purpose and a
   Save key button. There is no list of other secrets and no way to submit a
   value as a chat message.
3. The owner enters the key. The save path calls `submit_api_key_answer`, which
   validates the value, stores it in the Vault, writes the file and records an
   answer containing only:

   ```json
   {
     "action_id": "saved",
     "values": {
       "name": "openai",
       "path": "/home/you/RAVENBOT/keys/api-keys/openai"
     }
   }
   ```

4. The run resumes. The tool result contains that metadata, never the value.

The value is not written to the question row, the transcript, the audit log,
the streamed tool card or the model request. Tool arguments for `save_api_key`
are redacted before they are emitted or audited, so a model that mistakenly
puts a key in the call still cannot leak it through those channels.

## Tools

| Tool | Access | What it does |
|---|---|---|
| `save_api_key` | Runtime card | Asks for `name`, `label?`, `website?`, `purpose?`; never accepts the value. |
| `list_api_keys` | Read-only | Returns `name`, `label`, `website`, `path`, `file_exists` and timestamps. Never the value. |
| `remove_api_key` | High risk | Deletes the Vault row and the owner-only file. |

`ask_user` with `kind: vault` and `payload.item_kind: api_key` is the same card
without the convenience wrapper:

```json
{
  "question": "Enter an OpenAI key so I can call the models API.",
  "header": "OpenAI API key",
  "kind": "vault",
  "payload": {
    "item_kind": "api_key",
    "name": "openai",
    "title": "OpenAI API key",
    "site": "https://platform.openai.com",
    "placeholder": "sk-…",
    "submit_label": "Save key"
  }
}
```

A key name is lowercase ASCII letters, digits, `.`, `_` or `-`, starts with a
letter or digit, is at most 64 characters, and cannot be a path, a leading-dot
name, a trailing-dot name or a Windows device name.

## Where it lives

- **Vault row**: `vault_items.kind = 'api_key'`, with `key_name` set to the
  machine-safe name. The value is stored in the same sealed `secret` column as
  a login: the OS keychain when available, otherwise the encrypted fallback
  under `config/sealed-secrets`. If neither can be used, the existing Vault
  fallback keeps the value in the owner-only database (`0600` on Unix).
- **Agent file**: `~/RAVENBOT/keys/api-keys/<name>` (or the same path under
  `RAVENBOT_HOME`). The directory is `0700`, the file is `0600`, and the write
  is atomic: a temporary `0600` file is renamed over the destination. A
  symlinked key directory is refused.

The file is a plaintext copy by design. It exists so a command can read the
value at the moment it is used without printing it into the conversation. It is
readable by your user account — the same account the agent runs as. It is not
readable by other users on the machine.

Linux bubblewrap and the Docker sandbox mount `keys/api-keys` read-only, and
the macOS Seatbelt profile allows reads but not writes. An agent can read a key
with `cat`, but it cannot rewrite or delete the owner's copy from inside the
sandbox.

## Using the key

The returned path is absolute. Use it without printing the contents:

```sh
curl -s https://api.openai.com/v1/models \
  -H "Authorization: Bearer $(cat /home/you/RAVENBOT/keys/api-keys/openai)"
```

```sh
python3 - <<'PY'
import os
path = os.path.expanduser("~/RAVENBOT/keys/api-keys/openai")
with open(path, "r", encoding="utf-8") as handle:
    key = handle.read().strip()
# Pass `key` to the SDK; do not print it.
PY
```

`RAVENBOT_HOME` relocates the root; use the absolute path returned by the tool
rather than the default in these examples.

Command substitution puts the value in the command line of `curl` for a short
time, where `ps` may see it. Prefer a config file or an SDK call when that
matters. Never `cat` the file into the transcript.

A backup of `~/RAVENBOT` includes `keys/api-keys`; protect that backup like a
password store. The Vault row is the source of truth; the file is the copy a
command reads.

## Remote and headless behaviour

- **Headless runs** return `skipped` and do not park: nobody can enter a key in
  a daemon or routine.
- **Paired chats** (Telegram, Slack, Discord, WhatsApp, Teams) do not offer an
  API-key card and refuse `answer` commands for one. The notice tells the owner
  to open RAVENBOT. A platform transcript is not a secure input.
- **Desktop only**: `submit_api_key_answer` is owner-only. Operators and viewers
  cannot invoke it.

## Removing and rotating

`remove_api_key` deletes both copies. To rotate, request the same name again;
the existing row is updated and the file is replaced atomically. `list_api_keys`
reports `file_exists` so a missing file is visible instead of silently failing
in a later command.

## Limits

- The value is protected from the model, not from the operating-system account.
  An agent with shell access can read the file; that is the intended way to use
  it. A process-isolated broker would be a larger change and is not claimed
  here.
- The file is plaintext on disk. Full-disk encryption and file permissions are
  the protection at rest for that copy.
- Only one value per name. There is no history, expiry or automatic rotation.
- `list_api_keys` returns metadata; it does not verify the key with the
  provider.

## Checks

Automated:

- `cargo test -p ravenbot-core api_key`
- `cargo test -p ravenbot-db api_keys`
- `cargo test -p ravenbot-skills api_keys`
- `cargo test -p ravenbot-runtime save_api_key`
- `cargo test -p ravenbot api_key`
- `npx vitest run src/lib/components/chat/VaultRequestCard.dom.test.ts`

Manual: see the API-key section in [LIVE-CHECKS.md](LIVE-CHECKS.md).
