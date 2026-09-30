# Diff GPUI patch

This crate is vendored from the crates.io `gpui-pre` 0.3.7 package, based on
`zed-industries/zed` commit `1a28cff4b409169bac058bca40dfbfeb7621d19b`, and
selected through the root Cargo `[patch.crates-io]` table. The matching platform
and web snapshots are also pinned to 0.3.7; the shared UI uses `gpui-base` 0.7.0
for ordinary text editing.

Only `src/elements/list.rs` changes runtime behavior. SVG test font paths use the
bundled `test-assets`, and example/test targets are omitted from the vendored
manifest. All other source matches the published snapshot.

The two scrolling fixes below remain absent from this snapshot. When upgrading,
compare against the published package and run both regression tests before
removing or carrying them forward.

Diff's rows have variable, width-dependent heights. The upstream `ListState`
discards all uniform item-height hints whenever the list width changes, making
every unmeasured row temporarily contribute zero pixels to the content and
prefix-height summaries. Scroll input is then clamped against that transient
height and visibly snaps as rows are measured.

The local change stores the configured uniform fallback height and reapplies it
during width invalidation. It still discards stale measured heights and lazily
remeasures visible rows, preserving virtualization while keeping scroll
geometry stable.

Scrollbar drags also freeze the estimated content height while rows are measured.
When the drag ends, the patch reapplies the released thumb fraction to the live
content height instead of retaining an offset from the stale estimate. This
prevents the thumb from jumping toward the top as soon as the frozen height is
released.

The regression tests
`test_uniform_height_hint_survives_width_invalidation` and
`test_scrollbar_drag_release_preserves_fraction_after_height_growth` cover these
behaviors.

Run the regressions independently of the workspace with:

```sh
cargo test --manifest-path patches/gpui/Cargo.toml \
  --config "patch.crates-io.gpui-pre.path=\"$PWD/patches/gpui\"" \
  --features test-support --lib test_uniform_height_hint_survives_width_invalidation
cargo test --manifest-path patches/gpui/Cargo.toml \
  --config "patch.crates-io.gpui-pre.path=\"$PWD/patches/gpui\"" \
  --features test-support --lib test_scrollbar_drag_release_preserves_fraction_after_height_growth
```

## Textarea compatibility

`gpui-base` owns cursor movement, selection, clipboard, undo/redo, and IME
editing. Diff observes changes to the input value to synchronize drafts during
IME composition, which does not emit the library's ordinary Change event.

The 0.7.0 textarea uses logical-line Home/End and wrap-aware vertical movement.
Its cursor boundaries are Unicode scalars rather than grapheme clusters, so
combining sequences and ZWJ emoji can be split by navigation or deletion. The
integration regression for grapheme editing is retained as explicitly ignored
pending an upstream fix; Diff does not implement a parallel cursor engine.
