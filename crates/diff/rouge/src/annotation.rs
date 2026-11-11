use crate::ReviewHandle;
use clankerdiff_core::{DiffSide, ReviewCommand};
use clankerdiff_rouge_ui::{
    icon::{Icon, IconName},
    theme::UiTheme,
    tokens::{rem, scaled},
};
use rouge_core::{Cursor, Styled, View, div};

pub(crate) fn add_comment(handle: ReviewHandle, row: usize, side: DiffSide, theme: UiTheme) -> impl View {
    div()
        .test_id(format!("add-comment-{row}-{side:?}"))
        .w(rem(19.0))
        .h(rem(19.0))
        .rounded(scaled(4.0))
        .bg(theme.paint(|c| c.accent))
        .cursor(Cursor::Pointer)
        .flex()
        .items_center()
        .justify_center()
        .child(Icon::new(IconName::Plus, theme.paint(|c| c.accent_foreground)).size(12.0))
        .on_click(move |event| {
            event.stop_propagation();
            handle.line_command(row, side, ReviewCommand::BeginComment.into());
        })
}
