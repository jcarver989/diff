# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.1](https://github.com/jcarver989/diff/compare/clankerdiff-gpui-v0.8.0...clankerdiff-gpui-v0.8.1) - 2026-10-31

### Added

- browse every repository file and rework the viewer toolbar ([#125](https://github.com/jcarver989/diff/pull/125))

## [0.8.0](https://github.com/jcarver989/diff/compare/clankerdiff-gpui-v0.7.1...clankerdiff-gpui-v0.8.0) - 2026-10-29

### Added

- bound the GPUI diff preview and cap the lspd log ([#123](https://github.com/jcarver989/diff/pull/123))
- drop the gpui patch, add a Waveform indicator, virtualize the sidebar ([#122](https://github.com/jcarver989/diff/pull/122))

### Other

- release ([#119](https://github.com/jcarver989/diff/pull/119))

## [0.7.1](https://github.com/jcarver989/diff/releases/tag/clankerdiff-gpui-v0.7.1) - 2026-10-29

### Added

- publish the GPUI crates to crates.io ([#118](https://github.com/jcarver989/diff/pull/118))
- shared keymap, toolbar and command palette across the TUI and desktop ([#115](https://github.com/jcarver989/diff/pull/115))
- look up definitions outside the diff through language servers ([#112](https://github.com/jcarver989/diff/pull/112))
- symbol navigation with definition cards in desktop and TUI ([#110](https://github.com/jcarver989/diff/pull/110))
- commit picker with previews for paging through branch commits ([#109](https://github.com/jcarver989/diff/pull/109))

### Other

- util/ui/diff crate layout, remote machines and framer-generated config ([#104](https://github.com/jcarver989/diff/pull/104))

## [0.1.0](https://github.com/jcarver989/diff/releases/tag/diff-gpui-v0.1.0) - 2026-08-31

### Added

- *(markdown)* render streaming syntax incrementally
- theme pickers for tui, desktop/web
- Use aborium for syntax highlighting
- add checkboxes for files/directories in filepicker
- rough pass on markdown support
- *(gpui)* Add keyboard shortcuts for nav an better font spacing
- *(diff-gpui)* Add filedrawer , resizeable sidebar, and resizeable

### Other

- *(gpui)* Simplify
- stash wip
- *(gpui)* start to add design system
- update gpui crates
- padding
- align checkboxes to right
- *(gpui)* Tweak fonts
- *(fonts)* Add bundled fonts
- stash wip
- *(gpui)* Enable word wrapping
- *(diff-gpui)* Cleanup pass
- *(diff-gpui)* Initial commit
