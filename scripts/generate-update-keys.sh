#!/usr/bin/env bash
# Generate the ed25519 signing keypair for RAVENBOT signed updates.
#
# The private key NEVER leaves your machine and NEVER goes into the repo.
# The public key goes into src-tauri/tauri.conf.json (plugins.updater.pubkey).
# Release jobs sign artifacts with TAURI_SIGNING_PRIVATE_KEY (+ optional password).
#
# Usage: ./scripts/generate-update-keys.sh
set -euo pipefail

OUT_DIR="${1:-.update-keys}"
mkdir -p "$OUT_DIR"

cd src-tauri
echo "Generating update signing keypair (password-protected private key)..."
cargo tauri signer generate -w "$OUT_DIR/ravenbot.key"

cat <<'NEXT'

Next steps (done once):
  1. Store .update-keys/ravenbot.key + password in your secret manager.
  2. Copy the PUBLIC key printed above into:
       src-tauri/tauri.conf.json -> plugins.updater.pubkey
  3. Set plugins.updater.endpoints (e.g. https://releases.yourhost.com/ravenbot/{{target}}/{{current_version}}).
  4. In CI release secrets set:
       TAURI_SIGNING_PRIVATE_KEY  = contents of ravenbot.key
       TAURI_SIGNING_PRIVATE_KEY_PASSWORD = the password you chose

Sign artifacts at release time with:
  cargo tauri build --release \
    --signer-args "-w /path/to/ravenbot.key"  # or env TAURI_SIGNING_PRIVATE_KEY

NEVER commit ravenbot.key or embed the private key anywhere.
NEXT
