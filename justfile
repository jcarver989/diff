# Build and install the release clankerdiff binary into Cargo's bin directory.
install:
    cargo install --path crates/diff/cli --locked --force --profile release

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
    cd crates/diff/web && trunk serve

doctest:
    cargo test --doc --workspace --all-features

# Compile benchmark targets without running statistical measurements.
bench-check:
    cargo bench --workspace --all-features --no-run

# Run the workspace's statistical rendering benchmarks.
bench:
    cargo bench --workspace --all-features

# Fail when a crate depends on a crate from a higher domain: util < ui < diff.
layer-check:
    #!/usr/bin/env bash
    set -euo pipefail
    violations="$(cargo metadata --format-version 1 --no-deps | jq -r '
        def domain: capture("/crates/(?<d>[^/]+)/[^/]+/Cargo.toml$").d;
        def rank: {"util": 0, "ui": 1, "diff": 2}[.] // error("unknown domain \(.)");
        (.packages | map({key: .name, value: (.manifest_path | domain)}) | from_entries) as $domains
        | .packages[]
        | (.manifest_path | domain) as $domain
        | .name as $name
        | .dependencies[]
        | select($domains[.name] != null and ($domains[.name] | rank) > ($domain | rank))
        | "\($name) (\($domain)) depends on \(.name) (\($domains[.name]))"')"
    if [ -n "$violations" ]; then printf '%s\n' "$violations" >&2; exit 1; fi

feature-check: consumer-check layer-check
    tree="$(cargo tree -p clankerdiff-git -e normal --prefix none)"; if printf '%s\n' "$tree" | grep -E '^(notify|ratatui[^ ]*|crossterm|clankerdiff-(watch|ratatui|syntax|markdown|gpui[^ ]*)) v'; then echo "unexpected UI or watcher dependency in Git graph" >&2; exit 1; fi
    tree="$(cargo tree -p clankerdiff-watch -e normal --prefix none)"; if printf '%s\n' "$tree" | grep -E '^(ratatui[^ ]*|crossterm|clankerdiff-(ratatui|syntax|markdown|gpui[^ ]*)) v'; then echo "unexpected renderer dependency in watcher graph" >&2; exit 1; fi
    cargo check -p clankerdiff-ratatui --all-features --all-targets
    tree="$(cargo tree -p clankerdiff-ratatui --no-default-features --features test-support -e normal --prefix none)"; if printf '%s\n' "$tree" | grep -E '^(crossterm|ratatui-crossterm|syntect|two-face|clankerdiff-(git|watch|gpui[^ ]*)) v'; then echo "unexpected dependency in portable ratatui graph" >&2; exit 1; fi
    tree="$(cargo tree -p clankerdiff-server --no-default-features -e normal --prefix none)"; if printf '%s\n' "$tree" | grep -E '^(axum|tower[^ ]*|matchit|tokio-tungstenite|tungstenite) v'; then echo "unexpected WebSocket dependency in embedded server graph" >&2; exit 1; fi
    cargo clippy -p clankerdiff-server --no-default-features --all-targets -- -D warnings

consumer-check:
    cargo nextest run --config-file .config/nextest.toml --manifest-path tests/consumer-fixture/Cargo.toml --target-dir target
    cargo nextest run --config-file .config/nextest.toml --manifest-path tests/consumer-fixture/Cargo.toml --target-dir target --features watch,crossterm-backend
    cargo nextest run --config-file .config/nextest.toml --manifest-path tests/consumer-fixture/Cargo.toml --target-dir target --features embedded
    cargo test --doc --manifest-path tests/consumer-fixture/Cargo.toml --target-dir target
    cargo test --doc --manifest-path tests/consumer-fixture/Cargo.toml --target-dir target --features watch,crossterm-backend
    cargo test --doc --manifest-path tests/consumer-fixture/Cargo.toml --target-dir target --features embedded

wasm-check:
    cargo check -p clankerdiff-core -p clankerdiff-theme -p clankerdiff-syntax -p clankerdiff-markdown -p clankerdiff-gpui-ui -p clankerdiff-gpui -p clankerdiff-gpui-web --target wasm32-unknown-unknown
    cargo check -p clankerdiff-ratatui --no-default-features --target wasm32-unknown-unknown

# Build the release WASM and execute its smoke test in Chromium.
web-test:
    cd crates/diff/web && npm ci && npx playwright install chromium && npm test

# Exercise the remote backend, clients, and the headless CLI build.
remote-test:
    cargo nextest run -p clankerdiff-client -p clankerdiff-server --all-features
    cargo nextest run -p clankerdiff-server --no-default-features
    cargo build -p clankerdiff-cli --no-default-features --bin clankerdiff
    cargo nextest run -p clankerdiff-cli --no-default-features
    cargo nextest run --config-file .config/nextest.toml --manifest-path tests/consumer-fixture/Cargo.toml --target-dir target --features remote

# Exercise the filesystem watcher against real Git worktrees.
watch-test:
    cargo nextest run -p clankerdiff-watch --all-features
    cargo test --doc -p clankerdiff-watch --all-features

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

dist-plan-tag tag:
    dist plan --tag "{{ tag }}"

dist-build:
    dist build

# framer:begin rust-monorepo
set shell := ["bash", "-euo", "pipefail", "-c"]

# List available recipes
default:
    @just --list

# Type-check every crate and target
check:
    cargo check --workspace --all-targets --all-features

# Run tests with nextest, then doctests, which nextest doesn't run
test profile="default":
    cargo nextest run --profile {{profile}} --workspace --all-features --no-tests pass
    cargo test --doc --workspace --all-features

# Run tests with nextest's CI profile, which writes JUnit results
test-ci: (test "ci")

# Run Clippy, failing on any warning
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo clippy --manifest-path tests/consumer-fixture/Cargo.toml --target-dir target --all-targets --all-features -- -D warnings

# Format all code
fmt:
    cargo fmt --all
    cargo fmt --manifest-path tests/consumer-fixture/Cargo.toml

# Check formatting
fmt-check:
    cargo fmt --all -- --check
    cargo fmt --manifest-path tests/consumer-fixture/Cargo.toml -- --check

# Build docs, failing on any warning
doc-check:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --document-private-items

# Regenerate files from .framer/
framer:
    cargo run --quiet --manifest-path .framer/Cargo.toml

# Check generated files are up to date
framer-check:
    cargo run --quiet --locked --manifest-path .framer/Cargo.toml -- --check

# Preview what dist will build for the next release
dist-plan:
    dist plan

# Regenerate the release workflow from dist-workspace.toml
dist-generate:
    dist generate

# Check the release workflow matches dist-workspace.toml
dist-check:
    dist generate --check
# framer:end rust-monorepo
