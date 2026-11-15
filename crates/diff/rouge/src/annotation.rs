use crate::ReviewHandle;
use clankerdiff_core::{DiffSide, ReviewCommand};
use rouge_core::{Cursor, Role, Styled, View, component, div, view};
use rouge_widgets::{Icon, IconName};

#[component]
pub(crate) fn add_comment(handle: ReviewHandle, row: usize, side: DiffSide) -> impl View {
    view! {
        <div test_id={format!("add-comment-{row}-{side:?}")} w=19.0 h=19.0 rounded=4.0 bg=Role::Accent
             cursor=Cursor::Pointer flex items_center justify_center
             on_click={move |event| {
                 event.stop_propagation();
                 handle.line_command(row, side, ReviewCommand::BeginComment.into());
             }}>
            {Icon::new(IconName::Plus, Role::AccentForeground).size(12.0)}
        </div>
    }
}
