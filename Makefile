.PHONY: dev build test audit deny sbom release clean

# Development
dev:
	cargo build
	npm run tauri dev

# Build
build:
	cargo build --release
	npm run tauri build

# Test
test:
	cargo test --all
	npm run check

# Security Audit
audit:
	cargo audit

# Deny Check
deny:
	cargo deny check

# Generate SBOM
sbom:
	cargo install cargo-cyclonedx
	cargo cyclonedx --format json --output-dir sbom

# Lint
lint:
	cargo clippy --all -- -D warnings
	npm run check

# Format
format:
	cargo fmt --all
	npm run format

# Clean
clean:
	cargo clean
	rm -rf sbom/

# Full security check
security: audit deny
	@echo "Security checks passed!"

# Release build
release:
	./scripts/release.sh $(VERSION) $(PLATFORM)

# Help
help:
	@echo "Available commands:"
	@echo "  dev       - Run development build"
	@echo "  build     - Build release"
	@echo "  test      - Run all tests"
	@echo "  audit     - Run cargo-audit"
	@echo "  deny      - Run cargo-deny"
	@echo "  sbom      - Generate SBOM"
	@echo "  lint      - Run linters"
	@echo "  format    - Format code"
	@echo "  clean     - Clean build artifacts"
	@echo "  security  - Run all security checks"
	@echo "  release   - Build release with hardening"
