# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.2](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.3.1...clankerdiff-ratatui-v0.3.2) - 2026-09-20

### Added

- *(diff-ratatui)* Add apply_client_state ([#74](https://github.com/jcarver989/diff/pull/74))

## [0.3.0](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.2.1...clankerdiff-ratatui-v0.3.0) - 2026-09-18

### Added

- Support option to see full file outside of diff view.  ([#61](https://github.com/jcarver989/diff/pull/61))

## [0.2.1](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.2.0...clankerdiff-ratatui-v0.2.1) - 2026-09-14

### Other

- Match markdown mouse comment behavior to diff review ([#60](https://github.com/jcarver989/diff/pull/60))
- Open and reposition empty comment drafts with diff clicks ([#58](https://github.com/jcarver989/diff/pull/58))

## [0.2.0](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.1.11...clankerdiff-ratatui-v0.2.0) - 2026-09-14

### Fixed

- Markdown wheel navigation getting stuck over blank rows ([#57](https://github.com/jcarver989/diff/pull/57))

### Other

- Fix Ratatui preview gutters and split change indicators ([#56](https://github.com/jcarver989/diff/pull/56))
- use cargo nextest for tests ([#53](https://github.com/jcarver989/diff/pull/53))

## [0.1.11](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.1.10...clankerdiff-ratatui-v0.1.11) - 2026-09-13

### Added

- *(diff-core)* Support parsing with path mapper ([#51](https://github.com/jcarver989/diff/pull/51))

## [0.1.10](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.1.9...clankerdiff-ratatui-v0.1.10) - 2026-09-12

### Other

- update Cargo.toml dependencies

## [0.1.9](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.1.8...clankerdiff-ratatui-v0.1.9) - 2026-09-12

### Fixed

- *(ratatui)* wrap compact previews and preserve diff indicator colors ([#38](https://github.com/jcarver989/diff/pull/38))

## [0.1.8](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.1.7...clankerdiff-ratatui-v0.1.8) - 2026-09-12

### Added

- *(ratatui)* add pull-based wrapping and logical-row rendering ([#36](https://github.com/jcarver989/diff/pull/36))

## [0.1.6](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.1.5...clankerdiff-ratatui-v0.1.6) - 2026-09-10

### Fixed

- retain repository snapshots and defer interactive updates ([#33](https://github.com/jcarver989/diff/pull/33))

## [0.1.4](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.1.3...clankerdiff-ratatui-v0.1.4) - 2026-09-10

### Added

- *(theme)* separate semantic UI palettes from diff colors ([#28](https://github.com/jcarver989/diff/pull/28))
- *(ratatui)* expose a complete embedding facade ([#27](https://github.com/jcarver989/diff/pull/27))
- *(ratatui)* default diff tabs to two configurable columns ([#25](https://github.com/jcarver989/diff/pull/25))

### Fixed

- *(diff-ratatui)* Split diff backgrounds looked weird ([#26](https://github.com/jcarver989/diff/pull/26))

## [0.1.2](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.1.1...clankerdiff-ratatui-v0.1.2) - 2026-09-10

### Added

- Streaming rendering ([#21](https://github.com/jcarver989/diff/pull/21))

### Added

- Terminal streaming policy, revision-checked row commits, immutable native-history rows, and source/render checkpoints for live-tail reflow and partial tabs.
- A terminal embedding example covering failed writes, acknowledgement, stale revisions, and completion.

### Fixed

- Committing terminal rows invalidates cached pre-commit deltas, so stale or repeated requests cannot return updates crossing acknowledged history.
- Rendered Markdown prefixes and open-fence row storage use persistent range reuse; ordinary appends no longer traverse every preceding block or code-line chunk. Explicit layout/source-copy counters and scaling regressions account for work outside parsing.
- Added structured commit-boundary, final-suffix commit, and standalone packaged-consumer regression coverage.

- Partial code edits invalidate text as well as syntax styles; one-shot and streamed code use the same newline encoding.
- Unchanged row handles remain identical in the renderer and host mirror, including when fences close.
- Finishing a Markdown stream or resuming it without additional source no longer reparses Markdown, rehighlights code, regenerates rows, or advances the layout revision when layout options and theme are unchanged.

## [0.1.1](https://github.com/jcarver989/diff/compare/clankerdiff-ratatui-v0.1.0...clankerdiff-ratatui-v0.1.1) - 2026-09-09

### Other

- Crate libs.  ([#17](https://github.com/jcarver989/diff/pull/17))

## [0.1.0](https://github.com/jcarver989/diff/releases/tag/diff-ratatui-v0.1.0) - 2026-08-31

### Added

- *(markdown)* render streaming syntax incrementally
- theme pickers for tui, desktop/web
- Use aborium for syntax highlighting
- add checkboxes for files/directories in filepicker
- rough pass on markdown support

### Other

- *(ratatui)* Simplify
- stash wip
- *(ratatui)* Start to add design system
- add ratatui
- align checkboxes to right
- *(ratatui)* Nicer comment box
- *(core)* Add row_showing_anchor
- *(fonts)* Add bundled fonts
- stash wip
- *(diff-ratatui)* Cleanup and benchmark pass
- *(diff-ratatui)* Initial commit
