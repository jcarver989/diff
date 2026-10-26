# ClankerDiff

Clankerdiff is a beautiful diff-viewer that lets you send PR-style comments to your coding agent. It runs on the desktop, in the terminal and on the web.

![Clankerdiff desktop diff view with syntax highlighting and an inline review comment.](crates/diff/cli/assets/clankerdiff-desktop.png)

For **macOS (Apple Silicon)** and **Linux (x86_64 / ARM64)**:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/jcarver989/diff/releases/download/clankerdiff-cli-v0.3.2/clankerdiff-cli-installer.sh | sh
```

See the [ClankerDiff README](crates/diff/cli/README.md) for usage, the agent skill and keyboard shortcuts.

## Repository layout

This repository also holds the GPUI building blocks ClankerDiff is made of, which other apps use too. Crates are grouped by domain under `crates/`:

| Domain | Crates | What's there |
| --- | --- | --- |
| `util/` | `fingerprint`, `remote` | General-purpose crates with no UI and no diff concepts. |
| `ui/` | `theme`, `syntax`, `gpui`, `gpui-shell` | Themes, syntax highlighting, and the GPUI design system and app shell. |
| `diff/` | `core`, `protocol`, `client`, `git`, `watch`, `server`, `markdown`, `gpui`, `ratatui`, `cli`, `desktop`, `web` | The diff and markdown review domain, its renderers and the ClankerDiff apps. |

A crate may depend only on its own domain or a lower one (`util` < `ui` < `diff`), which `just layer-check` enforces. See [AGENTS.md](AGENTS.md) for conventions.
