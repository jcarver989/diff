# ClankerDiff

ClankerDiff is a beautiful diff viewer that lets you give feedback to your coding agent as PR style comments. It's written in Rust and works in your TUI, desktop and web.

## Why should I use ClankerDiff?

- It makes it easier to review the code your agent generates, and give it targeted feedback.
- It's written in Rust, so it's "blazing fast" (tm) and doesn't have the "JS flicker". 
- It's retro (runs in your TUI) _and_ modern (native, gpu accelerated rendering on Desktop; WASM on web)
- It's renders really nice looking diffs, with theme support.

## Live updates

The diff refreshes itself. When the worktree or the index changes, the TUI and
desktop viewers reload within a few hundred milliseconds, keeping your file
selection, scroll position, expanded context, queued comments, and any comment
draft you are still typing. Staging, committing, and discarding are picked up by
the filesystem watcher without replacing the viewer with a loading screen.

Changes Git ignores are ignored here too, so build output does not cause work.

Watching is always enabled; no manual refresh is needed.

### Web

The browser host's push/acknowledgement protocol is documented by the
[`diff-gpui-web` crate](crates/diff-gpui-web/src/lib.rs).
