# Direct GPUI-to-Rouge migration plan

## Overview

Replace every GPUI implementation in Diff with Rouge, using the local sibling checkout at `../rouge` (`/home/josh/code/rouge`). This is a hard cutover, not an incremental migration. Desktop, hosted web, the shared design system, the native application shell, examples, and renderer tests must all use Rouge when the work is complete.

Preserve the renderer-neutral domain and behavior already present in this repository. Do not rewrite the diff engine, review model, syntax engine, Markdown parser, protocol, client/server architecture, or terminal frontend merely because the graphical renderer changes.

This document is the implementation plan only. Writing it does not authorize application-code changes.

### Non-negotiable migration constraints

1. Move and rewrite the existing renderer crates; do not introduce replacement crates beside functioning GPUI versions.
2. Remove GPUI dependencies and application wiring at the beginning of implementation. Do not retain renderer-selection feature flags, fallback entry points, or a GPUI compatibility facade.
3. Do not build a separate prototype frontend. The actual application is the development target.
4. Do not maintain two implementations of diff presentation, syntax highlighting, review commands, or general-purpose widgets.
5. The implementation branch may temporarily fail to compile. Independently shippable intermediate states are not required.
6. Desktop and browser hosts may have platform-specific adapters, but they share the same Rouge renderer, application views, and review behavior. Platform adapters are not alternative UI implementations.
7. Preserve hosted use cases. Rouge browser support is part of this work, not grounds for silently dropping the web application.
8. Make generic toolkit improvements in Rouge. Keep application styling in `ui/` and Diff concepts in `diff/`.
9. Change generated configuration through Framer and regenerate it. Do not hand-edit generated outputs.
10. Do not preserve the old GPUI-typed public API. Consumers must adopt the Rouge API; there is no compatibility implementation.

### Success criteria

- All graphical entry points render through Rouge; no GPUI package is reachable in their normal or test dependency graphs.
- Desktop diff review, client-driven review, and Markdown review retain their behavior and submission/cancellation outcomes.
- Browser-hosted diff and Markdown review retain their command/event contracts, revision handling, persistence, and remote-client behavior.
- The reusable UI replacement covers the complete existing public component surface, including components not exercised by Diff itself.
- Large diffs remain virtualized, with correct content refresh, stable location anchoring, and bounded mounted UI.
- Text input supports the existing editing behavior, clipboard operations, focus rules, and IME without duplicate input or accidental submission.
- Ratatui, headless CLI builds, and lower-layer crates do not acquire Rouge dependencies.
- Integration, rendering, browser, consumer, dependency-layer, and generated-configuration checks pass.
- Linux Wayland/X11 and macOS windowed verification is complete; headless tests alone are insufficient.

## Current architecture and scope

The five GPUI-related source surfaces contain approximately 14,200 lines of Rust source and 5,400 lines of Rust integration tests. These counts describe the existing surface, not a requirement to replace every line: renderer-neutral decoding, preferences, and boundary behavior can remain.

| Existing path | Responsibility | Migration treatment |
| --- | --- | --- |
| `crates/ui/gpui/` | Shared design system, fonts, icons, themes, pickers, editing, trees, scrollbars, indicators, waveform, chrome | Move to `crates/ui/rouge/`; rewrite against Rouge |
| `crates/ui/gpui-shell/` | Native launch, runtime bridge, menus, preferences persistence, scripting, quit behavior | Move to `crates/ui/rouge-shell/`; replace GPUI lifecycle integration |
| `crates/diff/gpui/` | Diff viewer, client view, Markdown reviewer, comments, navigation, overlays, previews | Move to `crates/diff/rouge/`; keep domain models and replace views/controllers |
| `crates/diff/desktop/` | Native executable and CLI-facing review functions | Keep directory; rename package and replace host wiring |
| `crates/diff/web/` | WASM shell, JS exports, DOM events, local storage, remote sessions | Keep directory; rename package and replace host wiring |

### Foundations to preserve

- `crates/util/`: utilities, remote/process ownership, and LSP services.
- `crates/ui/core/`: renderer-neutral palettes, theme choices, fuzzy matching, selection, and picker behavior.
- `crates/ui/syntax/`: production Tree-sitter language support, syntax spans, incremental caches, and symbol-related syntax data.
- `crates/diff/core/`: `DiffDocument`, `DiffPresentation`, `PresentedRow`, `PresentedCell`, `ReviewSession`, commands, anchors, comments, file tree, and review submissions.
- `crates/diff/markdown/`: semantic parsing, block identities, code blocks, and Markdown review state.
- `crates/diff/client/`, `protocol/`, `server/`, `git/`, `watch/`, and `symbols/`.
- `crates/diff/ratatui/` and CLI behavior unrelated to desktop dependency wiring.

Only extend these APIs when an actual renderer-neutral behavior is missing. Do not introduce a second session or presentation model inside the Rouge views.

### Rouge facts and limitations

The local checkout contains `rouge`, `rouge-core`, `rouge-reactive`, `rouge-render`, `rouge-text`, and `rouge-widgets`.

Existing usable APIs include `View`, `Ui`, reactive properties, rich text, editable controls, focus and typed actions, virtual lists, scroll handles, widgets, native `App`, and headless `TestUi`/`TestUiBuilder`. Text shaping/rasterization uses Parley/Swash; layout uses Taffy; rendering uses wgpu.

`../rouge/crates/rouge/examples/shared/diff/` is a useful reference for virtualized rows, sticky headers, split/unified layouts, and comment input. Its parser, presentation structures, and small syntax lexer are demo implementations, not replacements for this repository's production crates.

Important gaps or validation areas:

- No browser application backend exists. The design explicitly excludes web from v1.
- Virtual-list mounted rows are keyed by index. Same-length replacement and row-kind changes need explicit reactive identity handling.
- Variable heights are supplied through indexed functions, not a complete measured-content virtualization contract. The current implementation allocates per-row height memos and builds prefix positions for variable-height lists.
- `Ui::add_font` exists, but native/browser startup need a clean font-registration point before first layout.
- The clipboard trait is synchronous; browser clipboard operations and permissions are not.
- Native runtime helpers include thread-based work/timers that require target-specific browser implementations or gating.
- Popover collision handling, general SVG-document rendering, color glyphs, and some platform behavior are incomplete or unverified. Preserve existing product behavior where these matter; do not make unrelated framework-roadmap completion a prerequisite.

These are implementation requirements or risks, not reasons to keep GPUI alive.

## Technical approach

### Final crate and package layout

| Current package | Final package | Final directory |
| --- | --- | --- |
| `clankerdiff-gpui-ui` | `clankerdiff-rouge-ui` | `crates/ui/rouge/` |
| `clankerdiff-gpui-shell` | `clankerdiff-rouge-shell` | `crates/ui/rouge-shell/` |
| `clankerdiff-gpui` | `clankerdiff-rouge` | `crates/diff/rouge/` |
| `clankerdiff-gpui-desktop` | `clankerdiff-desktop` | `crates/diff/desktop/` |
| `clankerdiff-gpui-web` | `clankerdiff-web` | `crates/diff/web/` |

Add `rouge-web` inside the Rouge repository as the browser host. It is a proposed new crate, not an existing dependency. Keep the current `rouge` facade as the native host; do not create a second renderer or a Diff-specific browser toolkit.

```text
util/*                         unchanged application-independent utilities
ui/core, ui/syntax             renderer-neutral behavior, themes, syntax
          |
ui/rouge                       themed reusable views
          |
diff/rouge                     Diff and Markdown controllers/views
          |
          +-- diff/desktop --> ui/rouge-shell --> rouge native App
          |
          +-- diff/web ----------------------> rouge-web browser host

rouge native App and rouge-web both use rouge-core/reactive/render/text/widgets
```

`ui/rouge` and `diff/rouge` must not depend on the native `rouge::App` facade or `ui/rouge-shell`. Use core/reactive/widgets directly in portable views. Native runtime features belong on native dependencies, not on root workspace aliases that would enable them in the WASM graph.

### Local dependencies and reproducibility

Declare these root workspace dependencies through Framer, then consume them with `.workspace = true`:

```toml
rouge = { path = "../rouge/crates/rouge" }
rouge-core = { path = "../rouge/crates/rouge-core" }
rouge-reactive = { path = "../rouge/crates/rouge-reactive" }
rouge-widgets = { path = "../rouge/crates/rouge-widgets" }
rouge-web = { path = "../rouge/crates/rouge-web" }
```

Add `rouge-render` or `rouge-text` directly only where their public APIs are actually used. Enable `rouge`'s `tokio-rt` feature in the native shell manifest, and gate native-only dependencies by target. The `rouge-web` path becomes valid when its crate is created during implementation.

Paths are relative to `/home/josh/code/diff`; Cargo does not expand `~`. Do not add the sibling checkout as a member of the Diff workspace or copy Rouge source into this repository.

Both checkouts pin Rust `1.99.1`; Rouge requires MSRV `1.99`, while Diff currently advertises `1.98`. Set Diff's generated workspace MSRV to `1.99` and keep the existing toolchain pin.

CI/release jobs must check out Rouge at a recorded commit into the sibling directory before Cargo metadata, cache-key computation that requires manifests, Framer validation, tests, or packaging. Record the exact Rouge revision in one Framer-owned configuration input; generate checkout steps from it. A local path plus a lockfile does not pin the source of the sibling checkout.

This prototype can build local binaries with path dependencies, but it cannot publish packages depending on unpublished Rouge crates to crates.io. Make that limitation explicit. Disable publishing for affected packages during the prototype; do not invent placeholder registry versions or claim published-consumer parity. Publishing Rouge and switching to registry dependencies is a separate release prerequisite, not another renderer implementation.

### Reactive ownership and proposed view API

Keep one authoritative `ReviewSession` per mounted review. A controller applies commands and publishes small derived state projections. The view must not subscribe every row to an entire cloned session or copy the document on every keystroke.

Proposed application API names, to be implemented in `diff/rouge`:

- `ReviewController`: owns the session and command dispatch.
- `ReviewHandle`: scope-owned handle passed to callbacks and child views.
- `diff_view(handle) -> impl View`: mounts the review tree once.
- `DiffViewerOptions`: renderer sizing and visibility options using Rouge units or plain renderer-neutral configuration, never GPUI types.
- `DiffClientView`: mounts the same review view and applies `ClientState` updates.
- `MarkdownController` and `markdown_review_view(handle)`: use the existing semantic document/review model.

Keep semantic operations recognizable: `set_document`, `set_theme`, `set_capabilities`, `command_context`, `command_enabled`, `handle_command`, `lookup_finished`, `command_finished`, and `clear_review`. These are behavioral continuations, not promises of GPUI signature compatibility.

Controller rules:

1. One dispatch path handles `DiffReviewCommand`; toolbar, menus, keyboard, and pointer handlers use it.
2. Preserve existing command availability checks and `CommandContext` semantics.
3. `ReviewSession` remains authoritative for selection, review comments, and committed draft content. The text widget may own an editing buffer/selection, but synchronization has one direction per update and no independent comment store.
4. Publish equality-checked projections for selection, presentation revision, comments, capabilities, pending state, theme, and overlay state. Batch related updates.
5. Keep `Arc<DiffDocument>` and existing syntax caches. Borrow large state with `with`/untracked access rather than cloning it for every row.
6. Use stable domain anchors for location restoration and persistent review state. Node indexes and mounted row instances are not domain identities.
7. Cancel scoped work and disregard stale asynchronous results when a document, lookup target, remote session, or mounted scope changes.
8. Emit existing domain events. Native and browser hosts handle external effects; views do not perform Git, filesystem, or transport work.

## Implementation steps

The following sequence is one rewrite. Checkpoints make correctness inspectable; none is a separately supported migration state.

### 1. Record the behavioral contract and baseline

Baseline record: [docs/migration-baseline/README.md](docs/migration-baseline/README.md). Source/API/test inventories, consumer contracts, deterministic fixtures, browser screenshots and boundary measurements are recorded at `09a11f7e7fbaa354a2df4743b4ee18ede000a30c`. Native release/platform measurements and human visual signoff remain explicitly outstanding in that record; this does not mark the structural cutover or full native acceptance complete.

Before deleting GPUI code, use existing tests, source, screenshots, and a recorded pre-rewrite revision as the specification. Do not create or retain another runnable implementation in the final tree.

- Inventory every exported symbol in `ui/gpui/src/lib.rs` and `diff/gpui/src/lib.rs`, every desktop public entry point, and every browser JS/event boundary.
- Record mappings from existing renderer tests to replacement tests; a removed test must have a replacement or a specific documented reason.
- Record screenshots for diff, split layout, comments, pickers, Markdown, themes, and interface sizes.
- Capture startup, steady-state memory, scrolling, layout-toggle, and refresh measurements on fixed hardware and representative documents.
- Inventory direct consumers in j-terminal/aether when available. Record their actual component/shell usage; do not assume Diff screens cover the reusable API.
- Record current browser/GPU expectations. Any reduced supported environment is an explicit product change requiring approval, not an implicit consequence of choosing Rouge.

Deliverable: acceptance fixtures and a behavior checklist used by the rewrite tests. Existing source remains available through Git history, not a parallel checkout requirement or CI comparison implementation.

### 2. Perform the structural cutover

Checkpoint: the renderer crates and application packages have their final names, and the native and WASM dependency graphs no longer resolve GPUI. The old graphical entry points are disconnected. The moved renderer source and tests still need the API ports in later steps; this checkpoint does not build the graphical application. The pinned browser skeleton is committed locally in the sibling Rouge checkout and must be pushed before hosted CI can fetch it.

- Move the three renderer directories to their final names, including assets, examples, changelogs, and tests.
- Rename packages, Rust imports, crate exports, desktop dependency names, browser package references, and example launch commands.
- Remove `gpui`, `gpui_platform`, `gpui_web`, and `gpui-base` dependencies and GPUI-specific test-support feature propagation.
- Create the `rouge-web` crate skeleton in Rouge so workspace path resolution is coherent.
- Replace `GpuiApp` with a `RougeApp` Framer preset. Preserve general internal-crate path generation while replacing renderer dependency and build-environment configuration.
- Update `.framer/src/lib.rs::CRATES`; register the web package if required by the revised dependency generation.
- Remove the GPUI method-specific radius lint paths; retain the policy through Rouge token usage and an appropriate check, not obsolete GPUI symbols.
- Update generated MSRV, manifests, CI, package/release settings, and tracked lockfiles through their owning tools.
- Update manually maintained `justfile` recipes outside generated blocks directly; update generated recipes only through Framer.

Acceptance: no application manifest selects GPUI; there is one set of renamed renderer crates. Compilation may remain broken until subsequent steps finish.

### 3. Make Rouge core services host-portable

Checkpoint: the shared Rouge crates type-check for WASM, and headless Chrome exercises browser wakeups, timer cancellation, byte-backed image decoding, and first-frame shaping with a bundled Inter face. Native Rouge integration tests cover the shared edit path, including stale clipboard completions and composition. Diff exposes the existing Inter/Lilex/Monaspace bundle to both hosts. The sibling Rouge changes must be committed and pushed before updating the Framer-owned CI checkout pin. The canvas/GPU and DOM host integration is still step 11; native windowed acceptance is still outstanding.

Work in `../rouge`, using its public APIs and integration-test conventions.

- Audit core/reactive/render/text dependencies for WASM compilation, including Parley/font discovery, wgpu features, channels, timing, randomness, and thread-based helpers.
- Keep the UI runtime and renderer shared. Put event-loop, native clipboard, menu, system-font discovery, and browser API details behind host-specific modules/dependencies.
- Preserve asynchronous `Gpu::new`; browser initialization must await it rather than call `pollster::block_on`.
- Keep blocking GPU waits, headless readback helpers, and OS-thread work off browser execution paths. Target-gate genuinely native functionality instead of stubbing required browser behavior.
- Implement browser wakeups and timers using the browser task/microtask and timer APIs. Native helpers continue using native facilities through the same conceptual runtime contract.
- Make font registration available before the first layout in both hosts. Bundle the existing Inter/Lilex/Monaspace assets and licenses; do not depend on host-installed fonts for primary text.
- Unify editing commands so platform events and programmatic clipboard results enter the same edit/selection/undo machinery.
- Expose only the host interaction needed by embedding applications: creation/disposal, input, cursor/IME state, dirty-frame scheduling, fonts, and clipboard effects. Do not introduce a GPUI-style App/Context/Entity emulation layer.

Acceptance: portable Rouge crates type-check for WASM; native behavior still passes Rouge integration tests. Browser-required helpers do not panic because they tried to spawn a native thread.

### 4. Complete the single virtualization API

Checkpoint: the sibling Rouge checkout extends `virtual_list` with stable `RowKey` identity/revisions and opt-in measured heights. The uniform-height fast path remains; variable-height changes update the prefix index without scanning offscreen rows. The list preserves the top intersecting row and its pixel offset across height changes and keyed replacements. Public Rouge tests cover refresh and pointer targets, keyed reorder/removal, wrapped text reflow and cache invalidation, scroll alignment, empty/hidden/zero-viewport cases, and bounded mounting on 100,000 rows. Diff and Markdown still need to supply domain keys and estimates in steps 7 and 9. Release-profile performance against the Diff baseline remains part of final acceptance. The local Rouge changes are not on the Framer-owned CI pin; publish the sibling changes before repinning.

Extend Rouge's existing `virtual_list`/`VirtualListHandle`; do not add a second application-specific list implementation.

- Support stable row identity/content revision so an already-mounted index can change text, row kind, file, and interaction targets correctly, even if the row count is unchanged.
- Maintain a uniform-height fast path and a variable-height path behind the same public API.
- Add measured-height feedback for wrapped comments/Markdown where required. Cache measurements by stable identity, available width, font/scale, and content revision; discard invalid measurements when those inputs change.
- Avoid mounting all rows to measure them. Use estimates for unmounted content and bounded overscan for mounted content.
- Avoid rebuilding all row-height reactive state on selection, caret, hover, or scroll-only changes. Use an efficient prefix-height index for measured updates if necessary; full presentation replacements may rebuild the index once.
- Preserve a stable item plus intra-item pixel offset while measurements change. Restore domain anchors after document replacement, context expansion, collapse, and split/unified switching.
- Retain `scroll_to_row` alignments, visible-range tracking, mounted row bounds, and sticky-header support.
- Test empty lists, zero-sized viewports, removed anchors, hidden/collapsed items, and repeated resize/scale changes.

Acceptance: public tests prove same-count refresh correctness, bounded mounted rows, variable-height anchoring, and scroll-to behavior. Rendering and input cost do not grow with the number of offscreen rows during ordinary scrolling or selection changes.

### 5. Rewrite `ui/rouge`

Checkpoint: the portable `clankerdiff-rouge-ui` library builds on native and WASM and contains no GPUI implementation. The native gallery now lives in the shared crate and uses the native Rouge facade only as a target-specific dev dependency. Public Rouge integration tests replace the old shared GPUI tests. The [shared coverage map](docs/migration-baseline/shared-rouge-coverage.md) lists replacements and specific omissions. The Diff/Markdown views and host cutover are unchanged by this checkpoint. The sibling changes are local; the Framer-owned CI pin is unchanged.

Verified locally on Linux: 24 shared Rouge integration tests plus 28 renderer-neutral tests passed; 264 affected Rouge toolkit tests (`rouge-core`, `rouge-render`, `rouge-widgets`) passed. Strict Clippy passed for the shared crate on native (all targets) and WASM (library), and for the affected Rouge crates on native and WASM. Scoped formatting, `just layer-check`, the baseline inventory tests, and `git diff --check` in both repositories passed. The gallery presented four native frames and exited successfully with `ROUGE_EXIT_AFTER_FRAMES=4`. The portable library's normal dependency graph excludes GPUI and the `rouge` native facade.

Remaining parity: the compositor does not retain GPUI's per-indicator presentation caps (10 Hz spinner, 30 Hz waveform); window visibility scheduling changed and real minimize/restore has not been exercised. Tiled-window resize suppression and platform titlebar capability/order need host support. Dynamic scrollbar drag under changing extents and tail-follow, and human visual sign-off, remain outstanding. Do not mark the complete shared-surface acceptance item done on the strength of the gallery smoke alone.

Use Rouge primitives/widgets with the existing design language. Do not redesign the product during the renderer replacement.

| Module/group | Required work |
| --- | --- |
| `theme.rs`, `tokens.rs`, `typography.rs`, `interface_size.rs` | Map `ReviewTheme` into Rouge colors/styles; keep rem-based sizing, typography roles, scale controls, and preview/commit/cancel behavior |
| `fonts.rs`, `icon.rs` | Register bundled faces before layout; preserve names, weights, italics, icons, and licenses; convert existing icon geometry to supported paths or add the required reusable SVG support to Rouge |
| `components.rs` | Reimplement surfaces, buttons/variants, icon buttons, badges, rows, segments, notices, empty states, action bars, modal sizing, tooltips, and state styling |
| `picker.rs`, `dialog.rs`, `menu.rs` | Reuse neutral picker behavior; implement filtering, details, selection, keyboard handling, dismissal, focus trapping/restoration, and anchored/collision-aware overlays |
| `inline_edit.rs`, `keymap.rs` | Reuse editable controls and existing shortcuts; preserve submit/cancel and display labels without duplicating command rules |
| `tree.rs`, `resizable.rs`, `scrollbar.rs` | Preserve indentation, disclosure, selection, constraints, pointer capture, bounds, scroll synchronization, and interface scaling |
| `indicator_layer.rs`, `waveform.rs`, `window_chrome.rs` | Port reusable drawing and interactions, including exports used by sibling apps but not current Diff screens |
| `testing.rs`, public prelude | Expose Rouge-based public test builders and cohesive exports; remove all GPUI types |

Use `root_font_size`, theme context, and equality-checked reactive colors/sizing so existing mounted views respond to theme and scale changes. Avoid rebuilding the entire UI to apply a palette.

Acceptance: the component gallery runs through the actual renamed crate; public component tests cover the complete exported surface. Generic widget fixes are implemented once in `rouge-widgets` or `rouge-core`.

### 6. Rewrite review state, commands, and navigation

Checkpoint: `clankerdiff-rouge` now mounts one scope-owned `ReviewController`/`ReviewHandle` around `ReviewSession`. The Rouge root routes keyboard and typed actions through the domain gates, publishes separate selection, presentation, annotation, theme, sidebar and status signals, and emits the existing review events. The controller uses the production file tree, source navigation and symbol lookup cache; compatible snapshots keep valid drafts, and dismissal, target changes and scope disposal guard against stale work.

The phase-six integration suites exercise the mounted root, command and keyboard gates, automatic layout, contextual navigation, composition, picker focus, and repository and lookup outcomes. Verified locally: 14 integration tests, strict native Clippy, WASM library compilation and crate-only Clippy, and the layer and inventory checks. Strict WASM Clippy with workspace dependencies included still reports `arc_with_non_send_sync` in the existing syntax crate. The root currently mounts the controller and overlays; the actual diff rows, virtual-list scroll/anchor wiring, and inline placement of the editor and definition card belong to step 7. The client, Markdown, desktop and web ports are not enabled. Unported legacy integration suites are under `crates/diff/rouge/tests/pending/` (not Cargo test targets); renderer-only and Markdown scenarios in the old mixed suites remain specified by the baseline test map and the recorded Git revision.

Rewrite `viewer.rs`, `viewer/commands.rs`, `viewer/overlay.rs`, `navigation.rs`, and `shortcuts.rs` under `diff/rouge`.

- Implement the controller/handle/view architecture described above.
- Preserve the current `command_context`, `command_enabled`, and command routing decisions instead of deriving them independently in widgets.
- Convert the existing shortcut definitions to Rouge chords/actions with the same OS conventions and interaction scopes.
- Ensure shortcuts in draft inputs, pickers, repository prompts, and other overlays do not leak into the underlying diff.
- Do not use a type-only binding-label lookup to identify every value of a payload-bearing `RunCommand`: display the chord associated with the particular domain command, not whichever binding shares its Rust type.
- Preserve pane focus, selected side, hunk/file movement, first/last/page navigation, layout selection, automatic split thresholds, browse/source modes, and context reveal behavior.
- Scope delayed hover/lookup work to the active target; stale results must not reopen dismissed overlays.

Acceptance: command, keyboard, navigation, and focus tests pass using one controller and the existing domain commands.

### 7. Rewrite diff rendering and review interactions

Checkpoint: the production Rouge Diff view now mounts the existing presentation and syntax model in unified, split and automatic layouts. It uses keyed measured virtual rows, a virtualized resizable file tree, horizontal code clipping, sticky file headers with inset-aware keyboard reveal, and domain viewport anchors. Toolbar, picker, repository and review actions route through the existing controller and command gates. Inline threads and drafts, shaped symbol hit ranges, definition cards and bounded detached previews are mounted; no GPUI fallback or demo model was added.

Verified locally: 28 mounted Diff tests (including the GPU smoke test), `clankerdiff-core` tests with `test-support`, 28 sibling Rouge scroll tests, strict native Clippy, WASM library compilation and crate-only strict WASM Clippy, the layer check and baseline inventory tests. Replacing per-row reactive estimates with a single scaled code-row estimate plus measured heights reduced the same 100,000-row debug regression from about 74 seconds to under half a second locally. This is a test runtime, not the release-profile or host performance comparison.

Full parity is not signed off. A compatible refresh preserves a focused editor’s selection and undo history; a row fully unmounted by scrolling retains the domain draft but not the editor’s transient state. Compact commit/definition formatting and additional legacy interactions remain open in the [Step 7 coverage map](docs/migration-baseline/diff-rouge-coverage.md). Human visual comparison and real native-window/browser acceptance are outstanding. The sibling Rouge changes are local; publish the coordinated changes before repinning CI through Framer. Steps 8 and later are unchanged.

Rewrite `diff_view.rs`, `sidebar.rs`, `toolbar.rs`, `commits.rs`, `review_bar.rs`, `annotation.rs`, `comments.rs`, `comment_editor.rs`, `definition_card.rs`, `diff_preview.rs`, `palette.rs`, and `style.rs`.

- Render existing `PresentedRow`/`PresentedCell` values, including file/hunk headers, gaps, empty sides, renames, binary/status-only entries, and no-newline markers where applicable.
- Use existing `DocumentSyntax`/syntax spans and convert ranges/styles into Rouge `rich_text`/`SpanStyle`. Preserve byte boundaries and symbol hit ranges.
- Keep horizontal clipping/scrolling, gutters, selected side, change indicators, file statistics, sticky-header push-off, and sidebar synchronization correct in both layouts.
- Put syntax/cache work on document or syntax revision changes, not every scroll frame or selection update.
- Reimplement comments with existing anchors and review storage. Preserve inline thread placement, edit/delete, blank rejection, undo-last-comment, counts, and formatted export.
- Configure the editor for the current behavior: auto-grow from 3 to 12 rows, Enter submits, Shift+Enter inserts a newline, Escape cancels, and nonempty discarded drafts require confirmation where they do today.
- Handle IME composition so Enter confirms composition without also submitting the comment.
- Preserve repository confirmations, disabled/pending controls, symbol cards, definition navigation, and diff previews.

Acceptance: review tests and visual fixtures cover both layouts, comments, scaling, themes, and document refresh. No parser, highlighter, review store, or layout model is copied from the Rouge demo.

### 8. Rewrite client-driven view integration

Replace `client_view.rs` without changing the client/protocol model.

- Continue installing snapshots only when `ClientState::snapshot_if_changed` reports a change; apply file-list changes independently.
- Preserve capabilities, connection/loading/error states, background errors, command errors, pending state, and lookup replies.
- Keep review drafts and anchors through compatible snapshot refreshes. Apply the existing session rules when a target genuinely changes.
- Keep review submission/cancel, review count, theme changes, and view-mode changes observable by the host.
- Ensure one repository command is pending when the existing protocol requires it. Completion and content pushes remain separate events.
- Prevent late callbacks from a disposed view or previous remote session from updating the current one.

Acceptance: `client_view_test` equivalents exercise loading, failure/reconnection, unchanged snapshots, changed file lists, pending completion with unchanged content, stale replies, and cleanup.

### 9. Rewrite Markdown review

Rewrite `markdown_viewer.rs` and its `chrome`, `commands`, `content`, `document`, `inline`, and `rows` modules using the existing `MarkdownDocument` and review APIs.

- Preserve semantic block identities, selected block, outline selection, and review anchors.
- Implement headings, paragraphs, nested lists, quotes, rules, tables/alignment, inline emphasis/strong/code/links, and highlighted code blocks.
- Preserve the existing HTML-as-content handling; do not start executing arbitrary document HTML or introduce another HTML/webview renderer.
- Reuse the same application comment editor, theme picker, focus conventions, and command/event routing.
- Preserve code-copy actions, review submission/export, draft confirmation, compact preview rendering, and outline navigation.
- Support wrapping/reflow and variable heights without destroying location, selection, or drafts when width, font scale, theme, or source changes.

Acceptance: Markdown renderer tests and browser-hosted Markdown tests pass; content is still parsed once in Rust through the existing semantic parser.

### 10. Rewrite the native shell and desktop entry points

Rewrite `ui/rouge-shell` and `diff/desktop` around `rouge::App`.

- Replace `launch`, native window options, menus, appearance updates, title handling, chrome, and platform shortcut wiring.
- Preserve standard application/Edit/View/Window menu behavior, including undo/redo/cut/copy/paste/select-all; use native menu support where Rouge provides it and Rouge-rendered menus on the existing Linux path.
- Preserve the current initial/maximized window behavior, minimum size, resizability, and persisted theme/view mode/interface-size behavior.
- Keep `run`, `run_review`, `run_client_review`, and `run_markdown_review` as the CLI-facing semantic entry points; update their implementation and crate paths, not submission formats.
- Build one owned Tokio runtime for desktop I/O and processes. Use Rouge's `spawn_on`/signal delivery or a channel bridge; do not run Tokio socket/process futures on an executor without the needed reactor.
- Preserve cancellation on task/scope disposal. Keep child-process ownership in the existing services; do not introduce shared mutable locking to bridge UI state.
- Closing/cancelling a one-shot review returns `None`; submission returns the existing submission value exactly once.
- Ensure the event loop returns normally, including macOS, so drop/shutdown logic runs. Do not carry forward GPUI-specific Objective-C event-loop workarounds unless the Rouge host actually needs equivalent platform code.
- Preserve debounced preference writes and flush the latest state before runtime disposal. Keep filesystem operations off the UI thread except bounded shutdown handling.
- Port the optional script feature: wait, type, keys, named action dispatch, click/drag, screenshot, and quit. Keep action naming in the application/shell dispatch layer; do not recreate GPUI action reflection.

Acceptance: the CLI default desktop path and standalone desktop executable both use Rouge, review result semantics are unchanged, and shutdown releases clients/processes/tasks without requiring process termination.

### 11. Implement the Rouge browser host

Implement the proposed `../rouge/crates/rouge-web/` crate using `rouge-core::Ui`, the shared renderer, and browser APIs. This is required production work, not an optional future migration.

Proposed host contract:

- Asynchronous mount to a supplied canvas with a root `View` factory and bundled fonts.
- An owned application handle responsible for event listeners, observers, scheduled frames, UI runtime, and GPU resources.
- Explicit disposal/unmount that cancels pending work and removes listeners/observers. Avoid permanently leaking closures for normal host lifecycle.
- No Diff commands, document JSON, or review semantics inside Rouge's host.

Implement:

1. **Initialization and GPU:** await canvas-surface/adapter/device creation; report unsupported GPU or initialization failure visibly to the embedding app. Use the same renderer and shaders; gate native APIs and validate browser-supported limits/formats.
2. **Resize and scale:** use canvas CSS size plus device-pixel ratio; update backing dimensions and logical viewport; handle zero sizes, resize observers, zoom/DPR changes, and surface reconfiguration without a resize loop.
3. **Frame scheduling:** use `requestAnimationFrame` only for dirty UI, pending compositor/caret animation, or necessary GPU work. Reactive wakeups schedule a frame; an idle app stops requesting frames. Never block the browser main thread waiting for GPU work.
4. **Pointer/wheel input:** translate CSS coordinates, buttons, modifiers, wheel delta modes, pointer capture/cancel, drag, context menus, and focus changes. Prevent browser defaults only for events the app consumes.
5. **Keyboard/text/IME:** bridge browser input through an offscreen textarea/input for composition, `beforeinput`/input, selection, and platform text behavior. Convert DOM UTF-16 offsets to the internal byte/grapheme conventions. Do not insert text through both keydown and input/composition paths.
6. **Caret/selection:** expose the active editable's caret/input rectangle and selection through a host-neutral core contract; position the browser text bridge appropriately for IME candidate windows. Keep the Rouge editor authoritative; the bridge is not a second editor.
7. **Clipboard:** handle user-initiated cut/copy/paste through clipboard events and programmatic copy through async browser APIs. The current synchronous `Clipboard::read` cannot wait for a permission-controlled Promise: add an event/completion path into the same edit commands. Treat denied permissions and loss of focus as recoverable errors, not successful copies.
8. **Platform services:** expose browser-correct primary modifiers, timers, external URL opening, focus/visibility, and frame/runtime wakeups. Compile-time `target_os = "macos"` is not enough to determine shortcut behavior for WASM running on macOS.
9. **Fonts:** load/register the same bundled fonts before measuring the first screen. Validate WASM shaping without native font-discovery assumptions.
10. **Recovery and disposal:** handle lost/outdated surfaces, recoverable device errors, tab visibility, unmount, and remount without retaining stale callbacks or duplicate canvases.

Use the current real-browser test harness in Chromium with a configured working GPU backend, and windowed/manual browser verification where needed. Record the supported browser/GPU matrix. If matching a previously supported environment requires additional shared-renderer portability work, that remains a blocker until implemented or an explicit requirement change is approved; do not add a second frontend as a fallback.

Acceptance: the same actual Diff/Markdown root views mount in a browser, accept composed text, resize correctly, support clipboard interactions, stop rendering while idle, and dispose cleanly. A WASM type-check alone is not sufficient.

### 12. Rewire the existing web application contract

Replace GPUI startup and `WebRoot` lifecycle in `crates/diff/web/src/lib.rs` with `rouge-web`. Keep one root that switches between the actual Rouge diff and Markdown views.

Preserve the existing exports:

- `start`
- `set_document_json`
- `set_markdown_document_json`
- `complete_repository_json`
- `set_theme`
- `clear_review`
- `connect_remote`
- `disconnect_remote`
- `dispatch_command_json`
- `set_capabilities_json`

Preserve inbound event names and payloads:

- `diff-review-set-document`, `diff-review-set-theme`, and `diff-review-clear`.
- `diff-review-command`, `diff-review-set-capabilities`, and `diff-review-repository-completed`.
- `markdown-review-set-document` and `markdown-review-clear`.

Preserve outbound events and payloads defined by the current host, including:

- `diff-review-refresh`, `diff-review-set-target`, and `diff-review-document-applied`.
- `diff-review-command-result`, review submission/cancel/change notifications, and `diff-review-connection-state` as currently emitted.
- `markdown-review-submit`, `markdown-review-copy`, and `markdown-review-cancel`.

The implementation must inventory exact existing payloads/event names rather than infer new ones from this summary.

Additional requirements:

- Preserve bare-document and revision-envelope decoding, stale/unchanged decisions, request IDs, and acknowledgement behavior.
- Completing a repository request must unblock the UI even when no changed document follows. Stale completion IDs must not complete a newer request.
- Preserve managed-remote ownership restrictions, connection error/reconnect behavior, `?connect=` startup, and disconnect semantics.
- Preserve current local-storage keys and graceful behavior when storage is unavailable.
- Queue pre-ready host commands deterministically while asynchronous browser mounting finishes; reject duplicates/invalid lifecycle calls consistently. Do not silently drop a document push during GPU initialization.
- Preserve browser clipboard/export host events; distinguish notifying the host from actually writing to the OS clipboard.
- Update WASM output discovery, package references, `index.html`, Trunk configuration, Vitest/Playwright tests, and remote setup for renamed artifacts.

Acceptance: existing browser contract tests run against Rouge with no test-only replacement view. Invalid JSON, stale revisions, denied storage/clipboard, and reconnect/disconnect are covered.

### 13. Complete consumer, configuration, and release integration

- Update CLI imports and the optional `desktop` dependency to `clankerdiff-desktop`; retain `--no-default-features` headless behavior.
- Add an independent Rouge UI consumer fixture, with an isolated `[workspace]`, that depends on the public UI/review APIs and creates/renders real views without GPUI. Do not force the existing portable ratatui fixture to pull a graphical renderer into its default graph.
- Update available sibling consumers to the new APIs as a coordinated breaking change, or record concrete external changes required before their next build. Do not claim they are migrated merely because the Diff workspace compiles.
- Rewrite dependency-graph deny checks in `justfile` to prohibit Rouge packages in Git/watch/ratatui/headless graphs; replacing a `gpui` name pattern with nothing would remove an architectural guard.
- Retain native, WASM, remote-client, browser, syntax, Markdown, and consumer coverage in CI. Add Rouge sibling checkout/setup before dependent jobs.
- Configure native test environments with needed font/system libraries and a working GPU driver for rendered tests. Configure browser tests with a supported GPU backend; do not silently skip rendering when initialization fails.
- Update docs, scripts, executable/package metadata, example commands, and Framer presets consumed by sibling applications.
- Inspect package/release validation for local dependency assumptions. Keep real local/dist packaging checks; explicitly disable or mark unavailable only checks requiring unpublished registry packages.
- The current `package-check` recipes reference `scripts/check-consumer.py`, which is absent in the inspected checkout. Verify whether this is pre-existing repository drift; restore or replace the intended validation deliberately rather than treating that command as already working or removing package validation wholesale.
- Remove stale tracked/generated web artifacts according to the existing asset policy and rebuild outputs; do not accidentally commit `target/` or `node_modules/`.

Acceptance: developer commands and CI describe the same source/dependency layout; native-only feature unification does not break WASM or portable consumers.

### 14. Final audit and hard-cutover completion

- Remove any remaining GPUI source, dependency aliases, lockfile packages, action names, renderer test features, generated preset settings, and obsolete platform dependencies that are no longer needed.
- Historical changelogs and this migration document may mention GPUI; active code/configuration must not reference it as an implementation.
- Ensure there is exactly one shared design system, review view, comment editor, Markdown rendering path, and virtual-list API.
- Complete the verification matrix below and compare fixed-fixture measurements with the baseline.
- Document the tested Diff/Rouge revisions and local checkout instructions.
- Merge/release the rewritten frontend only when desktop and web acceptance pass. There is no desktop-only completion claim for this plan.

If the rewrite must be abandoned, revert the coordinated changes to both repositories. Do not add runtime fallback switches or restore a second renderer inside the migrated architecture.

## Testing plan

### Testing conventions

Use public integration tests under each crate's `tests/` directory, matching module paths where useful. Do not add tests of private implementation details or new private unit-test suites. Move existing tests of public decoders/menu configuration out of inline modules where they are touched.

Use real documents, sessions, and clients wherever possible. Reuse existing document/test builders and in-memory services. Extend Rouge's `TestUiBuilder`, `TestUi`, and `MemoryClipboard` rather than adding bespoke mocks or another rendering harness. Tests return `Result` and use `?` rather than `unwrap`; nonshared helpers go below tests.

### Required suites

| Suite | Coverage |
| --- | --- |
| Shared UI | Complete public exports, states, themes, scaling, font/icon loading, focus, overlay placement, picker search/navigation, resizing, indicators, waveform, inline edit |
| Review controller | Command availability/routing, capabilities, pending state, selection/side, browse/source modes, context expansion, draft/comment lifecycle, domain events |
| Diff rendering | Unified/split/auto layouts, sticky headers, gutters, syntax/symbol hit ranges, collapsed files, empty/status-only/binary entries, long lines and horizontal behavior |
| Virtualization | Same-count replacement, changed row kind/identity, item insertion/removal, measured heights, anchor preservation, offscreen updates, bounded mounts, scale/width changes |
| Text editing | Unicode/graphemes, combining marks, selection, undo/redo, word/line movement, clipboard, paste, multiline auto-grow, Enter/Shift+Enter/Escape, composition |
| Markdown | Existing semantic elements, table alignment, inline links/styles, code copy/highlighting, compact rendering, outline, block identities, comments and reflow |
| Client integration | Loading/failure/reconnect, snapshots/file-list updates, capabilities, pending/unchanged completion, stale lookup/repository results, scope cancellation |
| Native host | Menus, window configuration, preferences debounce/final flush, review result delivery, close/cancel/submit, scripting, cleanup of owned processes |
| Browser host | Real canvas rendering, input/IME bridge, CSS/DPR resizing, async initialization/command queuing, clipboard policy, visibility/idle behavior, disposal/remount |
| Browser contract | All JS exports/events, payloads, revisions/acks, local storage, managed-remote ownership, command replies, diff/Markdown switching |
| Consumers/layers | Independent UI consumer, existing portable consumers, CLI headless build, native feature isolation, no reverse-domain dependencies |

### Performance fixtures and acceptance budgets

Use fixed deterministic fixtures for a small review, a roughly 100,000-row diff, many-file changes, very long lines, many expanded comments, and a large Markdown document. Include Unicode, tabs, empty files, renames, and repeated same-length replacements.

Measure startup-to-first-frame, process memory, mounted nodes, text shaping work, frame CPU time, selection latency, presentation refresh, and layout switches. Record hardware, OS, build profile, scale, GPU, and fixture revisions. Run repeated samples; do not compare different machines or debug builds with release baselines.

Proposed acceptance budgets for the rewrite:

- Mounted rows are bounded by the visible range plus configured overscan; mounted row count does not scale with total document length.
- Ordinary scrolling, hover, caret, and selection changes do not rebuild or reshape the entire presentation.
- After initialization, no continuous frames or layout/paint work occur while idle and unfocused with no active animations. Focused caret behavior remains intentionally scheduled.
- On the recorded baseline machine, p95 interactive frame CPU time stays within a 60 Hz budget and does not regress materially against the old implementation; investigate repeated regressions greater than 10% rather than accepting noise from a single run.
- A changed offscreen row is correct when brought onscreen; a visible same-count replacement is correct before the next presented frame after its state update is processed.
- Repeated open/close, remote reconnect, theme change, and layout switching do not grow retained scopes, listeners, tasks, or GPU resources without bound.

If a fixed budget is infeasible on the baseline hardware, record the measurement and get an explicit budget adjustment. Do not declare performance parity from Rouge demo numbers.

### Verification commands

Run the updated repository recipes after their configuration is migrated:

```sh
just framer-check
just fmt-check
just check
just feature-check
just lint
just test
just wasm-check
just web-test
just remote-test
just watch-test
just consumer-check
just doc-check
just automation-check
just package-check
just bench-check
```

`just package-check` must first be made meaningful as described in step 13. Published-consumer checks are unavailable for unpublished Rouge dependencies and must be identified as such, not reported as passing.

Also run the Rouge repository's formatting, lint, integration-test, and new WASM/browser checks. Native Rouge examples must continue compiling; the existing demo is not a second Diff application to maintain in this repository.

Useful dependency audits after package renaming:

```sh
cargo tree -p clankerdiff-desktop -e normal,build,dev
cargo tree -p clankerdiff-web --target wasm32-unknown-unknown -e normal,build,dev
cargo tree -p clankerdiff-cli --no-default-features -e normal,build
cargo tree -p clankerdiff-ratatui --no-default-features -e normal,build
```

Assert against resolved package names in Cargo metadata/trees: GPUI/pre/base packages must be absent from graphical normal/build/dev graphs; Rouge/wgpu/platform packages must be absent from portable/headless normal/build graphs. Do not use a blanket text search that fails on historical documentation or this plan.

Manual release checks cover Linux Wayland, Linux X11, macOS, and browser IME/clipboard workflows that automation cannot reliably establish. Rendered/headless GPU tests may require dedicated runners; lack of a usable GPU is a test-environment failure, not permission to mark them successful.

## Files to modify/create/remove

Paths beginning `../rouge/` are changes in the sibling repository. All other paths are relative to this repository. Proposed new files may be split internally for cohesion, but retain one implementation/API for each capability.

| Path | Action | Required change |
| --- | --- | --- |
| `crates/ui/gpui/` -> `crates/ui/rouge/` | Move/modify | Rename package, rewrite all exported components, port assets/examples/tests, remove GPUI API |
| `crates/ui/gpui-shell/` -> `crates/ui/rouge-shell/` | Move/modify | Native launch, Tokio bridge, menus, persistence, shutdown, script feature |
| `crates/diff/gpui/` -> `crates/diff/rouge/` | Move/modify | Single review/controller architecture, diff/Markdown/client views, renderer tests |
| `crates/diff/rouge/src/controller.rs` and equivalent Markdown controller module | Add | Scoped ownership, command dispatch, narrow reactive projections, host events |
| `crates/diff/desktop/Cargo.toml`, `src/`, `tests/` | Modify | Rename package and executable metadata as appropriate; replace host without changing CLI review outcomes |
| `crates/diff/cli/Cargo.toml`, `src/main.rs` | Modify | Update optional desktop dependency/imports; preserve terminal/headless behavior |
| `crates/diff/web/Cargo.toml`, `src/`, `tests/` | Modify | Rename package, use Rouge web host, preserve JS/event/protocol contracts |
| `crates/diff/web/index.html`, `Trunk.toml`, `package.json`, test configs/lockfiles | Modify as needed | Canvas/bootstrap/artifact names, actual browser rendering setup, remote tests |
| `crates/diff/web/dist/` | Regenerate/remove under existing policy | Eliminate stale GPUI artifacts; do not hand-edit generated WASM/JS |
| `crates/ui/core/`, `ui/syntax/`, `diff/core/`, `diff/markdown/` | Preserve; minimal extensions only | Reuse existing behavior; add neutral APIs only where the new frontend exposes a real missing capability |
| `tests/rouge-consumer-fixture/Cargo.toml`, `src/` | Add | Isolated consumer compile/render coverage of public reusable UI/review APIs |
| `tests/consumer-fixture/` | Modify only as needed | Preserve existing portable feature coverage and update shared path/config changes |
| `.framer/src/lib.rs` | Modify | Replace `GpuiApp`, internal crate mapping, local Rouge dependency setup, platform build prerequisites |
| `.framer/src/main.rs` | Modify | MSRV, preset application, pinned sibling checkout, WASM/native CI, renderer lint policy, prototype publish settings |
| `Cargo.toml`, toolchain/lint/test/dist/release configuration, generated workflow files | Regenerate | Apply Framer-owned changes; do not edit generated outputs by hand |
| `Cargo.lock`, fixture lockfiles | Regenerate as applicable | Resolve local Rouge packages and remove GPUI packages |
| `justfile` | Modify/regenerate by ownership | Package commands, architecture guards, consumer checks, browser/native checks |
| `scripts/check-consumer.py` or its replacement | Verify then restore/add if needed | Repair current missing validation target and adapt local/published package modes honestly |
| `README.md`, `AGENTS.md`, applicable contributor/build documentation | Modify | Architecture, dependencies, local sibling setup, package names, support matrix, migration instructions |
| `../rouge/Cargo.toml` and dependency manifests | Modify | Register browser host, target-specific features/dependencies, portable text/runtime/renderer configuration |
| `../rouge/crates/rouge-web/Cargo.toml`, `src/`, `tests/` | Add | Generic owned browser host and real browser validation, without Diff dependencies |
| `../rouge/crates/rouge-core/src/scroll.rs`, related tests | Modify | Existing list API: content identity, measured heights, anchoring, bounded reactive work |
| `../rouge/crates/rouge-core/src/{ui,clipboard,editable,event,window}.rs`, related tests | Modify as needed | Font/bootstrap hook, shared edit operations, browser input/clipboard/caret/platform contracts |
| `../rouge/crates/rouge-reactive/src/`, related tests | Modify | Browser-safe wake/timer/async behavior; gate native thread/runtime operations |
| `../rouge/crates/rouge-render/src/`, `rouge-text/src/`, related tests | Modify as needed | Browser-compatible shared renderer/shaping, async initialization, device limits, font handling |
| `../rouge/crates/rouge-widgets/src/`, related tests | Modify as needed | Generic widget fixes such as overlay collision handling and editor behavior, implemented once |
| `../rouge/crates/rouge/src/{app,platform}.rs`, related tests | Modify as needed | Native initialization, font registration, lifecycle, and graceful shutdown |
| `../rouge/design.md`, build/CI configuration | Modify | Update actual browser/native status, supported APIs, dependencies, and verification instructions |

## Risks, decisions, and handling

| Risk | Handling |
| --- | --- |
| Browser backend is a substantial new capability | Include it in the same rewrite acceptance; reuse the existing renderer and views; do not claim native completion equals full completion |
| Retained rows display stale content | Stable identity/revision semantics plus same-count replacement tests before relying on virtualization |
| Wrapped content causes scroll jumps or full-document work | Extend the existing list with measured heights, anchor preservation, and bounded updates; instrument representative fixtures |
| IME/clipboard appears correct in synthetic tests but fails on real systems | Real browser/native verification, one edit engine, correct async/permission and UTF-16 conversion handling |
| Theme/selection changes invalidate everything | Narrow equality-checked reactive projections; cache syntax/presentation independently; measure layout/paint work |
| Local Rouge edits make CI/release irreproducible | Record the sibling revision and check it out consistently; local path dependencies alone do not pin source |
| Cargo feature unification enables native runtime in WASM | Keep native features out of shared aliases; enforce target-specific dependency and compile checks |
| Reusable components or sibling apps are forgotten | Inventory all exports and actual consumers; public consumer fixture; coordinated breaking API update with no adapter |
| New shutdown behavior drops submissions/preferences or leaks processes | Exactly-once outcome delivery, owned runtime lifetime, final flush, disposal and process integration tests |
| Toolkit gaps expand into an unrelated framework rewrite | Implement only capabilities needed for existing behavior and browser hosting; track unrelated roadmap work separately |
| Publishing is blocked by unpublished Rouge crates | Explicit local prototype status, affected-package publish settings, deterministic binary builds; publishing remains a release prerequisite |
| Branch is temporarily nonbuilding | Accepted hard-cutover constraint; keep work ordered by contracts and ownership, not by retaining an old renderer |

## Additional notes

### Documentation and handoff

- All API names identified as proposed are new design targets, not claims about existing Rouge functionality.
- Update build documentation with the required sibling checkout, tested Rouge revision, native libraries/GPU requirements, browser support, and local-only publishing limitations.
- Document the breaking public crate/API rename for j-terminal/aether consumers. This repository cannot promise externally maintained consumers are updated without checking their code.
- Preserve asset licenses and historical changelogs. New crate descriptions and examples must describe Rouge, not GPUI.
- Follow existing repository conventions: `thiserror`, cohesive APIs, scoped/channel-based ownership over locks, public integration tests, and no new source-code comments unless explicitly requested.

### Deferred work

These are not excuses to leave GPUI or duplicate implementations behind:

- Publishing Rouge and enabling registry-based release/published-consumer checks.
- Visual redesign, new compositor effects, and unrelated framework roadmap work.
- Accessibility enhancements beyond an explicit no-regression assessment of current product requirements.
- Additional platforms or browser/GPU configurations beyond the agreed existing support matrix.

### Implementation tracking

When implementation is authorized, create work items matching steps 1-14, with dependencies reflecting the shared framework contracts. Independent test-fixture, design-system, and host work can proceed concurrently after those contracts are established, but there is still one implementation and one final cutover. Do not create implementation tasks that assume a GPUI fallback or a separate prototype application.

## Final acceptance checklist

- [x] All three renderer directories and all GPUI-named application packages have their final names.
- [x] Graphical normal/build/test dependency graphs contain no GPUI/pre/base packages.
- [ ] One Rouge-based shared design system covers the complete public component surface.
- [ ] One review controller/view path uses existing domain/session/presentation APIs.
- [ ] One Markdown rendering path uses the existing semantic parser/review model.
- [x] One virtual-list API handles content refresh, measured heights, and anchoring correctly (Rouge toolkit tests; Diff integration follows).
- [ ] Desktop standalone, CLI diff review, client review, and Markdown review pass.
- [ ] Browser diff/Markdown review, remote sessions, exports/events, revisions, and acknowledgements pass.
- [ ] Real text composition, clipboard, focus, resize/DPR, and shutdown behavior are verified.
- [ ] Large-document performance and resource-lifetime checks meet recorded budgets.
- [ ] Ratatui, headless CLI, portable consumers, and dependency-layer boundaries remain intact.
- [ ] Generated configuration matches its Framer source; sibling Rouge checkout is reproducible.
- [ ] Local/dist package checks are meaningful; unpublished registry limitations are explicit.
- [ ] Shared consumer breakages are resolved or concretely documented for coordinated updates.
- [ ] Both repositories pass their applicable native, WASM, integration, rendering, and browser checks.
- [ ] No fallback switch, GPUI adapter, parallel frontend, or copied demo domain engine remains.

## Research references

Repository findings are based on the local checkout inspected while preparing this plan; tests and builds have not been run as part of planning.

- Diff configuration ownership: `.framer/src/main.rs` and `.framer/src/lib.rs`.
- Existing public component surface: `crates/ui/gpui/src/lib.rs`.
- Existing session/command routing: `crates/diff/gpui/src/viewer.rs` and `viewer/commands.rs`.
- Existing client integration: `crates/diff/gpui/src/client_view.rs`.
- Existing native review lifecycle: `crates/diff/desktop/src/lib.rs` and `crates/ui/gpui-shell/src/`.
- Existing browser contract: `crates/diff/web/src/{lib,commands}.rs` and browser tests.
- Rouge actual exports/startup: `../rouge/crates/rouge/src/{lib,app}.rs`.
- Rouge virtualization/editing/embedding: `../rouge/crates/rouge-core/src/{scroll,editable,clipboard,ui}.rs`.
- Rouge example reference: `../rouge/crates/rouge/examples/shared/diff/`.
- Rouge design/status: `../rouge/design.md`; distinguish planned features from the current exported implementation.
- [MDN Clipboard API](https://developer.mozilla.org/en-US/docs/Web/API/Clipboard_API): asynchronous browser access, secure contexts, user-activation/permission restrictions, and cut/copy/paste events.
