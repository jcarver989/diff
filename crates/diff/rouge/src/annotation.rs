use crate::ReviewHandle;
use clankerdiff_core::{DiffSide, ReviewCommand};
use rouge_core::{Role, Styled, View, component};
use rouge_widgets::{
    IconName, icon_button,
    tokens::{ICON_SIZE_SMALL, Radius},
};

#[component]
pub(crate) fn add_comment(handle: ReviewHandle, row: usize, side: DiffSide) -> impl View {
    icon_button(IconName::Plus, "Add comment".to_owned())
        .test_id(format!("add-comment-{row}-{side:?}"))
        .size(ADD_COMMENT_SIZE)
        .icon_size(ICON_SIZE_SMALL)
        .rounded(Radius::Inner)
        .bg(Role::Accent)
        .hover(|style| style.bg(Role::Accent))
        .color(Role::AccentForeground)
        .on_click(move || {
            handle.line_command(row, side, ReviewCommand::BeginComment.into());
        })
}

pub(crate) const ADD_COMMENT_SIZE: f32 = 20.0;
