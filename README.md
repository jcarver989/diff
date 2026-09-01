# ClankerDiff

ClankerDiff is a beautiful diff viewer that lets you give feedback to your coding agent as PR style comments. It's written in Rust and works in your TUI, desktop and web.

## Why should I use ClankerDiff?

- It makes it easier to review the code your agent generates, and give it targeted feedback.
- It's written in Rust, so it's "blazing fast" (tm) and doesn't have the "JS flicker". 
- It's retro (runs in your TUI) _and_ modern (native, gpu accelerated rendering on Desktop; WASM on web)
- It's renders really nice looking diffs, with theme support.

## Live repository refresh

Native desktop and terminal diff reviews automatically refresh after debounced repository filesystem changes, including worktree edits and ordinary in-tree Git metadata updates. Editor save bursts are coalesced before ClankerDiff rebuilds the authoritative Git snapshot.

Manual refresh remains available with `r` in the TUI or ⌘/Ctrl+R on desktop, including when the operating system cannot establish or maintain a filesystem watcher. Hosted web and Markdown reviews consume supplied documents and do not watch a native repository.
