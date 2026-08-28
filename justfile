set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

tui:
    cargo run -p diff-ratatui --example review

desktop:
    cargo run -p diff-gpui-desktop

web:
    cd crates/diff-gpui-web && trunk serve

check:
    cargo check --workspace --all-targets

test:
    cargo test --workspace

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

wasm-check:
    cargo check -p diff-core -p diff-gpui -p diff-gpui-web --target wasm32-unknown-unknown

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

doc-check:
    cargo doc --workspace --all-features --no-deps --document-private-items

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
