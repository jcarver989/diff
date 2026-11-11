# Direct consumer acceptance checklist

The revisions and explicit source/import evidence are in [consumers.json](consumers.json). Both repositories were available and clean in tracked files when inspected. This is a source inventory, not a claim that either app was built, tested, or migrated. Read-only inspection found no actual `GpuiApp` preset use in either consumer; do not invent a preset migration requirement from their UI imports.

## j-terminal

| Actual use | Source locations relative to j-terminal | Required continuation |
| --- | --- | --- |
| Shell application launch and async runtime ownership | `crates/j-terminal/src/lib.rs` | One native launch/runtime lifecycle, registered fonts/theme and correct shutdown |
| Configuration location/persistence | `src/state.rs`, `src/settings.rs` under `crates/j-terminal` | Existing config directory and read/write behavior, not a Diff-specific preference service |
| Native menu installation and app/window actions | `crates/j-terminal/src/menus.rs` | About/hide/quit/window actions, Edit menu OS actions, command labels and platform shortcuts |
| Optional scripted UI execution | `crates/j-terminal/src/lib.rs`, shell `script::run` imports | Script input, capture and completion behavior through the real app, not an alternate renderer |
| Confirmation, searchable picker, menus and interface scaling | `crates/j-terminal/src/{remote,workspace,tab,sidebar,settings}.rs` | Dialog dismissal/confirmation, filtering/selection, focus restoration and rem-based resizing |
| Theme/prelude, typography, font and token helpers, window chrome | `crates/j-terminal/src/{lib,sidebar,tab}.rs`; `crates/jt-term-view/src/{style,element,view}.rs` | Reusable style semantics and terminal-view embedding, including components not visible in Diff |
| Public UI test harness | `crates/j-terminal/tests/workspace_test.rs`, `crates/jt-term-view/tests/view_test.rs` | Public integration-test equivalents rather than retained GPUI harnesses |

- [ ] Update package/import names directly during cutover; no GPUI-shaped compatibility facade.
- [ ] Port each concrete usage above, including terminal rendering/selection styling rather than merely compiling the Diff app.
- [ ] Run consumer builds/tests against the rewritten shared UI and shell; record the exact consumer revision used.

## aether

| Actual use | Source locations relative to aether | Required continuation |
| --- | --- | --- |
| Native launch/runtime/menu integration | `crates/desktop/src/{lib,menus,app}.rs` | Shared shell lifecycle, native actions/shortcuts and app menus |
| Artifact diff review | `crates/desktop/src/artifact_review.rs`, `crates/desktop/src/threads/driver.rs` | `DiffViewer`, `DiffViewerOptions`, aliased viewer event handling, client view updates and review outcomes |
| Markdown artifact review | `crates/desktop/src/artifact_review.rs` | `MarkdownReviewer`/options and structured submit/cancel integration |
| Embedded patch previews and rich transcript code/tool content | `crates/desktop/src/chat/transcript/{tools,mod,code,message}.rs` | `PatchPreview`, syntax/style helpers and bounded embedding inside transcript layout |
| General UI/prelude/tokens/fonts/themes and file UI | `crates/desktop/src/ui/{mod,tokens}.rs`, `crates/desktop/src/files.rs` | Shared controls and style behavior, not copied app-local widget implementations |
| Window-driving tests | `crates/desktop/src/testing.rs` | Public `WindowDriver`-equivalent integration testing without GPUI dependencies |

- [ ] Migrate artifact event handling and command/capability behavior as well as view construction.
- [ ] Preserve embedding behavior and patch-preview sizing inside a non-Diff host.
- [ ] Port test-driver usage and execute consumer tests at a recorded revision.

## Shared surface not established by import scanning

Glob imports and the Diff crate's `ui` reexport do not form a resolved use graph. The human checklist above supplements, not replaces, the explicit evidence. All public components remain in scope even when not imported by these applications: see [api.json](api.json) and the UI section of [contract.md](contract.md), including waveform, indicators, tree rows, inline editing, resizable panels, scrollbars, typography and generic overlays.

The `util < ui < diff` boundary remains a constraint. A consumer of generic UI/shell components must not acquire Diff domain dependencies through the replacement. Consumers are future cutover work, not a reason to retain two renderer packages.

## Concrete Rouge API changes to coordinate

- Replace `clankerdiff-gpui-ui` / `clankerdiff-gpui-shell` with `clankerdiff-rouge-ui` / `clankerdiff-rouge-shell`. Native consumers construct `rouge::App`, attach menus and windows, then call `clankerdiff_rouge_shell::launch(app)`. Install the `runtime::build()` Tokio handle in the root scope via `runtime::install`; use `runtime::spawn` for Tokio I/O that returns to the UI. `persist::{config_dir, read, write, Store}` does not depend on Diff; retain the consumer's config directory and serialized schema.
- Replace GPUI action registration and reflection with typed Rouge `MenuItem` actions, root `on_action` and `bind_key`. For j-terminal's script entry point, call `script::run(script, actions)` from the window's root scope with the application's explicit name-to-menu-item registry (`menus::commands` for menu actions); add non-menu actions to that registry as needed. Port terminal input, selection, font/style, split panels, and window controls to Rouge views/signals. Use `rouge_core::testing::TestUiBuilder` for mounted behavior and a real native app for OS integration tests.
- For Aether artifact reviews, replace `DiffViewer` construction with `ReviewController::with_options(document, theme, options).mount(output)` and `diff_view(handle)`, or use `DiffClientView::mount`, `set_state` and `view` for a client-driven mount. Send `ReviewOutput` to the host driver and call `command_finished` or `lookup_finished` separately from snapshot installation. Replace `MarkdownReviewer` with `MarkdownController::new(document, theme).mount(output)` and `markdown_review_view(handle)`. Host `MarkdownOutput` clipboard, URL, submit and cancel handling explicitly. `PatchPreview::parse` and `PatchPreview::render` now produce a Rouge view; feed the full document passed to its open callback into the same review route.
- Adopt the generated sibling checkout for each consumer only once a tested Rouge revision containing the current toolkit changes is fetchable. The local public render fixture verifies the new dependency boundary, not either consumer app. Neither consumer repository has been modified or verified against these APIs.
