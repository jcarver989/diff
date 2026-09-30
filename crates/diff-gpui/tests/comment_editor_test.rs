use clankerdiff_core::ViewMode;
use clankerdiff_gpui::testing::{DiffViewerHarness, DiffViewerHarnessBuilder};
use gpui::{
    Bounds, ClipboardItem, InputHandler, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, TestAppContext, UTF16Selection, point, px,
};
use std::{error::Error, ops::Range};

#[gpui::test]
fn empty_caret_tracks_focus_and_remains_available_after_input(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = open_editor(cx);
        assert!(harness.comment_focused(cx)?);
        let empty = caret(&harness, cx)?;
        let input = harness.bounds(cx, "comment-input").ok_or("input missing")?;
        assert_eq!(empty.size.width, px(2.0));
        assert!(empty.size.height > px(0.0));
        assert!(empty.origin.x > input.origin.x);
        harness.simulate_input(cx, "a");
        assert!(harness.comment_focused(cx)?);
        harness.with_comment_input(cx, |_, window, cx| window.blur(cx))?;
        assert!(!harness.comment_focused(cx)?);
        click(&harness, cx, empty.origin, Modifiers::default())?;
        assert!(harness.comment_focused(cx)?);
        assert_eq!(selection(&harness, cx)?.range, 0..0);
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn clicking_uses_text_geometry_in_both_diff_layouts(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        for mode in [ViewMode::Unified, ViewMode::Split] {
            let harness = DiffViewerHarnessBuilder::default().build(cx);
            harness.update(cx, |viewer, cx| viewer.set_view_mode(mode, cx));
            harness.simulate_keystrokes(cx, "c");
            harness.simulate_input(cx, "abcdef");
            let position = range_bounds(&harness, cx, 2..2)?.origin;
            let index = harness
                .with_comment_input(cx, |input, window, cx| {
                    input.character_index_for_point(position, window, cx)
                })?
                .ok_or("point has no character")?;
            assert_eq!(index, 2);
            click(&harness, cx, position, Modifiers::default())?;
            assert_eq!(selection(&harness, cx)?.range, 2..2);
            harness.simulate_input(cx, "X");
            assert_eq!(body(&harness, cx)?, "abXcdef");
        }
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn horizontal_selection_replaces_and_deletes_the_selected_text(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = open_editor(cx);
        harness.simulate_input(cx, "abcdef");
        harness.simulate_keystrokes(cx, "left shift-left shift-left");
        let selected = selection(&harness, cx)?;
        assert_eq!(selected.range, 3..5);
        assert!(selected.reversed);
        harness.simulate_input(cx, "X");
        assert_eq!(body(&harness, cx)?, "abcXf");
        harness.simulate_keystrokes(cx, "shift-left backspace");
        assert_eq!(body(&harness, cx)?, "abcf");
        harness.simulate_keystrokes(cx, "ctrl-a delete");
        assert_eq!(body(&harness, cx)?, "");
        assert!(harness.comment_focused(cx)?);
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
#[ignore = "gpui-base 0.7.0 uses scalar cursor boundaries; awaiting upstream grapheme support"]
fn grapheme_navigation_and_deletion_keep_combining_text_and_emoji_intact(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = open_editor(cx);
        harness.simulate_input(cx, "Ae\u{301}👩‍💻Z");
        harness.simulate_keystrokes(cx, "ctrl-home right right");
        assert_eq!(selection(&harness, cx)?.range, 3..3);
        harness.simulate_keystrokes(cx, "left delete");
        assert_eq!(body(&harness, cx)?, "A👩‍💻Z");
        harness.simulate_keystrokes(cx, "right backspace");
        assert_eq!(body(&harness, cx)?, "AZ");
        harness.simulate_keystrokes(cx, "ctrl-end left delete");
        assert_eq!(body(&harness, cx)?, "A");
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn vertical_navigation_preserves_horizontal_position_across_short_lines(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = open_editor(cx);
        set_body(&harness, cx, "abcdefghij\nx\nabcdefghij\n")?;
        harness.simulate_keystrokes(
            cx,
            "ctrl-home right right right right right right right right",
        );
        let first = caret(&harness, cx)?;
        harness.simulate_keystrokes(cx, "down");
        assert_eq!(selection(&harness, cx)?.range, 12..12);
        harness.simulate_keystrokes(cx, "down");
        assert_eq!(selection(&harness, cx)?.range, 21..21);
        assert_eq!(caret(&harness, cx)?.origin.x, first.origin.x);
        harness.simulate_keystrokes(cx, "home");
        assert_eq!(selection(&harness, cx)?.range, 13..13);
        harness.simulate_keystrokes(cx, "end");
        assert_eq!(selection(&harness, cx)?.range, 23..23);
        harness.simulate_keystrokes(cx, "ctrl-end");
        let last = caret(&harness, cx)?;
        assert!(last.origin.y > first.origin.y);
        assert_eq!(last.origin.x, range_bounds(&harness, cx, 0..0)?.origin.x);
        click(&harness, cx, last.origin, Modifiers::default())?;
        assert_eq!(selection(&harness, cx)?.range, 24..24);
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn caret_and_selection_preserve_wrapping_and_vertical_navigation(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = open_editor(cx);
        let text = "wrapping text ".repeat(20);
        set_body(&harness, cx, &text)?;
        let bounds = harness
            .comment_text_bounds(cx)
            .ok_or("text layout missing")?;
        let end = caret(&harness, cx)?;
        harness.simulate_keystrokes(cx, "ctrl-home");
        let first = caret(&harness, cx)?;
        harness.simulate_keystrokes(cx, "down");
        let start = selection(&harness, cx)?.range.start;
        assert!(start > 0);
        let row_start = caret(&harness, cx)?;
        assert!(row_start.origin.y > first.origin.y);
        harness.simulate_keystrokes(cx, "home");
        assert_eq!(selection(&harness, cx)?.range, 0..0);
        harness.simulate_keystrokes(cx, "end");
        assert_eq!(selection(&harness, cx)?.range, text.len()..text.len());
        harness.simulate_keystrokes(cx, "ctrl-a");
        assert_eq!(harness.comment_text_bounds(cx), Some(bounds));
        harness.simulate_keystrokes(cx, "ctrl-end");
        assert_eq!(caret(&harness, cx)?, end);
        assert_eq!(body(&harness, cx)?, text);
        click(&harness, cx, row_start.origin, Modifiers::default())?;
        assert_eq!(selection(&harness, cx)?.range, start..start);
        harness.simulate_input(cx, "X");
        assert_eq!(
            body(&harness, cx)?,
            format!("{}X{}", &text[..start], &text[start..])
        );
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn dragging_and_shift_click_extend_selection(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = open_editor(cx);
        harness.simulate_input(cx, "abcdef");
        let start = range_bounds(&harness, cx, 1..1)?.origin;
        let end = range_bounds(&harness, cx, 4..4)?.origin;
        harness.simulate_event(
            cx,
            MouseDownEvent {
                position: start,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            },
        )?;
        harness.simulate_event(
            cx,
            MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            },
        )?;
        harness.simulate_event(
            cx,
            MouseUpEvent {
                position: end,
                button: MouseButton::Left,
                ..Default::default()
            },
        )?;
        assert_eq!(selection(&harness, cx)?.range, 1..4);
        click(
            &harness,
            cx,
            start,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        )?;
        assert_eq!(selection(&harness, cx)?.range, 1..1);
        click(
            &harness,
            cx,
            end,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        )?;
        harness.simulate_input(cx, "X");
        assert_eq!(body(&harness, cx)?, "aXef");
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn clipboard_and_word_navigation_respect_selection(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = open_editor(cx);
        harness.simulate_input(cx, "one two three");
        harness.simulate_keystrokes(cx, "ctrl-left ctrl-shift-left ctrl-c");
        assert_eq!(selection(&harness, cx)?.range, 4..8);
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("two ")
        );
        harness.simulate_keystrokes(cx, "ctrl-x");
        assert_eq!(body(&harness, cx)?, "one three");
        cx.write_to_clipboard(ClipboardItem::new_string("TWO ".into()));
        harness.simulate_keystrokes(cx, "ctrl-v");
        assert_eq!(body(&harness, cx)?, "one TWO three");
        harness.simulate_keystrokes(cx, "ctrl-a");
        harness.with_comment_input(cx, |input, window, cx| {
            input.paste(ClipboardItem::new_string("replacement".into()), window, cx);
        })?;
        assert_eq!(body(&harness, cx)?, "replacement");
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn platform_selection_and_ime_ranges_use_utf16_and_precise_geometry(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = open_editor(cx);
        set_body(&harness, cx, "a😀b")?;
        harness.with_comment_state(cx, |input, _, cx| input.set_selected_range(1..5, cx))?;
        harness.with_comment_input(cx, |input, window, cx| {
            input.replace_and_mark_text_in_range(None, "界😀", Some(1..3), window, cx);
        })?;
        assert_eq!(body(&harness, cx)?, "a界😀b");
        assert_eq!(selection(&harness, cx)?.range, 2..4);
        let marked = harness.with_comment_input(cx, InputHandler::marked_text_range)?;
        assert_eq!(marked, Some(1..4));
        let at_two = range_bounds(&harness, cx, 2..2)?;
        let at_four = range_bounds(&harness, cx, 4..4)?;
        assert!(at_four.origin.x > at_two.origin.x);
        assert!(at_two.size.height > px(0.0));
        assert_eq!(
            harness.with_comment_input(cx, |input, window, cx| {
                input.character_index_for_point(
                    at_four.origin + point(px(1.0), px(1.0)),
                    window,
                    cx,
                )
            })?,
            Some(4)
        );
        harness.with_comment_input(cx, |input, window, cx| {
            input.replace_text_in_range(None, "語", window, cx);
        })?;
        assert_eq!(body(&harness, cx)?, "a語b");
        assert_eq!(selection(&harness, cx)?.range, 2..2);
        assert_eq!(
            harness.with_comment_input(cx, InputHandler::marked_text_range)?,
            None
        );
        harness.with_comment_state(cx, |input, _, cx| {
            input.set_selected_range(0..usize::MAX, cx);
        })?;
        assert_eq!(selection(&harness, cx)?.range, 0..3);
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

#[gpui::test]
fn library_undo_and_redo_restore_the_draft(cx: &mut TestAppContext) {
    let result = (|| -> Result<(), Box<dyn Error>> {
        let harness = open_editor(cx);
        set_body(&harness, cx, "original")?;
        set_body(&harness, cx, "replacement")?;
        assert_eq!(body(&harness, cx)?, "replacement");
        harness.simulate_keystrokes(cx, "ctrl-z");
        assert_eq!(body(&harness, cx)?, "original");
        harness.simulate_keystrokes(cx, "ctrl-y");
        assert_eq!(body(&harness, cx)?, "replacement");
        Ok(())
    })();
    if let Err(error) = result {
        panic!("{error}");
    }
}

fn open_editor(cx: &mut TestAppContext) -> DiffViewerHarness {
    let harness = DiffViewerHarnessBuilder::default().build(cx);
    harness.simulate_keystrokes(cx, "c");
    harness
}

fn body(harness: &DiffViewerHarness, cx: &TestAppContext) -> Result<String, Box<dyn Error>> {
    harness
        .read(cx, |viewer, _| {
            viewer
                .session()
                .draft()
                .map(|draft| draft.body().to_owned())
        })
        .ok_or_else(|| "draft missing".into())
}

fn set_body(
    harness: &DiffViewerHarness,
    cx: &mut TestAppContext,
    text: &str,
) -> Result<(), Box<dyn Error>> {
    let end = body(harness, cx)?.encode_utf16().count();
    harness.with_comment_input(cx, |input, window, cx| {
        input.replace_text_in_range(Some(0..end), text, window, cx);
    })
}

fn selection(
    harness: &DiffViewerHarness,
    cx: &mut TestAppContext,
) -> Result<UTF16Selection, Box<dyn Error>> {
    harness
        .with_comment_input(cx, |input, window, cx| {
            input.selected_text_range(false, window, cx)
        })?
        .ok_or_else(|| "selection missing".into())
}

fn range_bounds(
    harness: &DiffViewerHarness,
    cx: &mut TestAppContext,
    range: Range<usize>,
) -> Result<Bounds<Pixels>, Box<dyn Error>> {
    harness
        .with_comment_input(cx, |input, window, cx| {
            input.bounds_for_range(range, window, cx)
        })?
        .ok_or_else(|| "range bounds missing".into())
}

fn caret(
    harness: &DiffViewerHarness,
    cx: &mut TestAppContext,
) -> Result<Bounds<Pixels>, Box<dyn Error>> {
    harness
        .with_comment_state(cx, |input, _, _| {
            input.cursor_layout().map(|(bounds, _)| bounds)
        })?
        .ok_or_else(|| "caret layout missing".into())
}

fn click(
    harness: &DiffViewerHarness,
    cx: &mut TestAppContext,
    position: Point<Pixels>,
    modifiers: Modifiers,
) -> Result<(), Box<dyn Error>> {
    let position = point(position.x, position.y + px(1.0));
    harness.simulate_event(
        cx,
        MouseDownEvent {
            position,
            modifiers,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        },
    )?;
    harness.simulate_event(
        cx,
        MouseUpEvent {
            position,
            modifiers,
            button: MouseButton::Left,
            ..Default::default()
        },
    )
}
