# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.3](https://github.com/jcarver989/diff/compare/clankerdiff-syntax-v0.1.2...clankerdiff-syntax-v0.1.3) - 2026-09-10

### Fixed

- *(syntax)* use upstream tree-sitter runtime ([#23](https://github.com/jcarver989/diff/pull/23))

## [0.1.2](https://github.com/jcarver989/diff/compare/clankerdiff-syntax-v0.1.1...clankerdiff-syntax-v0.1.2) - 2026-09-10

### Added

- Streaming rendering ([#21](https://github.com/jcarver989/diff/pull/21))

### Added

- Retained Arborium Tree-sitter documents, incremental query/injection invalidation, shared line projections, and parser/query/projection work counters.
- Full-document and streaming comparison against Arborium, including long multiline context, UTF-8, EOF edits, injection depth limits, boundary reinterpretation, and input-limit atomicity.

### Changed

- One document-highlighting API now accepts a `Fingerprint`, language hint, and lazy exact-source closure. `SyntaxError` replaces `SyntaxStreamError`; callers handle typed failures for complete documents and streams.
- Theme changes recolor retained captures without reparsing. Complete source context is retained without a fixed lookbehind window.

### Removed

- F# grammar coverage and its aliases are no longer bundled. Unsupported hints use plain-text highlighting; consumers should not assume parity with the previous grammar catalog.
- The syntax crate's `SourceSequenceId` re-export and separate line-sequence highlighting entry points; line-oriented consumers assemble consistently encoded source lazily at the document cache boundary.

## [0.1.1](https://github.com/jcarver989/diff/compare/clankerdiff-syntax-v0.1.0...clankerdiff-syntax-v0.1.1) - 2026-09-09

### Other

- Crate libs.  ([#17](https://github.com/jcarver989/diff/pull/17))

## [0.1.0](https://github.com/jcarver989/diff/releases/tag/diff-syntax-v0.1.0) - 2026-08-31

### Added

- *(syntax)* expand embedding API and language support

### Other

- *(syntax)* Simplify
- stash wip
- *(syntax)* Update tests
- Cleanup cache key logic
- add diff syntax crate
- shuffle files around and break up crates
