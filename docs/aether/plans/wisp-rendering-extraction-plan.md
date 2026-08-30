# Wisp Rendering and Review Extraction Plan

## Overview

### Problem statement

Wisp still owns generic implementations for syntax highlighting, Markdown rendering (including append-only streaming), inline diff presentation, Git review, and plan review. This duplicates or overlaps code already present in this workspace and makes those capabilities difficult to evolve independently.

The goal is to make this workspace the long-term owner of those capabilities and reduce Wisp to an application adapter that:

- translates ACP events into reusable diff/Markdown inputs;
- supplies Wisp-specific theme values and transcript layout;
- launches `clankerdiff` for modal Git and plan reviews;
- maps typed review results back into prompts or ACP elicitation responses.

This plan is intentionally scoped to `/Users/josh/code/diff` only. It prepares publishable APIs and CLI contracts that Wisp can consume in a separate future migration; it does not modify, remove, or test code in the Aether/Wisp repository.

### Architectural outcome

Use portable domain/engine crates plus **one Ratatui adapter crate with feature flags**, rather than creating one Ratatui crate per capability:

```text
diff-theme
    renderer-neutral colors, syntax themes, Markdown/diff palettes, theme import

diff-syntax
    language resolution, Arborium highlighting, bounded caches, sequence/stream sessions

diff-markdown
    semantic Markdown documents, streaming source stabilization, anchors/review sessions

diff-core
    diff models, parsing, presentation, anchors/review sessions (no Markdown or syntax engine)

diff-ratatui
    features: syntax, diff-preview, diff-review, markdown, markdown-review

diff-gpui
    GPUI adapters for diff/Markdown; depends on the same portable crates

clankerdiff-protocol
    typed and versioned subprocess result/capability envelopes

clankerdiff
    Git/Markdown application and CLI orchestration
```

The intended Wisp dependency set is:

```toml
diff-core = "..."
diff-theme = { version = "...", features = ["tm-theme"] }
diff-syntax = { version = "...", features = ["common-languages"] }
diff-ratatui = {
    version = "...",
    default-features = false,
    features = ["syntax", "diff-preview", "markdown"],
}
clankerdiff-protocol = "..."
```

The resulting public surface is designed so a future Wisp migration can invoke the `clankerdiff` executable for full Git and plan review without linking `diff-git`, `diff-review`, or `markdown-review` UI state.

### Success criteria and acceptance conditions

1. **Syntax highlighting**
   - `diff-syntax` is usable without `diff-core`, Ratatui, GPUI, Git, Tokio, or pulldown-cmark.
   - Its public output consists of ordered, non-overlapping UTF-8 byte spans and opaque sequence/stream state; no Arborium/Tree-sitter implementation types leak into public APIs.
   - A downstream terminal application can replace its Syntect/two-face implementation by depending only on `diff-theme`, `diff-syntax`, and the terminal adapter's `syntax` feature.

2. **Markdown, including streaming**
   - `diff-markdown` owns semantic parsing and append-only source stabilization.
   - `diff-ratatui`'s `markdown` feature exposes read-only whole-document and streaming line rendering suitable for a transcript, without pulling in review controls.
   - Arbitrary append chunking produces the same final visible text/layout as one-shot rendering at the same width/theme. Representative multiline code constructs preserve syntax state within the documented sequence context bound.
   - A downstream terminal application can render whole and streaming Markdown without directly depending on pulldown-cmark or implementing its own fence scanner.

3. **Inline diff previews**
   - `diff-ratatui`'s `diff-preview` feature exposes a compact, bounded line-producing API that reuses `diff-core::DiffPresentation` and `diff-syntax`.
   - A downstream host can replace a preview snapshot repeatedly while a tool is pending/in progress.
   - The preview API contains all split/unified pairing and diff styling logic, so consumers need only translate their event payload into `FileDiff`.

4. **Full Git and plan review**
   - `clankerdiff --format=json` uses types from `clankerdiff-protocol`, emits a response for submitted and cancelled reviews, and documents stdout/stderr/exit-code behavior.
   - Git TUI review can run directly in the current terminal; it does not require the private loopback attach protocol for downstream integration.
   - Plan review can review an exact in-memory snapshot via a temporary file while preserving a logical `source_path` in feedback.
   - The CLI supports current-terminal execution with machine-readable stdout, inherited interactive stdin/stderr, and deterministic exit codes, so a future terminal host can suspend itself and invoke it safely.
   - Full Git and Markdown review behavior is available through the CLI without requiring consumers to link or reimplement review screens, diff parsing, or Git mutation logic.

5. **Distribution and compatibility**
   - Portable crates are consumable from Aether through versioned releases (registry packages or immutable Git tags during rollout).
   - Public Rust APIs follow semver, serialized protocol types have an explicit independent protocol version, and golden compatibility fixtures exist.
   - Existing desktop, web, TUI, and CLI behavior remains functional.

### Explicit non-goals for this migration

- Do not add a filesystem watcher or incremental Git patch transport. ACP tool updates drive inline previews; full Git review uses complete repository snapshots.
- Do not create a plan-specific semantic crate. Plan review remains a configured Markdown review until plans require domain concepts beyond Markdown targets/comments/decisions.
- Do not make the private loopback HTTP attach protocol part of the public integration contract. Keep it for external-terminal launching, or version it later as a separate follow-up.
- Do not combine GPUI and Ratatui into one UI crate. The “one terminal crate” decision applies to Ratatui adapters only.

## Technical Approach

### 1. Portable dependency boundaries

Move renderer-neutral concerns out of `diff-core`:

- `diff-theme`: move the neutral color/style/theme model from `theme.rs`. Split syntax, Markdown, and diff palettes while retaining a convenience aggregate theme for review applications.
- `diff-syntax`: move `highlight.rs`, `language.rs`, and syntax-specific tests. It depends on `diff-theme` only.
- `diff-markdown`: move `markdown.rs`, `markdown_anchor.rs`, `markdown_review.rs`, and `markdown_session.rs`. It depends on `diff-theme` only if a neutral Markdown palette is genuinely needed by a domain API; otherwise keep it theme-free.
- `diff-core`: retain diff model/parser/presentation/anchors/review/session. Remove syntax rendering from `DiffPresentation`; presentation describes rows and source sequences but does not execute a highlighter.

Recommended dependency direction:

```text
diff-theme
   ↑          ↑
diff-syntax   diff-markdown
   ↑          ↑
   └──── diff-ratatui ──── diff-core
   └──── diff-gpui ─────── diff-core

diff-git ─────────────────↑

clankerdiff-protocol ── diff-core + diff-markdown(review types)
clankerdiff ─────────── protocol + git + ratatui/gpui applications
```

Avoid optional-dependency cycles. In particular, `diff-syntax` must not depend on `diff-core`, and `diff-markdown` must not depend on a renderer.

### 2. Theme model and migration

Replace the current coupling where `SyntaxHighlighter` accepts `&DiffTheme` with explicit types:

```rust
pub struct SyntaxTheme { /* capture map + stable revision */ }
pub struct MarkdownPalette { /* heading/link/quote/code roles */ }
pub struct DiffPalette { /* added/removed/gutter/selection roles */ }

pub struct ReviewTheme {
    pub syntax: SyntaxTheme,
    pub markdown: MarkdownPalette,
    pub diff: DiffPalette,
    // base foreground/background/accent/muted as needed
}
```

Provide builders and accessors rather than requiring callers to construct private maps:

```rust
let syntax = SyntaxTheme::builder("wisp")
    .capture("keyword", style)
    .capture("string", string_style)
    .build()?;
```

Keep stable theme revisions derived from all relevant values so syntax and renderer caches invalidate correctly.

Wisp currently reads Sublime `.tmTheme` files through Syntect. Preserve that user-facing capability without retaining syntax implementation in Wisp by adding an optional `tm-theme` feature to `diff-theme`:

```rust
ReviewTheme::from_tm_theme_bytes(id, bytes)?
```

The importer should derive base/Markdown/diff roles and map supported Sublime scopes onto known Tree-sitter capture names. Keep this feature optional so browser and consumers of built-in themes do not compile Syntect. Document unsupported scope fallbacks. Downstream applications can later migrate users to the native versioned theme JSON format.

### 3. Syntax API

Expose three usage levels:

```rust
pub struct SyntaxHighlighter { /* bounded shared cache */ }
pub struct SyntaxSequence { /* opaque continuation/context */ }

impl SyntaxHighlighter {
    pub fn highlight(
        &mut self,
        theme: &SyntaxTheme,
        hint: LanguageHint<'_>,
        source: &str,
    ) -> Arc<[HighlightSpan]>;

    pub fn highlight_lines<'a>(
        &mut self,
        theme: &SyntaxTheme,
        hint: LanguageHint<'_>,
        lines: impl IntoIterator<Item = &'a str>,
    ) -> Vec<Vec<HighlightSpan>>;

    pub fn begin_sequence(
        &mut self,
        theme: &SyntaxTheme,
        hint: LanguageHint<'_>,
    ) -> SyntaxSequence;

    pub fn append_lines<'a>(
        &mut self,
        sequence: &mut SyntaxSequence,
        lines: impl IntoIterator<Item = &'a str>,
    ) -> Vec<Vec<HighlightSpan>>;
}
```

`LanguageHint` should distinguish an info string, language ID, and path without string conventions:

```rust
#[non_exhaustive]
pub enum LanguageHint<'a> {
    Id(&'a str),
    InfoString(&'a str),
    Path(&'a str),
    Auto,
}
```

Implement `SyntaxSequence` using the existing bounded preceding-context behavior first. Retain raw context internally up to `PARSE_CONTEXT_LINES`, never expose Arborium parse state, and document that constructs beginning beyond the context bound rely on parser recovery. This gives one engine and one API for diff viewports and streaming code blocks without retaining Syntect solely for continuation state.

Feature-gate grammar bundles in `diff-syntax`. Define a stable `common-languages` feature that includes the current common set, and add Wisp-critical missing grammars after inventorying current Syntect fixtures (at minimum C#, Java, Kotlin, Ruby, Swift, PHP, SQL, Lua, and a real Dockerfile grammar where available). Unknown languages must return plain spans rather than errors.

### 4. Markdown stream model

Port Wisp's stable-prefix behavior into `diff-markdown`, not directly into Ratatui. Keep whole-document `MarkdownDocument` snapshots as the canonical semantic model and add append-oriented source state:

```rust
pub struct MarkdownStream { /* source, revision, stable split, open fence */ }
pub struct MarkdownStreamUpdate {
    pub revision: u64,
    pub newly_stable: Range<usize>,
    pub unstable_tail: Range<usize>,
    pub reset: bool,
}

impl MarkdownStream {
    pub fn new() -> Self;
    pub fn push(&mut self, chunk: &str) -> MarkdownStreamUpdate;
    pub fn replace(&mut self, source: impl Into<String>) -> MarkdownStreamUpdate;
    pub fn finish(&mut self) -> MarkdownStreamUpdate;
    pub fn source(&self) -> &str;
    pub fn stable_offset(&self) -> usize;
    pub fn continuation(&self) -> Option<&FenceContinuation>;
}
```

Port the proven CommonMark rules from Wisp:

- prose becomes stable only at safe blank-line boundaries;
- complete lines inside top-level open fenced code blocks may stabilize;
- list/blockquote-contained fences remain unstable until closed;
- fence character and run length are respected;
- the closing fence is held with the tail so finished block spacing remains correct.

`FenceContinuation` may expose a method needed by renderers (`opening_line`) but should not expose mutable parser internals.

### 5. One feature-gated terminal crate

Keep `diff-ratatui` as the single terminal adapter package. Use this feature matrix:

```toml
[features]
default = ["diff-review", "markdown-review"]
syntax = ["dep:diff-syntax", "dep:diff-theme"]
diff-preview = ["syntax", "dep:diff-core"]
diff-review = ["diff-preview", "dep:crossterm"]
markdown = ["syntax", "dep:diff-markdown"]
markdown-review = ["markdown", "diff-markdown/review", "dep:crossterm"]
```

The exact default can remain backward compatible, but CI must check meaningful minimal combinations individually. Gate modules and exports, not just dependencies. A transcript-style downstream TUI uses `syntax + diff-preview + markdown`; `clankerdiff` uses the review features.

Under `syntax`, expose neutral-span-to-Ratatui conversion helpers so Wisp does not recreate adapters:

```rust
pub fn highlighted_line(
    source: &str,
    spans: &[HighlightSpan],
    base: Style,
) -> Line<'static>;
```

Under `markdown`, expose a read-only renderer and stream state, separate from `MarkdownReviewWidget`:

```rust
pub struct MarkdownRenderer { /* width/theme-independent services */ }
pub struct StreamingMarkdownState { /* stable rows, source offset, code continuation */ }
pub struct MarkdownRenderOptions { pub width: u16, /* spacing policies */ }

impl MarkdownRenderer {
    pub fn render_lines(... ) -> Arc<[Line<'static>]>;
    pub fn render_stream_lines(
        &mut self,
        state: &mut StreamingMarkdownState,
        stream: &MarkdownStream,
        options: MarkdownRenderOptions,
        theme: &ReviewTheme,
        highlighter: &mut SyntaxHighlighter,
    ) -> Arc<[Line<'static>]>;
}
```

The stream state owns only renderer concerns: rendered stable rows, width/theme cache key, and syntax continuation. A downstream host retains only its own item-ID-to-`StreamingMarkdownState` map because conversation identity does not belong in this workspace's renderer API.

Under `diff-preview`, expose a compact line-producing API, not the full review state:

```rust
pub struct DiffPreviewOptions {
    pub max_content_rows: usize,
    pub view_mode: ViewMode,
    pub include_hunk_headers: bool,
    pub overflow_summary: bool,
}

pub fn render_diff_preview(
    file: &FileDiff,
    width: u16,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
    options: DiffPreviewOptions,
) -> Vec<Line<'static>>;
```

Default to 20 content rows and an `… N more rows` summary to preserve Wisp behavior. Auto layout uses the existing 96-column split breakpoint and falls back to unified. Reuse `DiffPresentation`; do not copy Wisp's `split_groups`/`pair_changed_block` implementation.

### 6. Move highlighting out of `DiffPresentation`

`DiffPresentation` should remain a cheap row index. Replace `DiffPresentation::highlight_cell` with a neutral source-sequence descriptor, for example:

```rust
pub struct CellSequence<'a> {
    pub language_hint: &'a str,
    pub sequence_id: Fingerprint,
    pub target: usize,
    pub lines: Box<dyn Iterator<Item = &'a str> + 'a>,
}

pub fn cell_sequence(&self, row: &PresentedRow, cell: &PresentedCell)
    -> Option<CellSequence<'_>>;
```

If boxing proves costly, expose an index/range descriptor plus a borrowing iterator method. `diff-ratatui` and `diff-gpui` then pass that descriptor to `diff-syntax`. This keeps `diff-core` independent of Arborium and theme types while preserving the current bounded sequence behavior.

### 7. GPUI parity

Update `diff-gpui` to consume the split crates. Add syntax highlighting for Markdown code blocks by highlighting the complete code block sequentially before applying GPUI styles. This fixes the current discrepancy where the Ratatui review attempts code highlighting but GPUI Markdown is plain. Renderer-specific style conversion remains in GPUI/Ratatui crates.

### 8. Versioned CLI protocol

Create `clankerdiff-protocol` with no Clap, terminal, Git process, GPUI, or Ratatui dependencies. Use typed tagged responses instead of anonymous `serde_json::json!` values:

```rust
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(tag = "document_kind", rename_all = "snake_case")]
pub enum ReviewResponse {
    Diff {
        protocol_version: u32,
        outcome: ReviewOutcome,
        repository_root: PathBuf,
        scope: DiffScope,
        submission: Option<ReviewSubmission>,
    },
    Markdown {
        protocol_version: u32,
        outcome: ReviewOutcome,
        source_path: Option<String>,
        submission: Option<MarkdownReviewSubmission>,
    },
}

#[non_exhaustive]
pub enum ReviewOutcome {
    Approved,
    ChangesRequested,
    Cancelled,
}
```

Also define a typed capability response and add `clankerdiff capabilities --format=json`, reporting supported protocol versions, UIs, current-terminal TUI support, and review kinds.

Protocol guarantees:

- JSON mode writes exactly one response object plus a trailing newline to stdout for submitted **and cancelled** reviews.
- Diagnostics and interactive TUI rendering never write to stdout; use stderr for the TUI and diagnostics.
- Exit code `0` means submitted, `2` means cancelled, and `1` means process/input/protocol failure.
- `submission` is absent only for cancellation.
- Unknown JSON fields are accepted for forward compatibility; `clankerdiff-protocol` provides a validator that rejects unsupported protocol versions with a clear error.

### 9. Current-terminal Git review

Keep the external-terminal session launcher for existing workflows, but add a direct current-terminal Git TUI path. Add a command-specific `TuiPlacement::{Current, External}` argument and preserve current defaults (Git external, Markdown current); downstream terminal hosts pass `Current` explicitly.

Refactor `clankerdiff/src/tui.rs` around a private backend interface:

```rust
trait DiffReviewBackend {
    fn apply(&mut self, action: RepositoryAction) -> Result<DiffDocument, String>;
}
```

- `SessionBackend` wraps the existing loopback HTTP client for `attach`.
- `LocalBackend` wraps `GitRepository` and executes an action followed by `snapshot(scope)` using the current Tokio runtime.
- One `run_diff_review(document, backend)` event loop handles both.

Do not expose or version the loopback protocol as part of this migration.

Remove the artificial Markdown stdin/TUI restriction where possible. Document that a current-terminal host with in-memory Markdown should use a temporary file so stdin remains attached to interactive controls. Add `--source-path` metadata so the temporary physical path does not leak into agent-facing comments.

### 10. Public current-terminal subprocess contract

This repository must make foreground integration possible without owning the parent application's terminal lifecycle. The supported invocation contract is:

```text
clankerdiff review --ui tui --tui-placement current --format json <repository>

clankerdiff markdown --ui tui --format json \
  --source-path <logical-plan-path> --title <title> <physical-file>
```

For both commands:

- interactive input is read from inherited stdin;
- the TUI and diagnostics are written only to inherited stderr;
- the final protocol response is written only to stdout;
- stdout may therefore be piped while stdin/stderr remain attached to the controlling terminal;
- JSON mode emits a response for submission and cancellation;
- exit code `0` means submitted, `2` means cancelled, and `1` means failure.

Document the parent-host responsibilities, but do not implement them here: stop its input event reader, restore cooked terminal mode, spawn the child with stdout piped, wait, recreate its terminal/event reader, and force a full redraw. Add a small shell/Rust example that demonstrates these file-descriptor semantics without depending on Wisp.

### 11. Generic host preview contract

Keep event-protocol translation outside this workspace. The reusable boundary is `FileDiff::from_texts(path, old, new)` followed by `render_diff_preview`; a host may replace that `FileDiff` on every in-progress update.

Because `RepoPath` intentionally rejects absolute and traversing paths, document that hosts must normalize paths against their repository root before construction. Do not weaken `RepoPath` invariants for display-only inputs. Add a preview example that updates the same logical preview from successive old/new text snapshots, proving that no watcher or incremental Git transport is required.

## Implementation Steps

1. **Capture migration fixtures before moving code.**
   - Copy representative Wisp syntax, Markdown streaming, diff preview, Git review formatting, and plan review formatting cases into workspace integration fixtures.
   - Add golden plain-text terminal snapshots for widths below/above the split breakpoint and for final streamed Markdown output.
   - Record Wisp performance counters for a long streamed response so the migration has a measurable no-regression target.

2. **Create `diff-theme`.**
   - Add the crate to the workspace.
   - Move `Rgba`, `FontStyle`, `SyntaxStyle`, theme IDs/descriptors, built-in theme loading, and revision hashing from `diff-core/src/theme.rs`.
   - Split `SyntaxTheme`, `MarkdownPalette`, `DiffPalette`, and aggregate `ReviewTheme`.
   - Add builders and `tm-theme` import behind an optional feature.
   - Update built-in theme JSON and tests to include Markdown roles and preserve current Sage/Ayu values.

3. **Create `diff-syntax`.**
   - Move `highlight.rs` and `language.rs` plus tests.
   - Change APIs to accept `&SyntaxTheme` and `LanguageHint`.
   - Preserve complete-source cache, viewport reservation, stats, and bounded sequence behavior.
   - Add opaque `SyntaxSequence`/`append_lines` API implemented with retained bounded context.
   - Split grammar features and define/test `common-languages`.

4. **Make `diff-core` syntax-free.**
   - Remove Arborium and theme dependencies from `diff-core/Cargo.toml`.
   - Replace `DiffPresentation::highlight_cell` with the neutral cell-sequence API.
   - Move palette lookup out of core; renderers map `DiffTone` to `DiffPalette`.
   - Update core unit tests to validate sequence descriptors rather than color spans.

5. **Create `diff-markdown`.**
   - Move semantic parser, anchor, review, and session modules and exports.
   - Add a `review` feature for anchors/comments/session/submission APIs; keep plain parsing available without it.
   - Port Wisp's `Fence`, `StableFence`, complete-line scanner, and stable-offset tests into a public `MarkdownStream` with opaque continuation state.
   - Preserve source byte/line ranges, target IDs, outline, metadata, reconciliation, and serde behavior.

6. **Refactor all workspace consumers to the split crates.**
   - Update `diff-gpui`, `diff-gpui-desktop`, `diff-gpui-web`, `diff-ratatui`, and `clankerdiff` imports/manifests.
   - Update WASM checks to include portable `diff-theme`, `diff-syntax`, and `diff-markdown` configurations.
   - Ensure native-only or Syntect compatibility features are not selected in WASM builds.

7. **Feature-gate the single `diff-ratatui` crate.**
   - Add `syntax`, `diff-preview`, `diff-review`, `markdown`, and `markdown-review` features.
   - Gate files/modules/exports with `cfg(feature = ...)`.
   - Keep existing public review exports available under default features for compatibility.
   - Add CI checks for each minimal feature combination and `--all-features`.

8. **Add terminal syntax adapters.**
   - Move `HighlightSpan` to `Line`/`Span` conversion into the `syntax` module of `diff-ratatui`.
   - Support applying caller-provided foreground/background/base modifiers without punching holes in diff row backgrounds.
   - Test UTF-8 ranges, gaps between highlights, merged spans, and base-style patching.

9. **Add read-only and streaming Markdown rendering.**
   - Refactor reusable visual layout/wrapping out of `markdown_review/layout.rs` so review and read-only views share it.
   - Build syntax spans for an entire code block sequentially before wrapping; do not highlight each visual row independently.
   - Implement `MarkdownRenderer` and `StreamingMarkdownState` under the `markdown` feature.
   - Port Wisp's stable-prefix caching behavior: cache finalized rows, rerender only the unstable tail, reset on width/theme/source replacement, and continue top-level fenced code through `SyntaxSequence`.
   - Keep source gutters, target selection, comments, decisions, and footer controls only in `markdown-review`.

10. **Add compact diff preview rendering.**
    - Implement `DiffPreviewOptions` and `render_diff_preview` under `diff-preview`.
    - Construct a one-file `DiffDocument`/`DiffPresentation` with file headers disabled, resolve auto layout by width, and render content rows through shared cell syntax/style helpers.
    - Implement bounded content-row selection and overflow summary.
    - Ensure the full review widget and preview use the same row/cell rendering primitives.

11. **Update GPUI rendering.**
    - Convert imports to the new theme/syntax/Markdown crates.
    - Reimplement diff cell highlighting from `CellSequence`.
    - Apply sequential syntax styles to GPUI Markdown fenced code blocks.
    - Keep Git and filesystem behavior outside `diff-gpui`.

12. **Create `clankerdiff-protocol`.**
    - Add typed protocol version, outcome, diff response, Markdown response, and capability response types.
    - Add `parse_response` validation for supported versions and kind-specific invariants.
    - Add JSON golden fixtures for approved, changes-requested, and cancelled outcomes.

13. **Refactor `clankerdiff` output and capabilities.**
    - Return typed run outcomes from `run`/`run_markdown` instead of writing anonymous JSON in the middle of orchestration.
    - Serialize protocol types in one output function.
    - Emit cancellation JSON before returning exit code 2 in JSON mode.
    - Add the `capabilities` subcommand and subprocess contract tests.
    - Add `--source-path` to Markdown arguments.

14. **Add current-terminal Git TUI execution.**
    - Add `TuiPlacement` to CLI args.
    - Refactor `tui.rs` around `DiffReviewBackend`, with local and session implementations.
    - Keep `attach` and `CLANKERDIFF_TUI_COMMAND` working for external placement.
    - Exercise repository actions and snapshot replacement in both backend paths.

15. **Prepare release consumption.**
    - Add `version = ...` alongside local `path` dependencies.
    - Enable publishing/releasing for portable crates and `clankerdiff-protocol`; applications may remain unpublished where appropriate.
    - Add crate READMEs, feature documentation, MSRV policy, semver checks, and release order.
    - Release an immutable version/tag before changing Wisp dependencies.

16. **Add downstream-consumer examples and compatibility shims.**
    - Add examples for standalone syntax highlighting, whole/streaming Markdown line rendering, repeatedly replaced diff previews, typed CLI result parsing, and current-terminal subprocess file-descriptor setup.
    - Where needed, add deprecated compatibility re-exports from `diff-core` for one release so existing workspace consumers can migrate without a flag day; document their removal version.
    - Verify examples use only the minimal intended feature sets and do not accidentally pull review, Git, GPUI, or desktop dependencies.

17. **Publish architecture and protocol documentation.**
    - Document crate responsibilities, the `diff-ratatui` feature matrix, language bundles/context limits, Markdown stream semantics, preview behavior, and CLI stdout/stderr/exit contracts.
    - Add a downstream migration guide mapping the generic responsibilities an embedding TUI should delete or retain, without changing that downstream repository.
    - Add immutable protocol fixtures and a compatibility policy independent of crate package versions.

18. **Run final compatibility, performance, and release verification.**
    - Check every meaningful workspace feature combination, native TUI/desktop flows, WASM, docs, semver, and subprocess tests.
    - Compare streaming/highlighting/preview performance against the captured migration fixtures and reject whole-document work per frame where the contract promises stable-prefix reuse.
    - Confirm portable crates have no unintended Ratatui, GPUI, Tokio, Git, or filesystem dependencies and that minimal `diff-ratatui` features do not pull full review code.
    - Publish/tag the reusable crates and CLI only after all compatibility fixtures and examples pass.

## Testing Plan

### Unit tests

#### `diff-theme`

- Built-in theme values and stable revisions.
- Builder exact/parent capture fallback.
- Theme revision changes for every syntax/Markdown/diff field.
- `.tmTheme` role/capture conversion and fallback behavior.
- Malformed and unsupported native theme schema versions.

#### `diff-syntax`

- Language IDs, aliases, paths, special files, shebangs, and info strings.
- Every enabled grammar is resolvable and a disabled grammar falls back to plain text.
- Highlight ranges are ordered, non-overlapping, in bounds, and UTF-8 boundaries.
- Complete-source and line-sequence output for multiline comments/strings.
- Append sequence continuation, reset on language/theme change, and context-bound behavior.
- Bounded cache, sequence eviction, viewport reservation, stats, and zero-capacity behavior.

#### `diff-markdown`

- Existing semantic document/source-range/outline/target tests after extraction.
- Existing anchor/reconciliation/review submission tests under `review`.
- Stable offsets for blank lines, setext headings, lazy continuation, lists, blockquotes, tables, backtick/tilde fences, and longer nested fence runs.
- Append chunks at every Unicode scalar boundary.
- `replace` resets when source is not append-only; `finish` stabilizes the final tail.

#### `diff-ratatui`

- Syntax adapter preserves gaps, modifiers, UTF-8, and base backgrounds.
- Whole Markdown code blocks use sequential highlighting before wrapping.
- Streaming state reuses stable rows and resets for width/theme/source replacement.
- Preview unified/split/narrow rendering, exact old/new numbers, truncation, binary/empty/no-newline cases, and overflow count.
- Minimal-feature compile tests ensure `markdown` does not require review controls and `diff-preview` does not require Git.

#### `clankerdiff-protocol`

- Round trips for every outcome/kind.
- Newer unsupported protocol versions fail clearly.
- Unknown fields are accepted.
- Invalid combinations (submitted without submission, cancellation with submission if forbidden) are rejected.

#### Downstream-facing examples

- Repository-relative path normalization is documented and invalid `RepoPath` values remain rejected.
- Repeated in-progress snapshots can replace the same preview state without stale rows.
- Protocol examples distinguish approved, changes-requested, cancelled, and process-failure outcomes.
- Minimal-feature examples compile without review/Git/application dependencies.

### Integration tests

- Stream each Markdown fixture using every split point and randomized chunk sizes; compare final plain text, row count, and styles to one-shot rendering.
- Render preview and full review rows for the same `FileDiff` and assert semantic row/text/tone parity.
- Execute `clankerdiff` against temporary real Git repositories and assert stdout JSON, stderr separation, repository actions, scopes, and exit codes 0/2/1.
- Execute Markdown review using a temporary physical file plus logical `--source-path` and assert feedback refers to the logical path.
- Exercise current-terminal and external/session Git backends with the same repository-action contract suite.
- Add a PTY-level current-terminal CLI test/harness: run `clankerdiff` with stdin/stderr attached and stdout piped, then verify success, cancellation, malformed input, and child failure leave the terminal restored and stdout protocol-clean.
- Verify approved/changes-requested/cancelled Markdown outcomes are represented exactly once and retain the logical `source_path`.

### Performance tests

- Preserve existing `diff-ratatui` viewport-bounded highlighting contracts.
- Add a long append-only Markdown benchmark asserting stable prose/code rows are not rebuilt after stabilization and cache memory remains bounded.
- Expose and compare `markdown_bytes_parsed`, highlighter bytes, and stable-row rebuild counters in reusable renderer benchmarks; allow documented bounded context reparsing but reject whole-document reparsing per frame.
- Benchmark a large repeatedly replaced diff preview and ensure rendering work is bounded by configured preview rows plus syntax context rather than full review-widget construction per frame.

### Edge cases

- Invalid UTF-8 is rejected at file/protocol boundaries; Rust string chunk APIs remain UTF-8 safe.
- CRLF Markdown and source files.
- Empty Markdown, empty diff, added/deleted files, binary and oversized files.
- Open fenced code block at stream completion; fence in list/blockquote; four-backtick block containing triple backticks.
- Theme change while a message streams.
- Terminal resize before and after foreground review.
- Child interrupted by signal, closes without JSON, emits oversized output, or returns JSON of the wrong review kind.
- A host-provided path outside the repository root remains a documented conversion error.
- Markdown logical source path differs from the physical input file path.
- Current-terminal review has stdout piped while stdin/stderr remain interactive.

## Files to Modify/Create

### `/Users/josh/code/diff`

| Path | Status | Changes |
|---|---|---|
| `Cargo.toml` | Modify | Add `diff-theme`, `diff-syntax`, `diff-markdown`, and `clankerdiff-protocol`; move shared dependencies/features to the appropriate crates. |
| `Cargo.lock` | Modify | Regenerate after crate/dependency changes. |
| `README.md` | Add | Document architecture, crate selection, feature matrix, Wisp embedding, and CLI protocol. |
| `release-plz.toml` | Modify | Add portable/protocol crates and enable intended publishing/releasing. |
| `justfile` | Modify | Add minimal-feature checks, semver checks, protocol tests, and updated WASM checks. |
| `.github/workflows/ci.yml` | Modify | Test minimal `diff-ratatui` feature combinations, protocol subprocesses, WASM, and semver. |
| `crates/diff-theme/Cargo.toml` | Add | New portable theme crate with optional `tm-theme` compatibility feature. |
| `crates/diff-theme/src/lib.rs` | Add | Public theme/palette/style API and builders. |
| `crates/diff-theme/src/import.rs` | Add | Optional Sublime `.tmTheme` importer and capture-role mapping. |
| `crates/diff-theme/assets/themes/*` | Add/Move | Move native Sage/Ayu theme assets from core and extend Markdown roles. |
| `crates/diff-theme/tests/*` | Add | Theme schema, import, fallback, and revision contracts. |
| `crates/diff-syntax/Cargo.toml` | Add | New highlighting crate and grammar feature bundles. |
| `crates/diff-syntax/src/lib.rs` | Add | Stable public highlighter, hint, sequence, stats, and span exports. |
| `crates/diff-syntax/src/highlight.rs` | Add/Move | Move/refactor existing highlighter/cache/sequence implementation. |
| `crates/diff-syntax/src/language.rs` | Add/Move | Move/refactor language resolution. |
| `crates/diff-syntax/tests/*` | Add/Move | Move existing tests and add sequence/language inventory contracts. |
| `crates/diff-markdown/Cargo.toml` | Add | New Markdown crate with optional `review` feature. |
| `crates/diff-markdown/src/lib.rs` | Add | Public parser, stream, and gated review exports. |
| `crates/diff-markdown/src/document.rs` | Add/Move | Move `diff-core/src/markdown.rs`. |
| `crates/diff-markdown/src/stream.rs` | Add | Port Wisp stable-prefix/fence source model. |
| `crates/diff-markdown/src/anchor.rs` | Add/Move | Move Markdown anchor/reconciliation implementation. |
| `crates/diff-markdown/src/review.rs` | Add/Move | Move Markdown comments/decisions/submissions. |
| `crates/diff-markdown/src/session.rs` | Add/Move | Move Markdown review session/draft state. |
| `crates/diff-markdown/tests/*` | Add/Move | Parser, streaming, review, and reconciliation contracts. |
| `crates/diff-core/Cargo.toml` | Modify | Remove Arborium, pulldown-cmark, and theme dependencies; retain diff-only dependencies. |
| `crates/diff-core/src/lib.rs` | Modify | Export diff-only APIs and neutral cell-sequence descriptors. |
| `crates/diff-core/src/presentation.rs` | Modify | Remove highlighting execution; add cell sequence/source APIs. |
| `crates/diff-core/src/theme.rs` | Remove | Move contents to `diff-theme`. |
| `crates/diff-core/src/highlight.rs` | Remove | Move contents to `diff-syntax`. |
| `crates/diff-core/src/language.rs` | Remove | Move contents to `diff-syntax`. |
| `crates/diff-core/src/markdown.rs` | Remove | Move contents to `diff-markdown`. |
| `crates/diff-core/src/markdown_anchor.rs` | Remove | Move contents to `diff-markdown`. |
| `crates/diff-core/src/markdown_review.rs` | Remove | Move contents to `diff-markdown`. |
| `crates/diff-core/src/markdown_session.rs` | Remove | Move contents to `diff-markdown`. |
| `crates/diff-ratatui/Cargo.toml` | Modify | Add the single-crate feature matrix and optional dependencies. |
| `crates/diff-ratatui/src/lib.rs` | Modify | Feature-gate modules/exports and expose syntax, preview, and Markdown APIs. |
| `crates/diff-ratatui/src/syntax.rs` | Add | Neutral highlight-to-Ratatui conversion helpers. |
| `crates/diff-ratatui/src/diff_preview.rs` | Add | Compact bounded preview API. |
| `crates/diff-ratatui/src/markdown.rs` | Add | Read-only whole/streaming renderer facade and state. |
| `crates/diff-ratatui/src/markdown_layout.rs` | Add/Refactor | Shared read-only visual layout/wrapping extracted from review layout. |
| `crates/diff-ratatui/src/render.rs` | Modify | Consume `CellSequence` + `diff-syntax`; share row primitives with preview. |
| `crates/diff-ratatui/src/state.rs` | Modify | Use split theme/syntax crates under review features. |
| `crates/diff-ratatui/src/markdown_review/{mod.rs,state.rs,layout.rs,render.rs,input.rs}` | Modify | Consume shared Markdown layout/renderer and gate under `markdown-review`. |
| `crates/diff-ratatui/tests/widget.rs` | Modify | Update imports and add full-review/preview parity coverage. |
| `crates/diff-ratatui/tests/markdown_widget.rs` | Modify | Add read-only/streaming/review parity and sequential code tests. |
| `crates/diff-ratatui/tests/performance_contract.rs` | Modify | Add streaming bounds and preserve viewport contracts. |
| `crates/diff-gpui/Cargo.toml` | Modify | Depend directly on core/theme/syntax/Markdown crates. |
| `crates/diff-gpui/src/viewer.rs` | Modify | Highlight via neutral cell sequence and split theme types. |
| `crates/diff-gpui/src/diff_view.rs` | Modify | Update syntax/style imports. |
| `crates/diff-gpui/src/markdown_viewer.rs` | Modify | Consume `diff-markdown` and add fenced-code syntax styles. |
| `crates/diff-gpui-desktop/Cargo.toml` | Modify | Update dependency graph/features. |
| `crates/diff-gpui-desktop/src/{app.rs,markdown_app.rs,preferences.rs}` | Modify | Update types/imports and theme loading. |
| `crates/diff-gpui-web/Cargo.toml` | Modify | Select portable WASM features only. |
| `crates/diff-gpui-web/src/lib.rs` | Modify | Update document/theme/protocol imports and commands. |
| `crates/diff-git/Cargo.toml` | Modify | Add versioned core dependency metadata. |
| `crates/clankerdiff-protocol/Cargo.toml` | Add | New no-UI protocol crate. |
| `crates/clankerdiff-protocol/src/lib.rs` | Add | Versioned review/capability result types and validators. |
| `crates/clankerdiff-protocol/tests/fixtures/*.json` | Add | Golden compatibility fixtures. |
| `crates/clankerdiff/Cargo.toml` | Modify | Add protocol/new domain dependencies and versioned paths. |
| `crates/clankerdiff/src/args.rs` | Modify | Add capabilities, `TuiPlacement`, and Markdown logical source path. |
| `crates/clankerdiff/src/main.rs` | Modify | Return typed outcomes, centralize serialization, emit cancellation JSON. |
| `crates/clankerdiff/src/tui.rs` | Modify | Share event loop across local/session backends and support current terminal. |
| `crates/clankerdiff/src/session.rs` | Modify | Keep private external attach backend; adapt to shared TUI backend interface. |
| `crates/clankerdiff/tests/cli_protocol.rs` | Add | Real subprocess stdout/stderr/exit-code contracts. |

## Additional Notes

### Rollout strategy

Implement this as a sequence of reviewable PRs entirely within this repository:

1. portable theme/syntax split;
2. Markdown split and streaming model;
3. `diff-ratatui` features/read-only renderers/preview;
4. protocol and current-terminal CLI;
5. downstream examples, compatibility documentation, and release/tag.

During these PRs, temporary compatibility re-exports from `diff-core` may reduce churn for existing workspace consumers, but mark them deprecated and remove them before `1.0`. Do not leave two independent implementations active long term. Any downstream Wisp migration is a separate follow-up plan after these crates and CLI contracts are released.

### API stability policy

- Use private fields plus constructors/builders for configurable service types.
- Add `#[non_exhaustive]` to public enums expected to grow.
- Treat Rust API semver and JSON protocol compatibility separately.
- Keep renderer-neutral byte ranges and IDs documented, including snapshot-local versus durable identities.
- Do not expose Ratatui `Line`, GPUI types, Tokio handles, Arborium parsers, or Git process types from portable crates.
- Add `cargo-semver-checks` before publishing the reusable crates.

### Documentation updates

Document:

- crate responsibility and dependency diagrams;
- the `diff-ratatui` feature matrix with minimal examples;
- syntax language bundle and context-bound guarantees;
- Markdown append/replace/finish semantics;
- diff preview truncation/layout behavior;
- `clankerdiff` JSON stdout/stderr/exit-code contract;
- downstream executable discovery, capability negotiation, and installation guidance;
- current-terminal versus external-terminal review behavior.

### Follow-up tasks

- If full-snapshot Git refresh becomes visibly expensive, add a separate native `diff-watch` service with debounce/coalescing; do not put filesystem watching in `diff-core`.
- If plans gain tasks/status/dependencies beyond Markdown review, introduce a plan domain crate then.
- If an external application needs the loopback attach API, extract it into a separately versioned protocol crate instead of declaring the current private HTTP implementation stable.
- In a separate downstream migration, prefer native theme JSON where practical and deprecate `.tmTheme` import only after a compatibility window.
- Evaluate true Tree-sitter incremental edits only after profiling the bounded sequence implementation; preserve the opaque `SyntaxSequence` API so that optimization does not break consumers.
