# ClankerDiff

ClankerDiff is a beautiful diff viewer that lets you give feedback to your coding agent as PR style comments. It's written in Rust and works in your TUI, desktop and web.

## Why should I use ClankerDiff?

- It makes it easier to review the code your agent generates, and give it targeted feedback.
- It's written in Rust, so it's "blazing fast" (tm) and doesn't have the "JS flicker". 
- It's retro (runs in your TUI) _and_ modern (native, gpu accelerated rendering on Desktop; WASM on web)
- It's renders really nice looking diffs, with theme support.

## Running tests

Install the pinned tools with `mise install`, or install
[cargo-nextest](https://nexte.st/docs/installation/) 0.9.144 or newer directly:

```sh
cargo install cargo-nextest --locked
```

Run `just test` for workspace tests, `just watch-test` for watcher tests, or
`just consumer-check` for the consumer feature combinations. These recipes use
nextest for unit and integration tests and retain `cargo test --doc` for doctests,
which nextest does not support. Run doctests alone with `just doctest`.

`just test-ci` uses the nextest CI profile and writes a JUnit report to
`target/nextest/ci/junit.xml`.
