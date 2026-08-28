use crate::style;
use diff_core::DiffTheme;
use gpui::{
    App, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler, FocusHandle,
    Focusable, KeyDownEvent, MouseButton, Pixels, UTF16Selection, Window, canvas, div, prelude::*,
    px,
};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommentEditorEvent {
    Changed(String),
    Submit,
    Cancel,
}

pub(crate) struct CommentEditor {
    focus_handle: FocusHandle,
    body: String,
    cursor: usize,
    marked_range: Option<Range<usize>>,
    theme: DiffTheme,
}

impl CommentEditor {
    pub(crate) fn new(body: String, theme: DiffTheme, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            cursor: body.len(),
            body,
            marked_range: None,
            theme,
        }
    }

    pub(crate) fn body(&self) -> &str {
        &self.body
    }

    pub(crate) fn is_blank(&self) -> bool {
        self.body.trim().is_empty()
    }

    pub(crate) fn set_theme(&mut self, theme: DiffTheme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    fn emit_changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(CommentEditorEvent::Changed(self.body.clone()));
        cx.notify();
    }

    fn replace_range(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        self.body.replace_range(range.clone(), text);
        self.cursor = range.start + text.len();
        self.marked_range = None;
        self.emit_changed(cx);
    }

    fn previous_boundary(&self) -> usize {
        self.body[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(offset, _)| offset)
    }

    fn next_boundary(&self) -> usize {
        self.body[self.cursor..]
            .char_indices()
            .nth(1)
            .map_or(self.body.len(), |(offset, _)| self.cursor + offset)
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "backspace" => {
                let previous = self.previous_boundary();
                self.replace_range(previous..self.cursor, "", cx);
                cx.stop_propagation();
            }
            "delete" => {
                let next = self.next_boundary();
                self.replace_range(self.cursor..next, "", cx);
                cx.stop_propagation();
            }
            "left" => {
                self.cursor = self.previous_boundary();
                cx.stop_propagation();
                cx.notify();
            }
            "right" => {
                self.cursor = self.next_boundary();
                cx.stop_propagation();
                cx.notify();
            }
            "home" => {
                self.cursor = 0;
                cx.stop_propagation();
                cx.notify();
            }
            "end" => {
                self.cursor = self.body.len();
                cx.stop_propagation();
                cx.notify();
            }
            "enter" if event.keystroke.modifiers.platform || event.keystroke.modifiers.control => {
                cx.emit(CommentEditorEvent::Submit);
                cx.stop_propagation();
            }
            "enter" => {
                self.replace_range(self.cursor..self.cursor, "\n", cx);
                cx.stop_propagation();
            }
            "escape" => {
                cx.emit(CommentEditorEvent::Cancel);
                cx.stop_propagation();
            }
            "v" if event.keystroke.modifiers.platform || event.keystroke.modifiers.control => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace_range(self.cursor..self.cursor, &text, cx);
                }
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    fn utf16_to_utf8(&self, offset: usize) -> usize {
        utf16_to_utf8(&self.body, offset)
    }

    fn utf8_to_utf16(&self, offset: usize) -> usize {
        utf8_to_utf16(&self.body, offset)
    }

    fn range_from_utf16(&self, range: Range<usize>) -> Range<usize> {
        self.utf16_to_utf8(range.start)..self.utf16_to_utf8(range.end)
    }

    fn range_to_utf16(&self, range: Range<usize>) -> Range<usize> {
        self.utf8_to_utf16(range.start)..self.utf8_to_utf16(range.end)
    }
}

fn utf16_to_utf8(text: &str, offset: usize) -> usize {
    let mut utf8 = 0;
    let mut utf16 = 0;
    for character in text.chars() {
        if utf16 >= offset {
            break;
        }
        utf8 += character.len_utf8();
        utf16 += character.len_utf16();
    }
    utf8
}

fn utf8_to_utf16(text: &str, offset: usize) -> usize {
    text[..offset].encode_utf16().count()
}

impl Focusable for CommentEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl gpui::EventEmitter<CommentEditorEvent> for CommentEditor {}

impl EntityInputHandler for CommentEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(range_utf16);
        actual_range.replace(self.range_to_utf16(range.clone()));
        Some(self.body[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let cursor = self.utf8_to_utf16(self.cursor);
        Some(UTF16Selection {
            range: cursor..cursor,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .clone()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn paste(&mut self, item: ClipboardItem, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = item.text() {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or(self.cursor..self.cursor);
        self.replace_range(range, text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or(self.cursor..self.cursor);
        let start = range.start;
        self.body.replace_range(range, text);
        self.marked_range = (!text.is_empty()).then_some(start..start + text.len());
        self.cursor = selected_range_utf16.map_or(start + text.len(), |selection| {
            start + utf16_to_utf8(text, selection.end)
        });
        self.emit_changed(cx);
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        _: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.utf8_to_utf16(self.cursor))
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cursor = self.utf16_to_utf8(range_utf16.end);
        cx.notify();
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.body.encode_utf16().count())
    }
}

impl Render for CommentEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme.palette();
        let display = if self.body.is_empty() {
            "Leave a comment…".to_owned()
        } else {
            format!(
                "{}▏{}",
                &self.body[..self.cursor],
                &self.body[self.cursor..]
            )
        };
        let entity = cx.entity();
        let focus_handle = self.focus_handle.clone();

        div()
            .id("comment-input")
            .relative()
            .min_h(px(96.0))
            .w_full()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(style::color(palette.border))
            .bg(style::color(palette.background))
            .text_color(style::color(if self.body.is_empty() {
                palette.muted
            } else {
                palette.foreground
            }))
            .whitespace_normal()
            .cursor_text()
            .track_focus(&self.focus_handle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|editor, _, window, cx| {
                    editor.cursor = editor.body.len();
                    editor.focus_handle.focus(window, cx);
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(Self::on_key_down))
            .child(display)
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), window, cx| {
                        window.handle_input(
                            &focus_handle,
                            ElementInputHandler::new(bounds, entity),
                            cx,
                        );
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_offsets_are_unicode_safe() {
        assert_eq!(utf8_to_utf16("a界b", "a界".len()), 2);
        assert_eq!(utf16_to_utf8("a界b", 2), "a界".len());
    }
}
