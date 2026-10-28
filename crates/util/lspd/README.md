# clankerdiff-lspd

An LSP daemon that manages language server processes and shares them across every client working in the same workspace, such as Diff and Aether. Communicates over Unix domain sockets.

## Quick start

Query hover info for a file in a workspace:

```rust,no_run
use clankerdiff_lspd::{ClientError, LanguageId, LspClient, WorkspaceKey, path_to_uri};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), ClientError> {
    let project = Path::new("/home/user/my-project");
    let client = LspClient::new(WorkspaceKey::resolve(project, LanguageId::Rust));

    let uri = path_to_uri(&project.join("src/main.rs")).unwrap();
    let hover = client.hover(uri, 10, 5).await?;
    Ok(())
}
```

A client connects on its first request, or eagerly through [`LspClient::connect`], and reconnects on the next request after its daemon goes away. When no daemon is running it starts one by running the current executable's `lspd` subcommand, so every binary that connects must call [`run_if_requested`] first thing in `main`. The daemon outlives the app that started it and exits after five idle minutes. Multiple clients share the same daemon for a given workspace and language server.

[`WorkspaceKey::resolve`] accepts any path inside the project and resolves the workspace root itself, so clients that start from different directories of one project share a daemon: the outermost Cargo workspace (else the nearest crate) for Rust, `go.work` (else `go.mod`) for Go, the outermost `package.json` with `workspaces` (else the nearest `package.json` or `tsconfig.json`) for JavaScript and TypeScript, the nearest `pyproject.toml`, `setup.cfg`, `setup.py` or `requirements.txt` for Python, and the outermost `CMakeLists.txt` for C and C++. The search stops at the repository root, which is also the fallback.

## Documentation

Full API documentation is available on [docs.rs](https://docs.rs/clankerdiff-lspd).

Key entry points:
- [`LspClient`] -- make LSP requests through a workspace's daemon
- [`run_if_requested`] -- run the daemon that manages language servers
- [`LanguageId`] -- supported languages
- [`WorkspaceKey`] -- the project root and language server a daemon serves

## Versioning

Several independently released apps share one daemon per workspace, so every change to the client-daemon wire format (`DaemonRequest`, `DaemonResponse` and the types they carry) must bump `PROTOCOL_VERSION`. The version is part of the socket name, so clients and daemons from different versions get separate sockets: version skew starts a second daemon rather than breaking the first.

## Feature Flags

| Feature | Description |
|---------|-------------|
| `daemon` | The daemon and [`run_if_requested`], for binaries that host it behind an `lspd` subcommand |
| `test-support` | In-process test daemons, a fake language server and temporary projects; implies `daemon` |

## License

MIT
