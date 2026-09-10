# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2](https://github.com/jcarver989/diff/compare/clankerdiff-markdown-v0.1.1...clankerdiff-markdown-v0.1.2) - 2026-09-10

### Added

- Streaming rendering ([#21](https://github.com/jcarver989/diff/pull/21))

### Added

- Source-buffer and retained-reference-prefix copy counters, plus regression coverage for growing paragraphs, lists, quotes, and nested fences without rescanning completed semantic prefixes.

- Retained semantic stream parsing with settled-block reuse, reference-definition invalidation, incremental open fences, and actual parser/scan work counters.
- `MarkdownStream::source_revision()` distinguishes source changes from completion and empty resumption. Source revisions advance on nonempty pushes and replacements; stream revisions retain their existing lifecycle semantics.

## [0.1.1](https://github.com/jcarver989/diff/compare/clankerdiff-markdown-v0.1.0...clankerdiff-markdown-v0.1.1) - 2026-09-09

### Other

- Crate libs.  ([#17](https://github.com/jcarver989/diff/pull/17))

## [0.1.0](https://github.com/jcarver989/diff/releases/tag/diff-markdown-v0.1.0) - 2026-08-31

### Added

- *(markdown)* render streaming syntax incrementally

### Other

- add markdown crate
- shuffle files around and break up crates
