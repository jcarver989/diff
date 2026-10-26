# Diff

This repository holds reusable GPUI app building blocks and Diff, a performant diff tool with comment support built on them. Diff's CLI runs in a TUI (ratatui) or Desktop (gpui), while the standalone Web frontend (gpui) supports hosted use cases. Other apps (j-terminal, aether) consume the `util/` and `ui/` crates directly.

## Layout

Crates are grouped by domain under `crates/`:

- `crates/util/`: general-purpose crates with no UI and no diff concepts (`fingerprint`, `remote`).
- `crates/ui/`: themes, syntax highlighting and the GPUI design system and app shell (`theme`, `syntax`, `gpui`, `gpui-shell`).
- `crates/diff/`: the diff and markdown review domain (`core`, `protocol`, `client`, `git`, `watch`, `server`, `markdown`, `gpui`, `ratatui`, `cli`, `desktop`, `web`).

A crate may depend only on crates in its own domain or a lower one: `util` < `ui` < `diff`. `just layer-check` enforces this. Put new code in the lowest domain it fits; anything another app could use without diff concepts belongs in `ui/` or `util/`.

Internal crates are declared once in the root `[workspace.dependencies]`; reference them with `.workspace = true`.

## Generated configuration

[framer](https://github.com/jcarver989/framer) generates the repository's configuration from `.framer/src/main.rs`: the toolchain pin, rustfmt, clippy and nextest config, `.tool-versions`, the workspace members, package settings, lints, profiles, gpui pins and internal crate paths in `Cargo.toml`, the CI and release-plz workflows, `dist-workspace.toml`, and the marked blocks in `justfile` and `.gitignore`. Files and blocks it owns say so in a header. Change `.framer/src/main.rs` and run `just framer` rather than editing them; `just framer-check` fails when they drift. `.framer/` depends on the `framer` and `framer-presets` crates from crates.io; CI runs `just framer-check`.

`.framer/src/lib.rs` holds the `GpuiApp` preset, which owns the gpui pins and the internal crate paths (`CRATES`). It's a library so sibling apps such as j-terminal and aether can apply it later; when a crate moves, update `CRATES` and run `just framer`.

## Coding conventions

### General

1. Prefer using `Foo` over `std::biz::baz::boo::Foo` in code by importing types at the top of the file, e.g. `use std::biz::baz::boo::Foo`.
2. Prefer using `T`, `U`, `V` etc for generic type param names, always start with `T`.
3. Use `thiserror` crate for errors.
4. Avoid `Mutex`, `Arc<Mutex>`, `Semaphore` and other forms of locking where possible. Instead give each task (or thread) ownership of its own resources, or if you must share resources use either structured concurrency or tokio channels via an actor pattern.
5. Crates in this workspace should have "one way to do something" and export a cohesive api -- e.g. if we need to support incremental syntax highlighting _and_ full file syntax highlighting, the incremental API should be designed to cover both use-cases and we should not create parallel implementations.

### Rust docs

1. Never add comments unless explicitly instructed to. 

### Testing

1. Use real objects where possible. When it's not possible, prefer crate-provided test utilities and/or in-memory fakes over mocks.
2. Use the test builder pattern described in this [post](https://jmmv.dev/2020/12/builder-pattern-for-tests.html).
3. Prefer extending an existing fake or test builder to support a use case over creating a new bespoke builder/fake. Good builders and fakes are general purpose, reusable across test suites, and mimic the behavior and APIs of the "real thing".
4. Tests may only test _public_ APIs; never private. Thus, prefer integration tests in `tests/` over unit tests.
5. Put non-shared test helpers at the _bottom_ of files, below the tests.
6. Prefer using `?` in tests by making the test return a `Result` vs using `unwrap`, which is an anti-pattern. 
7. Prefer integration tests in the crate's `tests/`dir over unit tests. Integration test files should match the name and directory structure of the file they test, e.g `src/foo/boo.rs` and `test/foo/boo_test.rs`.
