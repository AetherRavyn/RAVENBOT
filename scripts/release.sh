#!/bin/bash
set -euo pipefail

# RAVENBOT Release Build Script
# This script builds hardened release binaries with code signing

VERSION=${1:-"0.1.0"}
PLATFORM=${2:-"linux"}

echo "Building RAVENBOT v${VERSION} for ${PLATFORM}"

# Run security audit first
echo "Running security audit..."
cargo audit || echo "Warning: Audit found issues"
cargo deny check

# Run tests
echo "Running tests..."
cargo test --all

# Build release
echo "Building release..."
cargo build --release

# Generate SBOM
echo "Generating SBOM..."
cargo install cargo-cyclonedx 2>/dev/null || true
cargo cyclonedx --format json --output-dir sbom

# Generate checksums
echo "Generating checksums..."
cd target/release
sha256sum ravenbot > ../SHA256SUMS
cd ../..

# Sign binaries (if on macOS)
if [ "$PLATFORM" = "macos" ] && command -v codesign &> /dev/null; then
    echo "Signing macOS binary..."
    codesign --sign "Developer ID Application: RAVENBOT" \
             --timestamp \
             --options runtime \
             target/release/ravenbot.app
fi

echo "Release build complete!"
echo "Artifacts:"
echo "  - Binary: target/release/ravenbot"
echo "  - SBOM: sbom/"
echo "  - Checksums: target/SHA256SUMS"
