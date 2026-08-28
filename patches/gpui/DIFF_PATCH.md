# Diff GPUI patch

This crate is vendored from `zed-industries/zed` commit
`797e5dc95c3859f7926681c91398c4d9e993865d` and selected through the root
Cargo `[patch]` table.

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
