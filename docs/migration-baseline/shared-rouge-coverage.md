# Shared Rouge test coverage (Phase 5)

`test-map.json` remains the generated inventory of the pre-cutover source, not a live test status file. This table maps its `crates/ui/gpui` groups to the actual public tests under `crates/ui/rouge/tests` unless a sibling Rouge path is specified.

| Baseline group | Replacement and scope |
| --- | --- |
| `components_test.rs` | `components_test.rs`: full-width button, row/segment activation, disabled segment, inline icon detail, section action and disclosure. Nested width/indentation are exercised by `tree_test.rs`; scrim and panel behavior by `dialog_test.rs`. The badge is mounted in a section, but its exact pixel silhouette and top-aligned modal offset are not compared against GPUI. |
| `dialog_test.rs` | `dialog_test.rs`: Enter, Escape, confirm/alternate buttons, outside click, focus wrap/restoration, Escape without a cancel button. |
| `icon_test.rs` | `icon_test.rs`: registered faces, rendered Lucide stroke, compositor animation without host relayout. The old ten-presentation-per-second cap is not preserved; native occlusion handling is implemented in Rouge's event loop, but minimize/restore was not automated on a real window. |
| `indicator_layer_test.rs` | `indicator_layer_test.rs`: partial overlap, ancestor clipping, slot movement/unmount, backdrop suppression, inside-overlay animation and resumption. `waveform_test.rs` covers no-layout animation outside an indicator layer. Native whole-window visibility has the same manual verification gap as icons. |
| `inline_edit_test.rs` | `inline_edit_test.rs`: keyboard submit/cancel and autofocus selection replacing the original text. |
| `interface_size_test.rs` | `interface_size_test.rs`: mounted rem layout, increase shortcut, range, reset and preview transaction. Platform modifier choice is tested by `keymap_test.rs` on the build platform; macOS was not run here. |
| `keymap_test.rs` | `keymap_test.rs`: platform label and scoped action. Rouge uses a view subtree rather than a GPUI context-expression language; invalid chord strings return an error instead of panicking. |
| `menu_test.rs` | `menu_test.rs`: open/closed mounting, selection via pointer and keyboard, a destructive entry after a separator, collision placement and outside dismissal. |
| `picker_test.rs` | `picker_test.rs`: filters including detail text, disabled navigation, grouped rows, shared columns, two-line row height and keyboard reveal, preview by hover and keys, and theme preview/commit/cancel on a supplied catalog. Exact inset/corner concentricity is not pixel-compared. |
| `resizable_test.rs` | `resizable_test.rs`: owner width and mounted rem scaling, both resize edges, captured drag, min/max limits and double-click reset. |
| `scrollbar_test.rs` | `scrollbar_test.rs`: overflow, track jump, drag to end, isolation from another scroller and rem scaling. Rouge's `scroll_view_test.rs` covers shared state with the virtual list; `scroll_test.rs` covers measured/estimated extent and viewport reflow. Content growing during the same drag and automatic tail-follow on release are not verified here. |
| `theme_test.rs` | `theme_test.rs`: catalog roles, alpha preservation, mounted palette and toolkit palette update, light and dark host appearance. |
| `tree_test.rs` | `tree_test.rs`: indentation, full-row hit area, scale and separate selection/disclosure. |
| `waveform_test.rs` | `waveform_test.rs` and `indicator_layer_test.rs`: compositor-only animation, static input levels, teardown and occlusion. The old 30-presentation-per-second cap and exact GPUI bar waveform are not preserved. |
| `src/window_chrome.rs` | `components_test.rs` verifies the shared drag and drag-exclusion path. Rouge's `window_chrome_test.rs` checks edges, corners and command routing. Tiled-edge suppression and platform-specific filtering/order of titlebar controls are not supported by the current Rouge window capability API. |

The native gallery smoke verifies actual presentation, not interactive or pixel-for-pixel parity. Remaining host and visual acceptance is tracked in the migration plan; the sibling app sources were not changed.
