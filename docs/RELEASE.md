# Release and supply-chain runbook

RAVENBOT releases are built by [`.github/workflows/release.yml`](../.github/workflows/release.yml)
on a `vX.Y.Z` tag. Each matrix job builds Linux, macOS and Windows bundles; the
checksum and SBOM jobs attach `SHA256SUMS.txt` and `sbom.cdx.json` to the same
GitHub Release.

## What ships today

- Linux AppImage, `.deb`, `.rpm`
- macOS universal `.dmg`
- Windows NSIS `.exe`, WiX `.msi`
- `SHA256SUMS.txt` generated from the attached installers
- `sbom.cdx.json` in CycloneDX 1.5, generated from `Cargo.lock` and
  `package-lock.json` by `scripts/generate-sbom.py`

The current artifacts are **unsigned**. macOS Gatekeeper and Windows SmartScreen
will warn until the signing and notarization secrets below are configured in the
repository.

## Signing and notarization

Set these repository secrets (Settings → Secrets and variables → Actions) and
the release workflow will pick them up:

| Secret | Platform | Purpose |
|---|---|---|
| `APPLE_CERTIFICATE` | macOS | base64-encoded `.p12` Developer ID Application certificate |
| `APPLE_CERTIFICATE_PASSWORD` | macOS | password for the `.p12` |
| `APPLE_SIGNING_IDENTITY` | macOS | e.g. `Developer ID Application: Name (TEAMID)` |
| `APPLE_ID` | macOS | Apple ID used for notarization |
| `APPLE_PASSWORD` | macOS | Apple app-specific password |
| `APPLE_TEAM_ID` | macOS | Apple Developer team ID |
| `WINDOWS_CERTIFICATE` | Windows | base64-encoded Authenticode `.pfx` |
| `WINDOWS_CERTIFICATE_PASSWORD` | Windows | password for the `.pfx` |
| `TAURI_SIGNING_PRIVATE_KEY` | updater | Tauri updater private key |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | updater | key password, if set |

Tauri’s action reads the Apple and Windows variables directly. Keep the private
key and certificate files out of the repository; `.gitignore` excludes
`.update-keys/`.

## Auto-update

The desktop app already registers `tauri-plugin-updater`, but
`src-tauri/tauri.conf.json` has an empty updater `pubkey` and no endpoints.
Enabling updates is a deliberate release step:

1. Generate a key pair with `npx tauri signer generate -w .update-keys/ravenbot.key`.
2. Put the public key in `plugins.updater.pubkey`.
3. Set `plugins.updater.endpoints` to the release manifest URL.
4. Set `bundle.createUpdaterArtifacts` to `true`.
5. Add the private key as `TAURI_SIGNING_PRIVATE_KEY` and publish the generated
   update manifest alongside the release assets.

Do not enable this in a fork with a different signing key; clients only accept
updates signed by the key compiled into the app.

## Verifying a release

```bash
sha256sum -c SHA256SUMS.txt
python3 - <<'PY'
import json
sbom = json.load(open("sbom.cdx.json"))
print(sbom["specVersion"], len(sbom["components"]), "components")
PY
```

Check that the tag, `Cargo.toml`, `package.json` and `src-tauri/tauri.conf.json`
all agree on the version. Run the tagged commit through the normal CI matrix
before publishing.
