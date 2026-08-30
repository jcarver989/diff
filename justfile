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

feature-check:
    cargo check -p diff-syntax --no-default-features
    cargo check -p diff-syntax --features common-languages
    cargo check -p diff-syntax --no-default-features --features agent-languages
    cargo check -p diff-ratatui --no-default-features
    cargo check -p diff-ratatui --no-default-features --features syntax
    cargo check -p diff-ratatui --no-default-features --features diff-preview
    cargo check -p diff-ratatui --no-default-features --features markdown
    cargo check -p diff-ratatui --no-default-features --features diff-review
    cargo check -p diff-ratatui --no-default-features --features markdown-review
    cargo check -p diff-ratatui --all-features
    cargo check -p diff-ratatui --no-default-features --features syntax,diff-preview,markdown --example streaming_markdown
    if cargo tree -i syntect --all-features >/dev/null 2>&1; then echo "syntect unexpectedly remains in the dependency graph" >&2; exit 1; fi

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

wasm-check:
    cargo check -p diff-core -p diff-theme -p diff-syntax -p diff-markdown -p diff-gpui -p diff-gpui-web --target wasm32-unknown-unknown

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
verify: fmt-check check feature-check lint test wasm-check doc-check

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
