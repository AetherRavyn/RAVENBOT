# Security Policy

## Reporting Vulnerabilities

If you discover a security vulnerability in RAVENBOT, please report it responsibly:

1. **Do NOT** open a public GitHub issue
2. Email security@ravenbot.local (or use the private vulnerability reporting feature)
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

We will respond within 48 hours and work with you to address the issue.

## Supply Chain Security

### Dependency Auditing

RAVENBOT uses automated tooling to ensure dependency security:

- **cargo-audit**: Checks for known vulnerabilities in dependencies
- **cargo-deny**: Enforces license compliance and dependency policies
- **Dependabot**: Automatically creates PRs for security updates

### SBOM Generation

Every release includes a Software Bill of Materials (SBOM) in CycloneDX format.

To generate an SBOM locally:

```bash
cargo install cargo-cyclonedx
cargo cyclonedx --format json --output-dir sbom
```

### Code Signing

Release binaries are code-signed:

- **macOS**: Apple notarization via `codesign`
- **Windows**: Authenticode signing via `signtool`
- **Linux**: GPG signatures for checksums

### Build Reproducibility

Builds are reproducible using:
- Locked dependency versions (`Cargo.lock`)
- Deterministic compiler settings
- CI/CD with fixed toolchain versions

## Runtime Security

### Sandboxing

Each bot runs in an isolated sandbox:
- OS-level process isolation
- Resource quotas (CPU, memory, network)
- File system access controls

### Network Security

- Per-bot network policies
- Domain allowlists
- Rate limiting
- TLS for all external connections

### Data Security

- Local-only storage (no cloud by default)
- Optional SQLCipher encryption at rest
- OS keychain for secrets
- No telemetry without explicit opt-in

## Verification

### Verifying Release Checksums

```bash
# Download checksum and signature
curl -sSL https://github.com/ravenbot/ravenbot/releases/latest/download/SHA256SUMS
curl -sSL https://github.com/ravenbot/ravenbot/releases/latest/download/SHA256SUMS.asc

# Import maintainer key
gpg --keyserver keyserver.ubuntu.com --recv-keys KEY_ID

# Verify signature
gpg --verify SHA256SUMS.asc SHA256SUMS

# Verify binary
sha256sum -c SHA256SUMS
```

### Verifying Build Attestation

```bash
# Install cosign
go install sigstore/cosign/cmd/cosign@latest

# Verify attestation
cosign verify-blob \
  --certificate-identity=KEY_ID \
  --certificate-oidc-issuer=https://token.actions.githubusercontent.com \
  --bundle=ravenbot.attestation.json \
  ravenbot-v0.1.0.tar.gz
```

## Compliance

RAVENBOT is designed to help users comply with:
- GDPR (data stays on user's device)
- CCPA (no data collection by default)
- HIPAA (with proper sandboxing configuration)

## Security Updates

Security updates are released as:
- Patch versions (e.g., 0.1.1) for vulnerability fixes
- Minor versions (e.g., 0.2.0) for security features
- GitHub Security Advisories for critical issues
