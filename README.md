# ClankerDiff

ClankerDiff is a beautiful diff viewer that lets you give feedback to your coding agent as PR style comments. It's written in Rust and works in your TUI, desktop and web.

## Why should I use ClankerDiff?

- It makes it easier to review the code your agent generates, and give it targeted feedback.
- It's written in Rust, so it's "blazing fast" (tm) and doesn't have the "JS flicker". 
- It's retro (runs in your TUI) _and_ modern (native, gpu accelerated rendering on Desktop; WASM on web)
- It's renders really nice looking diffs, with theme support.

## Remote repositories

Run the backend where the checkout lives, then connect from a UI machine:

```sh
clankerdiff serve /workspace/repo --listen 0.0.0.0:7331
clankerdiff connect ws://vm:7331/ws --ui tui --scope both --format json
clankerdiff connect wss://sandbox.example/ws --ui desktop
```

The backend is **unauthenticated**: anyone who can reach it can read repository
content and invoke Git actions. Use a trusted network boundary or an existing
access-controlled TLS proxy. The default listener binds only to `127.0.0.1`.
Local `review` remains in-process and does not require a listening port unless
launching an external terminal.

See [remote diff](docs/remote-development.md) for deployment, the Rust client
API, and the live protocol. A renderer-free server/TUI build is available with
`cargo build -p clankerdiff-cli --no-default-features --bin clankerdiff`.

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
