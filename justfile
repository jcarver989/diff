set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Build and install the release clankerdiff binary into Cargo's bin directory.
install:
    cargo install --path crates/clankerdiff --locked --force --profile release

tui:
    cargo run -p clankerdiff-ratatui --features crossterm-backend --example review

desktop:
    cargo run -p clankerdiff-gpui-desktop

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
    cargo test --workspace --all-features

# Compile benchmark targets without running statistical measurements.
bench-check:
    cargo bench --workspace --all-features --no-run

# Run the workspace's statistical rendering benchmarks.
bench:
    cargo bench --workspace --all-features

feature-check: consumer-check
    tree="$(cargo tree -p clankerdiff-git -e normal --prefix none)"; if printf '%s\n' "$tree" | grep -E '^(notify|ratatui[^ ]*|crossterm|clankerdiff-(watch|ratatui|syntax|markdown|gpui[^ ]*)) v'; then echo "unexpected UI or watcher dependency in Git graph" >&2; exit 1; fi
    tree="$(cargo tree -p clankerdiff-watch -e normal --prefix none)"; if printf '%s\n' "$tree" | grep -E '^(ratatui[^ ]*|crossterm|clankerdiff-(ratatui|syntax|markdown|gpui[^ ]*)) v'; then echo "unexpected renderer dependency in watcher graph" >&2; exit 1; fi
    cargo check -p clankerdiff-ratatui --all-features --all-targets
    tree="$(cargo tree -p clankerdiff-ratatui --no-default-features --features test-support -e normal --prefix none)"; if printf '%s\n' "$tree" | grep -E '^(crossterm|ratatui-crossterm|syntect|two-face|clankerdiff-(git|watch|gpui[^ ]*)) v'; then echo "unexpected dependency in portable ratatui graph" >&2; exit 1; fi

consumer-check:
    cargo test --manifest-path tests/consumer-fixture/Cargo.toml --target-dir target
    cargo test --manifest-path tests/consumer-fixture/Cargo.toml --target-dir target --features watch,crossterm-backend

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo clippy --manifest-path tests/consumer-fixture/Cargo.toml --target-dir target --all-targets --all-features -- -D warnings

wasm-check:
    cargo check -p clankerdiff-core -p clankerdiff-theme -p clankerdiff-syntax -p clankerdiff-markdown -p clankerdiff-gpui -p clankerdiff-gpui-web --target wasm32-unknown-unknown
    cargo check -p clankerdiff-ratatui --no-default-features --target wasm32-unknown-unknown

# Build the release WASM and execute its smoke test in Chromium.
web-test:
    cd crates/diff-gpui-web && npm ci && npx playwright install chromium && npm test

# Exercise the filesystem watcher against real Git worktrees.
watch-test:
    cargo test -p clankerdiff-watch --all-features

fmt:
    cargo fmt --all
    cargo fmt --manifest-path tests/consumer-fixture/Cargo.toml

fmt-check:
    cargo fmt --all -- --check
    cargo fmt --manifest-path tests/consumer-fixture/Cargo.toml -- --check

doc-check:
    cargo doc --workspace --all-features --no-deps --document-private-items

# Validate the GitHub Aether automation.
automation-check:
    jq empty .aether/settings.json .aether/github.settings.json .aether/mcp.json
    for script in scripts/aether-agent/*; do bash -n "$script"; done
    python3 -B -m unittest discover -s tests/aether-agent -p 'test_*.py'
    # actionlint does not yet recognize GitHub's concurrency.queue property.
    actionlint -ignore 'unexpected key "queue" for "concurrency" section' \
        .github/workflows/aether-agent.yml .github/workflows/ci.yml

# Run every local verification check. CI intentionally runs these as separate jobs.
verify: fmt-check check feature-check lint test wasm-check doc-check automation-check package-check

package-check:
    python3 scripts/check-consumer.py

published-consumer-check:
    python3 scripts/check-consumer.py --published

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
