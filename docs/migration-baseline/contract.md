# Rewrite behavioral contract

Baseline: `09a11f7e7fbaa354a2df4743b4ee18ede000a30c`. These are acceptance requirements for the replacement, **not** claims that Rouge already implements them. The [source inventory](api.json), [named test map](test-map.json), fixtures and Git source are the detailed specification. Preserve public behavior without preserving GPUI types or an executable GPUI fallback.

## Diff renderer

Root exported symbols, as recorded in `crates/diff/gpui/src/lib.rs`:

- Modules `style`, feature-gated `testing`; `ui` reexports the shared UI crate.
- `DiffViewer`, `DiffViewerOptions`, `DiffViewerEvent` (alias of domain `DiffReviewEvent`), `ViewerPane`.
- `DiffClientView`, `DiffClientEvent`, `PatchPreview`.
- `ThemeChanged`, `ViewModeChanged`.
- `RunCommand`, `CancelComment`, `SubmitComment`, `ConfirmDiscard`, `CancelRepositoryPrompt`.
- `MarkdownReviewer`, `MarkdownReviewerOptions`, `MarkdownReviewerEvent` (domain event alias).
- Markdown action markers: `MarkdownAddComment`, `MarkdownApprove`, `MarkdownCancel`, `MarkdownCopyReview`, `MarkdownDeleteComment`, `MarkdownEditComment`, `MarkdownFirstTarget`, `MarkdownLastTarget`, `MarkdownNextHeading`, `MarkdownNextTarget`, `MarkdownOpenSelected`, `MarkdownPageDown`, `MarkdownPageUp`, `MarkdownPreviousHeading`, `MarkdownPreviousTarget`, `MarkdownRequestChanges`, `MarkdownShowShortcuts`, `MarkdownShowThemePicker`, `MarkdownSubmitComment`, `MarkdownToggleFocus`, `MarkdownToggleOutline`, `MarkdownUndoComment`.

The replacement may change construction/lifetimes/action types, but every semantic use needs a counterpart. Public style helpers include syntax highlight conversion and added/deleted stat badges. Public test builders/drivers need public integration-test replacements, not compatibility wrappers.

- [ ] One authoritative `ReviewSession`/domain command path for keyboard, toolbar, menu, pointer and host commands; preserve `command_context`, availability, capability and modal/pending gates.
- [ ] Unified and split patch layouts, old/new line numbers, syntax/inline changes, file headers, stats, stage/status indicators, binary/empty files, rename paths and missing-newline markers.
- [ ] Source mode uses full old/new snapshots, including out-of-hunk content; preserves independent location/scroll through refresh, resize and return to patch view. Source-mode/comment/full-file command restrictions remain consistent. Patch-only sources fail gracefully rather than manufacturing full-file content.
- [ ] Navigation by row/file/hunk/commit, first/last/page, focus transfer, file tree, collapse/expand/context gaps, source peek/definition navigation, selection and stable anchor restoration.
- [ ] Equal documents do not reset location/drafts or force unnecessary work; changed same-count content repaints syntax, rows, headers and row kinds, not just list length. Refresh can preserve or mark outdated review anchors using domain reconciliation.
- [ ] Large diffs remain virtualized with bounded mounted content, correct variable comment heights, stable sticky headers, split column geometry, horizontal scrolling and thumb math. Indexes are not durable domain identities.
- [ ] Comment create/type/submit/cancel/edit/delete/undo, multiline wrapping, old/new-side anchors, draft focus, selected comment rendering and exact structured/formatted export.
- [ ] Text input selection/navigation, paste/copy/cut, undo/redo, newline/submit separation, IME composition and focus restoration. Do not emit duplicate input or submit a composition accidentally.
- [ ] Repository actions, refresh and target/scope requests emit domain events, respect capabilities and pending state, display errors, and settle only on their corresponding completion. An unrelated snapshot or background-health update cannot settle a repository command.
- [ ] Theme picker search/selection/preview/cancel, command palette/help/overlays, discard confirmation and resizable sidebars preserve focus, keyboard precedence and dismissal rules.
- [ ] `DiffClientView` applies real client snapshots/capabilities/pending/errors and translates effects using the existing client API, not a second session or transport model.
- [ ] `PatchPreview` parses detached patches with the real parser, reports added/deleted stats, bounds previews to 120 rows, provides open-more behavior, and renders useful parse-error text rather than panicking.

Key source-test families: `diff_view_test`, `rendered`, `commands_test`, `keyboard`, `navigation_test`, `sidebar_test`, `commits_test`, `comment_editor_test`, `client_view_test`, `diff_preview_test`, `toolbar_test`, `palette_test`, `definition_card_test`. Every named test has a destination in `test-map.json`.

Known existing deficiency: `grapheme_navigation_and_deletion_keep_combining_text_and_emoji_intact` is ignored because gpui-base uses scalar cursor boundaries. This is recorded, not silently removed or represented as passing. Preserve the intended test for the new input system.

## Markdown renderer

- [ ] Parse the existing `MarkdownDocument`, preserve semantic block IDs/source ranges, and use the existing review model rather than reparsing into a second review engine.
- [ ] Headings/outline, nested and ordered lists, quotations, tables/alignment, inline strong/emphasis/code/links, separators and highlighted fenced blocks; HTML remains content rather than executable host markup.
- [ ] Reflow on viewport/interface-size changes without moving anchors to unrelated blocks; long Unicode/tab-containing text and code remain legible.
- [ ] Block/heading/first/last/page navigation, selected-target opening, outline focus/toggle, comment lifecycle, copy, approval and changes-requested submissions.
- [ ] Capability and modal gates match command behavior, and inactive reviewer commands fail explicitly rather than targeting a hidden view.

Evidence: `markdown_viewer_test.rs`, browser contracts, `tests/fixtures/migration/review.md`, and the browser Markdown screenshot. The screenshot shows the visible viewport, not every block in the full Markdown fixture.

## Shared UI and shell

`crates/ui/gpui/src/lib.rs` exports modules `components`, `dialog`, `fonts`, `icon`, `indicator_layer`, `inline_edit`, `interface_size`, `keymap`, `menu`, `picker`, `resizable`, `scrollbar`, feature-gated `testing`, `theme`, `tokens`, `tree`, `typography`, `waveform`, `window_chrome`, plus root `InterfaceSize` and `prelude`.

The prelude exports:

| Surface | Symbols |
| --- | --- |
| Controls/surfaces | `ActionBar`, `Backdrop`, `Badge`, `Button`, `ButtonVariant`, `ControlSize`, `ControlState`, `Disclosure`, `EmptyState`, `IconButton`, `InteractionState`, `ListRow`, `Modal`, `ModalSize`, `MutedText`, `NoticeTone`, `Notification`, `SectionHeader`, `Segment`, `SegmentedControl`, `SelectionState`, `Surface`, `SurfaceExt`, `Tooltip` |
| Dialog/input | `ConfirmDialog`, `ConfirmDialogEvent`, `InlineEdit` |
| Fonts/icons/indicators | `CODE_FONT`, `UI_FONT`, `Icon`, `IconName`, `spin_when_shown`, `IndicatorLayer`, `IndicatorMark` |
| Keys/menus | `binding_label`, `key_binding`, `MenuItem`, `with_menu` |
| Pickers | `PickerDetail`, `PickerItem`, `PickerList`, `PickerMarker`, `SearchPicker`, `SearchPickerEvent`, `ThemePicker`, `ThemePickerEvent`, `ThemePickerOverlay` |
| Layout/scrolling | `PanelSide`, `ResizablePanel`, `ListScrollbar`, `uniform_height_hints`, `TreeRow`, `WindowDragExt` |
| Styling | `ActiveTheme`, `UiColors`, `UiStyle`, `UiTheme`, `color`, `window_appearance`, `rem`, `Typography`, `TypographyExt` |
| Other reusable views | `Waveform` |

- [ ] Preserve enabled/disabled, hover/pressed/focus/selection states, size variants, accessibility/focus semantics and callbacks; generic controls stay independent of Diff.
- [ ] Modal/backdrop/menu/picker placement, outside-click/Escape dismissal, keyboard filtering/navigation and focus restoration; nested content cannot bypass overlay command precedence.
- [ ] Resizable-panel min/max and side handling; scrollbar clamping/dragging/height hints; tree disclosure/indentation, indicators and waveform data updates.
- [ ] Register bundled fonts before first layout, preserve syntax/theme modifiers and token-based radius/spacing/typography rather than approximate screens with ad-hoc values.
- [ ] Interface size is rounded/clamped to 10–32, increases/decreases in steps of 1, resets to its initialized default, and changes window rem sizing. Default is 16 on macOS and 18 elsewhere. Secondary shortcuts use Command on macOS and Control on Linux/browser; design tokens use base size 16.
- [ ] Shared shell `launch`, `runtime`, `run_loop`, `persist`, `menus`, optional `script`: runtime ownership, initialization/shutdown, configuration paths, persistence and app/window action dispatch remain usable by non-Diff consumers.
- [ ] Native About/Hide/Hide Others/Show All/Quit, Edit OS actions, Minimize/Zoom, platform menu conventions, and active-window behavior. Do not turn shell actions into Diff-specific commands.

See UI/shell tests in `test-map.json` and [consumer acceptance](consumers.md); the Diff screenshots do not exercise this whole surface.

## Desktop public boundary

| Entry point | Existing input | Existing outcome |
| --- | --- | --- |
| `run` | `CliArgs`, optional `DiffClient` | Runs regular desktop app |
| `run_review` | `CliArgs` | `Option<ReviewSubmission>` |
| `run_client_review` | `DiffClient` | `Option<ReviewSubmission>` |
| `run_markdown_review` | `MarkdownDocument` | `Option<MarkdownReviewSubmission>` |

Public modules: `args`, `preferences`. Reexported menu actions: `About`, `Hide`, `HideOthers`, `Minimize`, `Quit`, `ShowAll`, `Zoom`.

- [ ] One-shot submit returns the structured submission once; cancellation/window close returns `None`. The host owns clipboard, repository I/O, client communication, and application/window lifetime.
- [ ] CLI repository path, scope (`unstaged`, `staged`, `both`), commits mode, `ws://`/`wss://` remote connection, help/errors, conflicting source and missing/invalid arguments retain behavior. Default local scope is both; local and remote review share the real view.
- [ ] Preferences retain their existing file/config behavior, graceful missing/invalid data handling, theme and serialized view mode. Do not silently discard existing user settings during package renaming.
- [ ] Native window opens focused, movable/resizable/minimizable, minimum 640×480, initially maximized around requested 1280×840 bounds. Platform window appearance and close semantics require real-window verification, not just test-window bounds.
- [ ] Menu scope/older/newer-commit actions dispatch through the same domain command availability rules as the in-app controls.

Evidence: desktop `args_test.rs`, `preferences_test.rs`, host/app source and shell menu tests. Actual native OS interaction/performance remains outstanding in the baseline README.

## Browser public boundary

### JavaScript exports

`start`, `set_document_json`, `set_markdown_document_json`, `complete_repository_json`, `set_theme`, `clear_review`, async `connect_remote`, `disconnect_remote`, `dispatch_command_json`, `set_capabilities_json`.

The current Trunk output exposes these as `window.wasmBindings`. `start` initializes one canvas/single-threaded application. Invalid JSON/themes, startup state and unavailable command channels return JS errors rather than silently succeeding. DOM event wrappers log invalid input; they require string details.

### Inbound events on `document`

| Event | Detail |
| --- | --- |
| `diff-review-set-document` | JSON string: bare `DiffDocument` or `{revision?, request_id?, scope?, document}` |
| `markdown-review-set-document` | JSON string: `{source, source_path?, title?}`; parse in Rust |
| `diff-review-set-theme` | Raw built-in theme ID string |
| `diff-review-clear`, `markdown-review-clear` | No payload required; clear active review/comments/draft |
| `diff-review-command` | JSON string: `{request_id?, target:"diff"\|"markdown", command:…}` |
| `diff-review-set-capabilities` | JSON string: domain `ReviewCapabilities` |
| `diff-review-repository-completed` | JSON string: `{request_id, error:string\|null}` |

Command tags follow existing serde, not Rust type names: e.g. `{ "set_view_mode": "Split" }`, `{ "scroll": { "pane": "diff", "lines": 25 } }`, `{ "review": "begin_comment" }`, or `"submit_review"`. Exact complete command variants remain the existing domain API; do not reimplement serialization in the renderer.

### Outbound events on `document`

| Event | Detail |
| --- | --- |
| `diff-review-document-applied` | JSON string: `{revision:number\|null, changed:boolean}` |
| `diff-review-refresh` | JSON string: `{request_id}` |
| `diff-review-set-target` | JSON string: `{target: ReviewTarget}` |
| `diff-review-repository-action` | JSON string: `{request_id, action: RepositoryAction}` |
| `diff-review-command-result` | JSON string: `{request_id:number\|null, handled:boolean, error:string\|null}` |
| `diff-review-submit` | JSON string: domain `ReviewSubmission`, including structured comments and formatted feedback |
| `markdown-review-submit` | JSON string: `MarkdownReviewSubmission`, including decision (`Approved`/`ChangesRequested`) |
| `diff-review-copy`, `markdown-review-copy` | Raw formatted-review string, not JSON |
| `diff-review-cancel`, `markdown-review-cancel` | No detail supplied |
| `diff-review-connection-state` | JSON string: `{state, error:string\|null}` |

There is no additional assumed browser “review changed” event. Preserve the actual events, their `document` target, payload encoding/nullability, ordering and error behavior.

### Ordering, ownership and persistence

- [ ] Every host document push is acknowledged, including stale/identical pushes. Older or equal supplied revisions cannot replace newer content; unversioned pushes are not stale. Identical documents are skipped. A newer identical document still advances the applied revision.
- [ ] Document envelopes can override scope; optional `request_id` can settle the matching command even when content/revision is unchanged. An unrelated push must not clear pending state. Late completions cannot settle the next request. Refresh/actions have independent request IDs and one pending repository operation at a time.
- [ ] Command result correlates the optional request ID and separates `handled:false` from an explicit inactive-reviewer/error result. Hidden reviewers do not receive active-target commands. Overlay/draft/capability/pending restrictions apply equally to host commands and keys.
- [ ] Host-driven diff/Markdown switching uses one mounted active reviewer; clearing review does not replace the domain document.
- [ ] Managed remote connection uses the existing Rust WebSocket client. While managed, host document/Markdown/capability/repository-completion pushes are rejected until disconnect; theme/review/command operations retain their own behavior. Failed connect releases ownership and reports failure. Disconnect closes the client and restores host mode. Preserve connection-state events, snapshots and repository completion correlation.
- [ ] `localStorage["clankerdiff.theme.v1"]` is the raw built-in ID; `localStorage["clankerdiff.view_mode.v1"]` is serialized `ViewMode` (e.g. `"Split"` as JSON text). Missing, invalid or unavailable storage falls back gracefully. Selecting theme/view mode persists through the existing update path.
- [ ] Browser review copy reports formatted text to its embedding host; do not replace this event with an unverified synchronous OS-clipboard assumption.

Evidence: existing `web.spec.ts` tests revision idempotence, pending/stale IDs, capability gates, reviewer switching, commands, Markdown decisions and source-mode restrictions; `remote.spec.ts` starts the real server and tests managed ownership; Rust command/document tests cover wire decoding and update decisions. The new capture records actual comment copy/submit and Markdown approval payloads in addition to screenshots.

## Browser/GPU support boundary

Observed Chromium succeeds with WebGL2 when no usable WebGPU adapter is available. The captured console proves the production app's selected GL backend and disabled subpixel AA on this adapter. WebGPU availability must be probed asynchronously; `navigator.gpu` alone is insufficient. Bundled fonts, canvas sizing/DPR, browser text input/focus, same-origin host event wiring, Rust WebSocket updates and graceful initialization/error reporting are existing expectations.

This record does not establish Firefox/Safari or native hardware-GPU support by inference. It also does not authorize dropping existing supported environments. If Rouge requires WebGPU-only or changes AA/font/input/clipboard behavior, that is an explicit product decision requiring approval. Host-specific graphics adapters still belong to one shared renderer; this does not permit retaining GPUI as a second renderer.
