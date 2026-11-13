use crate::ReviewHandle;
use clankerdiff_core::{DiffSide, ReviewCommand};
use rouge_core::{Cursor, Styled, Theme, View, component, div, view};
use rouge_widgets::{Icon, IconName};

#[component]
pub(crate) fn add_comment(handle: ReviewHandle, row: usize, side: DiffSide) -> impl View {
    let theme = Theme::current();
    view! {
        <div test_id={format!("add-comment-{row}-{side:?}")} w=19.0 h=19.0 rounded=4.0 bg=theme.accent
             cursor=Cursor::Pointer flex items_center justify_center
             on_click={move |event| {
                 event.stop_propagation();
                 handle.line_command(row, side, ReviewCommand::BeginComment.into());
             }}>
            {Icon::new(IconName::Plus, theme.accent_foreground).size(12.0)}
        </div>
    }
}
