#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DiffReviewCommand, DiffSide, FocusPane, ReviewCommand, SourceLocation, ViewMode, testing::DocumentBuilder,
};
use clankerdiff_gpui::{
    ViewModeChanged,
    testing::{DiffViewerHarness, DiffViewerHarnessBuilder},
};
use gpui::{ScrollDelta, TestAppContext, point, px};
use std::{cell::RefCell, rc::Rc};

#[gpui::test]
fn a_wheel_notch_scrolls_six_code_rows_at_any_interface_size(cx: &mut TestAppContext) {
    for interface_size in [16.0, 20.0] {
        DiffViewerHarnessBuilder { interface_size, ..multi_file_builder() }.run(cx, |harness, cx| {
            let top = harness.diff_scroll_top(cx);
            harness.scroll(cx, "diff-pane", ScrollDelta::Lines(point(0.0, -3.0)))?;
            let scrolled = harness.diff_scroll_top(cx);
            assert_eq!(scrolled.item_ix, top.item_ix + 1 + 4);
            assert_eq!(scrolled.offset_in_item, top.offset_in_item);
            Ok(())
        });
    }
}

#[gpui::test]
fn wheel_notches_past_the_end_do_not_delay_scrolling_back_up(cx: &mut TestAppContext) {
    let scroll_up_from_bottom = |overscroll: f32, cx: &mut TestAppContext| {
        let harness = multi_file_builder().build(cx);
        harness.scroll_to_bottom_of_diff(cx);
        let bottom = harness.diff_scroll_top(cx);
        harness.scroll(cx, "diff-pane", ScrollDelta::Lines(point(0.0, -overscroll))).expect("the diff pane is painted");
        let overscrolled = harness.diff_scroll_top(cx);
        assert_eq!((overscrolled.item_ix, overscrolled.offset_in_item), (bottom.item_ix, bottom.offset_in_item));
        harness.scroll(cx, "diff-pane", ScrollDelta::Lines(point(0.0, 3.0))).expect("the diff pane is painted");
        harness.diff_scroll_top(cx)
    };
    let direct = scroll_up_from_bottom(0.0, cx);
    let after_overscroll = scroll_up_from_bottom(9.0, cx);
    assert_eq!((after_overscroll.item_ix, after_overscroll.offset_in_item), (direct.item_ix, direct.offset_in_item));
}

#[gpui::test]
fn wheel_scrolling_crosses_files_without_changing_selection_or_draft(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        let selected = harness.selected_row(cx);
        assert!(harness.dispatch_command(cx, ReviewCommand::BeginComment)?);
        harness.simulate_input(cx, "unfinished comment");
        let body = harness.draft_body(cx);
        for _ in 0..5 {
            harness.scroll(cx, "diff-pane", ScrollDelta::Pixels(point(px(0.0), px(-600.0))))?;
        }
        assert_eq!(harness.read(cx, |viewer, _| viewer.active_file()), Some(1));
        assert!(harness.bounds(cx, "diff-active-file:1").is_some());
        assert_eq!(harness.selected_row(cx), selected);
        assert_eq!(harness.read(cx, |viewer, _| viewer.selected_file()), Some(0));
        assert_eq!(harness.draft_body(cx), body);
        assert!(harness.comment_focused(cx)?);
        harness.simulate_input(cx, " still editing");
        assert!(harness.draft_body(cx).ok_or("draft")?.contains("still editing"));
        for _ in 0..8 {
            harness.scroll(cx, "diff-pane", ScrollDelta::Pixels(point(px(0.0), px(600.0))))?;
        }
        assert_eq!(harness.read(cx, |viewer, _| viewer.active_file()), Some(0));
        assert_eq!(harness.selected_row(cx), selected);
        Ok(())
    });
}

#[gpui::test]
fn selecting_and_commenting_on_another_visible_file_uses_its_anchor(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        let row = harness
            .read(cx, |viewer, _| {
                let range = viewer.presentation().file_range(1)?;
                viewer.presentation().first_navigable(range)
            })
            .ok_or("second file code row")?;
        assert!(harness.dispatch_command(cx, DiffReviewCommand::SelectRow(row))?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::SelectSide(DiffSide::New))?);
        assert!(harness.dispatch_command(cx, ReviewCommand::BeginComment)?);
        harness.simulate_input(cx, "second file comment");
        assert!(harness.dispatch_command(cx, ReviewCommand::SubmitComment)?);
        let path = harness
            .read(cx, |viewer, _| viewer.review().comments().next().map(|comment| comment.anchor.path.to_string()));
        assert_eq!(path.as_deref(), Some("b.rs"));
        Ok(())
    });
}

#[gpui::test]
fn refreshes_and_layout_changes_preserve_the_top_visible_source_line(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        assert!(harness.dispatch_command(cx, DiffReviewCommand::SelectFile(1))?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Scroll { pane: FocusPane::Diff, lines: 25 })?);
        let top = top_source(harness, cx).ok_or("top source")?;
        assert!(harness.dispatch_command(cx, DiffReviewCommand::SetViewMode(ViewMode::Unified))?);
        assert_eq!(top_source(harness, cx).as_ref(), Some(&top));
        for builder in [
            DocumentBuilder::new().generated("a.rs", 150).generated("b.rs", 100),
            DocumentBuilder::new().generated("b.rs", 100),
        ] {
            harness.update(cx, |viewer, cx| viewer.set_document(builder.build(), cx));
            harness.draw(cx);
            assert_eq!(top_source(harness, cx).as_ref(), Some(&top));
        }
        Ok(())
    });
}

#[gpui::test]
fn source_round_trip_restores_the_independent_multi_file_viewport(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        for _ in 0..5 {
            harness.scroll(cx, "diff-pane", ScrollDelta::Pixels(point(px(0.0), px(-600.0))))?;
        }
        assert_eq!(harness.read(cx, |viewer, _| viewer.active_file()), Some(1));
        let selected = harness.selected_row(cx);
        let offset = harness.diff_scroll_top(cx);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::ToggleSourceView)?);
        assert_eq!(harness.read(cx, |viewer, _| viewer.active_file()), Some(0));
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Last)?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::ToggleSourceView)?);
        assert_eq!(harness.diff_scroll_top(cx).item_ix, offset.item_ix);
        assert_eq!(harness.diff_scroll_top(cx).offset_in_item, offset.offset_in_item);
        assert_eq!(harness.selected_row(cx), selected);
        Ok(())
    });
}

#[gpui::test]
fn whole_diff_scrollbar_and_highlighting_remain_virtualized(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder {
        document: DocumentBuilder::new().generated_files(200, 100).build(),
        ..Default::default()
    }
    .with_view_mode(ViewMode::Split)
    .run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "diff-scrollbar").is_some());
        let before = harness.read(cx, |viewer, _| viewer.highlight_stats());
        assert!(before.misses < 10);
        harness.scroll_to_bottom_of_diff(cx);
        assert_eq!(harness.read(cx, |viewer, _| viewer.active_file()), Some(99));
        let after = harness.read(cx, |viewer, _| viewer.highlight_stats());
        assert!(after.misses - before.misses < 10);
        assert!(after.calls - before.calls < 1000);
        Ok(())
    });
}

#[gpui::test]
fn removing_the_visible_file_falls_back_safely_and_empty_diffs_remain_usable(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        assert!(harness.dispatch_command(cx, DiffReviewCommand::SelectFile(1))?);
        harness.update(cx, |viewer, cx| {
            viewer.set_document(DocumentBuilder::new().binary("c.bin").build(), cx);
        });
        harness.draw(cx);
        assert_eq!(harness.read(cx, |viewer, _| viewer.active_file()), Some(0));
        assert!(harness.bounds(cx, "diff-file-header:0").is_some());
        for command in [DiffReviewCommand::First, DiffReviewCommand::Last, DiffReviewCommand::MoveSelection(1)] {
            assert!(harness.dispatch_command(cx, command)?);
        }
        harness.update(cx, |viewer, cx| {
            viewer.set_document(DocumentBuilder::new().build(), cx);
        });
        harness.draw(cx);
        assert_eq!(harness.read(cx, |viewer, _| viewer.active_file()), None);
        assert_eq!(harness.selected_row(cx), None);
        Ok(())
    });
}

#[gpui::test]
fn the_sidebar_follows_the_top_file_without_expanding_collapsed_directories(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder {
        document: DocumentBuilder::new()
            .generated("src/a.rs", 80)
            .generated("src/b.rs", 80)
            .generated("z.rs", 80)
            .build(),
        ..Default::default()
    }
    .with_view_mode(ViewMode::Split)
    .run(cx, |harness, cx| {
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Focus(FocusPane::Files))?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::First)?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::CollapseSelected)?);
        for _ in 0..4 {
            harness.scroll(cx, "diff-pane", ScrollDelta::Pixels(point(px(0.0), px(-600.0))))?;
        }
        assert_eq!(harness.read(cx, |viewer, _| viewer.active_file()), Some(1));
        assert!(harness.dispatch_command(cx, DiffReviewCommand::MoveSelection(1))?);
        assert_eq!(harness.read(cx, |viewer, _| viewer.selected_file()), Some(2));
        Ok(())
    });
}

#[gpui::test]
fn the_next_file_divider_pushes_the_sticky_divider_out_of_view(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        assert!(harness.dispatch_command(cx, DiffReviewCommand::SelectFile(1))?);
        let pane_top = harness.bounds(cx, "diff-file-header:1").ok_or("header")?.top();
        assert!(harness.bounds(cx, "diff-active-file:1").is_some());
        harness.scroll(cx, "diff-pane", ScrollDelta::Pixels(point(px(0.0), px(20.0))))?;
        assert_eq!(harness.read(cx, |viewer, _| viewer.active_file()), Some(0));
        let sticky = harness.bounds(cx, "diff-active-file:0").ok_or("sticky")?;
        let next = harness.bounds(cx, "diff-file-header:1").ok_or("next header")?;
        assert_eq!(next.top(), pane_top + px(20.0));
        assert_eq!(sticky.bottom(), next.top());
        assert!(sticky.top() < pane_top);
        Ok(())
    });
}

#[gpui::test]
fn revealing_a_row_keeps_it_below_the_sticky_divider(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        assert!(harness.dispatch_command(cx, DiffReviewCommand::SelectFile(1))?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::MoveSelection(3))?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Scroll { pane: FocusPane::Diff, lines: 30 })?);
        assert!(harness.dispatch_command(cx, DiffReviewCommand::MoveSelection(-1))?);
        let selected = harness
            .read(cx, |viewer, _| {
                let first = viewer.presentation().file_range(0)?;
                Some(first.len() + harness.selected_row(cx)? - viewer.presentation().file_range(1)?.start)
            })
            .ok_or("selected item")?;
        let top = harness.diff_scroll_top(cx);
        assert!(top.item_ix < selected);
        assert_eq!(harness.read(cx, |viewer, _| viewer.active_file()), Some(1));
        Ok(())
    });
}

#[gpui::test]
fn clicking_a_file_divider_selects_that_file(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder {
        document: DocumentBuilder::new()
            .changed("src/a.rs", "old", "new")
            .renamed("old/b.rs", "src/b.rs", "before", "after")
            .build(),
        ..Default::default()
    }
    .run(cx, |harness, cx| {
        harness.click(cx, "diff-file-header:1")?;
        assert_eq!(harness.read(cx, |viewer, _| viewer.selected_file()), Some(1));
        Ok(())
    });
}

#[gpui::test]
#[allow(clippy::format_collect)]
fn gaps_expand_from_their_gutter_button_and_only_select_from_the_rule(cx: &mut TestAppContext) {
    let old = (1..=60).map(|line| format!("line {line}\n")).collect::<String>();
    let new = old.replace("line 10\n", "changed 10\n").replace("line 50\n", "changed 50\n");
    DiffViewerHarnessBuilder {
        document: DocumentBuilder::new().changed("a.rs", &old, &new).build(),
        ..Default::default()
    }
    .with_view_mode(ViewMode::Unified)
    .with_window_size(1200.0, 900.0)
    .run(cx, |harness, cx| {
        let hidden = |harness: &DiffViewerHarness, cx: &TestAppContext| {
            harness.read(cx, |viewer, _| {
                (0..viewer.presentation().row_count())
                    .filter_map(|row| viewer.presentation().gap_info(row))
                    .map(|info| info.hidden_lines)
                    .sum::<usize>()
            })
        };
        let gap = harness
            .read(cx, |viewer, _| {
                (0..viewer.presentation().row_count())
                    .find(|&row| viewer.presentation().gap_info(row).is_some_and(|info| info.id.gap_index == 1))
            })
            .ok_or("gap between hunks")?;
        let before = hidden(harness, cx);
        let rule: &'static str = Box::leak(format!("diff-gap:{gap}").into_boxed_str());
        let button: &'static str = Box::leak(format!("diff-gap-expand-{gap}").into_boxed_str());
        assert!(harness.bounds(cx, button).is_some());
        harness.click(cx, rule)?;
        assert_eq!(harness.selected_row(cx), Some(gap));
        assert_eq!(hidden(harness, cx), before);
        harness.click(cx, button)?;
        assert!(hidden(harness, cx) < before);
        Ok(())
    });
}

fn top_source(harness: &DiffViewerHarness, cx: &TestAppContext) -> Option<SourceLocation> {
    harness.read(cx, |viewer, _| {
        let row = viewer.presentation().row(viewer.diff_scroll_top().item_ix)?;
        viewer.presentation().source_location(row, row.primary_cell()?)
    })
}

#[gpui::test]
fn file_header_chevrons_collapse_and_expand_a_file(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        let expanded = file_rows(harness, cx, 0);
        assert!(harness.bounds(cx, "diff-file-header:1").is_none());
        harness.click(cx, "diff-file-collapse:0")?;
        assert!(harness.read(cx, |viewer, _| viewer.session().is_file_collapsed(0)));
        assert_eq!(file_rows(harness, cx, 0), 1);
        assert!(harness.bounds(cx, "diff-file-header:1").is_some());
        assert_eq!(harness.read(cx, |viewer, _| viewer.selected_file()), Some(0));
        harness.click(cx, "diff-file-collapse:0")?;
        assert!(!harness.read(cx, |viewer, _| viewer.session().is_file_collapsed(0)));
        assert_eq!(file_rows(harness, cx, 0), expanded);
        Ok(())
    });
}

#[gpui::test]
fn selecting_a_collapsed_file_scrolls_to_its_header_without_expanding(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        harness.click(cx, "diff-file-collapse:0")?;
        harness.click(cx, "diff-file-collapse:1")?;
        assert!(harness.dispatch_command(cx, DiffReviewCommand::SelectFile(1))?);
        assert!(harness.read(cx, |viewer, _| viewer.session().is_file_collapsed(1)));
        let header = harness.read(cx, |viewer, _| viewer.presentation().file_range(1).map(|range| range.start));
        assert_eq!(harness.selected_row(cx), header);
        assert!(harness.bounds(cx, "diff-file-header:1").is_some());
        Ok(())
    });
}

#[gpui::test]
fn toolbar_icon_collapses_and_expands_every_file(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        harness.click(cx, "toggle-all-files-collapsed")?;
        assert!(harness.read(cx, |viewer, _| viewer.session().all_files_collapsed()));
        assert!(harness.bounds(cx, "diff-file-header:1").is_some());
        harness.click(cx, "toggle-all-files-collapsed")?;
        assert!(harness.read(cx, |viewer, _| {
            !viewer.session().is_file_collapsed(0) && !viewer.session().is_file_collapsed(1)
        }));
        Ok(())
    });
}

#[gpui::test]
fn toolbar_icons_pick_the_layout_mode_and_report_each_choice(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder::default().with_window_size(1200.0, 600.0).run(cx, |harness, cx| {
        let picked = Rc::new(RefCell::new(Vec::new()));
        let _subscription = cx.update(|cx| {
            let picked = picked.clone();
            cx.subscribe(&harness.viewer(), move |_, event: &ViewModeChanged, _| {
                picked.borrow_mut().push(event.mode);
            })
        });

        harness.click(cx, "view-mode-split")?;
        assert_eq!(harness.read(cx, |viewer, _| viewer.view_mode()), ViewMode::Split);
        assert!(harness.read(cx, |viewer, _| viewer.session().layout().is_split()));

        harness.click(cx, "view-mode-unified")?;
        assert_eq!(harness.read(cx, |viewer, _| viewer.view_mode()), ViewMode::Unified);
        assert!(!harness.read(cx, |viewer, _| viewer.session().layout().is_split()));

        harness.click(cx, "view-mode-auto")?;
        harness.click(cx, "view-mode-auto")?;
        assert_eq!(harness.read(cx, |viewer, _| viewer.view_mode()), ViewMode::Auto);

        assert_eq!(*picked.borrow(), [ViewMode::Split, ViewMode::Unified, ViewMode::Auto]);
        Ok(())
    });
}

#[gpui::test]
fn layout_toggle_is_hidden_in_source_view(cx: &mut TestAppContext) {
    DiffViewerHarnessBuilder::default().run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "view-mode-toggle").is_some());
        assert!(harness.dispatch_command(cx, DiffReviewCommand::ToggleSourceView)?);
        assert!(harness.bounds(cx, "view-mode-toggle").is_none());
        Ok(())
    });
}

#[gpui::test]
fn z_collapses_the_selected_file_and_the_toolbar_collapses_all_files(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Focus(FocusPane::Diff))?);
        harness.simulate_keystrokes(cx, "z");
        assert!(harness.read(cx, |viewer, _| viewer.session().is_file_collapsed(0)));
        assert!(!harness.read(cx, |viewer, _| viewer.session().is_file_collapsed(1)));
        harness.simulate_keystrokes(cx, "enter");
        assert!(!harness.read(cx, |viewer, _| viewer.session().is_file_collapsed(0)));
        harness.click(cx, "toggle-all-files-collapsed")?;
        assert!(harness.read(cx, |viewer, _| viewer.session().all_files_collapsed()));
        harness.click(cx, "toggle-all-files-collapsed")?;
        assert!(!harness.read(cx, |viewer, _| viewer.session().all_files_collapsed()));
        Ok(())
    });
}

#[gpui::test]
fn collapsed_headers_show_their_comment_count(cx: &mut TestAppContext) {
    multi_file_builder().run(cx, |harness, cx| {
        assert!(harness.dispatch_command(cx, DiffReviewCommand::Focus(FocusPane::Diff))?);
        harness.add_comment(cx, "remember this")?;
        assert!(harness.bounds(cx, "diff-file-comment-count:0").is_none());
        harness.click(cx, "diff-file-collapse:0")?;
        assert!(harness.bounds(cx, "diff-file-comment-count:0").is_some());
        assert!(harness.bounds(cx, "diff-file-comment-count:1").is_none());
        Ok(())
    });
}

fn multi_file_builder() -> DiffViewerHarnessBuilder {
    DiffViewerHarnessBuilder {
        document: DocumentBuilder::new().generated("a.rs", 100).generated("b.rs", 100).build(),
        ..Default::default()
    }
    .with_view_mode(ViewMode::Split)
    .with_window_size(1200.0, 600.0)
}

fn file_rows(harness: &DiffViewerHarness, cx: &TestAppContext, file: usize) -> usize {
    harness.read(cx, |viewer, _| viewer.presentation().file_range(file).map_or(0, |range| range.len()))
}
