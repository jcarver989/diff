set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Build and install the release clankerdiff binary into Cargo's bin directory.
install:
    cargo install --path crates/clankerdiff --locked --force --profile release

tui:
    cargo run -p diff-ratatui --example review

desktop:
    cargo run -p diff-gpui-desktop

ensure_trunk_installed:
    if ! command -v trunk >/dev/null 2>&1; then \
        if command -v cargo-binstall >/dev/null 2>&1; then \
            cargo binstall --no-confirm trunk; \
        else \
            cargo install --locked trunk; \
        fi; \
    fi

web: ensure_trunk_installed
    cd crates/diff-gpui-web && trunk serve

check:
    cargo check --workspace --all-targets

test:
    cargo test --workspace

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

wasm-check:
    cargo check -p diff-core -p diff-gpui -p diff-gpui-web --target wasm32-unknown-unknown

# Build the release WASM and execute its smoke test in Chromium.
web-test:
    cd crates/diff-gpui-web && npm ci && npx playwright install chromium && npm test

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

doc-check:
    cargo doc --workspace --all-features --no-deps --document-private-items

# Run every local verification check. CI intentionally runs these as separate jobs.
verify: fmt-check check lint test wasm-check doc-check

release-pr-preview:
    release-plz release-pr --dry-run

dist-generate:
    dist generate

dist-generate-check:
    dist generate --check

dist-plan:
    dist plan

dist-plan-tag tag:
    dist plan --tag "{{ tag }}"

dist-build:
    dist build
