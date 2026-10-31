# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.1](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.8.0...clankerdiff-core-v0.8.1) - 2026-10-31

### Added

- browse every repository file and rework the viewer toolbar ([#125](https://github.com/jcarver989/diff/pull/125))

## [0.8.0](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.7.1...clankerdiff-core-v0.8.0) - 2026-10-29

### Added

- drop the gpui patch, add a Waveform indicator, virtualize the sidebar ([#122](https://github.com/jcarver989/diff/pull/122))

## [0.7.1](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.7.0...clankerdiff-core-v0.7.1) - 2026-10-29

### Fixed

- *(core)* restore the keys the shared keymap dropped ([#116](https://github.com/jcarver989/diff/pull/116))

## [0.7.0](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.6.0...clankerdiff-core-v0.7.0) - 2026-10-29

### Added

- shared keymap, toolbar and command palette across the TUI and desktop ([#115](https://github.com/jcarver989/diff/pull/115))

### Fixed

- *(core)* rebuild the diff when navigating back from a reference ([#113](https://github.com/jcarver989/diff/pull/113))

## [0.6.0](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.5.0...clankerdiff-core-v0.6.0) - 2026-10-28

### Added

- look up definitions outside the diff through language servers ([#112](https://github.com/jcarver989/diff/pull/112))
- symbol navigation with definition cards in desktop and TUI ([#110](https://github.com/jcarver989/diff/pull/110))

## [0.5.0](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.4.0...clankerdiff-core-v0.5.0) - 2026-10-27

### Added

- commit picker with previews for paging through branch commits ([#109](https://github.com/jcarver989/diff/pull/109))

## [0.4.0](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.3.3...clankerdiff-core-v0.4.0) - 2026-10-26

### Other

- util/ui/diff crate layout, remote machines and framer-generated config ([#104](https://github.com/jcarver989/diff/pull/104))

## [0.3.0](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.2.1...clankerdiff-core-v0.3.0) - 2026-09-18

### Added

- Remote diff support  ([#64](https://github.com/jcarver989/diff/pull/64))
- Support option to see full file outside of diff view.  ([#61](https://github.com/jcarver989/diff/pull/61))

## [0.1.11](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.1.10...clankerdiff-core-v0.1.11) - 2026-09-13

### Added

- *(diff-core)* Support parsing with path mapper ([#51](https://github.com/jcarver989/diff/pull/51))

## [0.1.10](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.1.9...clankerdiff-core-v0.1.10) - 2026-09-12

### Added

- generate git patches from text snapshots

## [0.1.2](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.1.1...clankerdiff-core-v0.1.2) - 2026-09-10

### Added

- Streaming rendering ([#21](https://github.com/jcarver989/diff/pull/21))

## [0.1.1](https://github.com/jcarver989/diff/compare/clankerdiff-core-v0.1.0...clankerdiff-core-v0.1.1) - 2026-09-09

### Other

- Crate libs.  ([#17](https://github.com/jcarver989/diff/pull/17))

## [0.1.0](https://github.com/jcarver989/diff/releases/tag/diff-core-v0.1.0) - 2026-08-31

### Added

- *(syntax)* expand embedding API and language support
- theme pickers for tui, desktop/web
- Use aborium for syntax highlighting
- add checkboxes for files/directories in filepicker
- rough pass on markdown support

### Other

- *(core)* Simplify
- stash wip
- appease clippy
- Cleanup cache key logic
- update core crate
- shuffle files around and break up crates
- align checkboxes to right
- use latest rust syntax
- *(core)* Add row_showing_anchor
- use ayu dark as default theme
- stash wip
- *(core)* Begin to cleanup some of the code
- *(core)* Add fingerprint
- *(diff-core)* initial commit
