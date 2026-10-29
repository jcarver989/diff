use crate::{
    style,
    ui::prelude::{Icon, IconName, rem},
};
use clankerdiff_ui_core::DiffPalette;
use gpui::{Div, SharedString, div, prelude::*};

/// Builds a hover-revealed add-comment affordance.
///
/// The owning viewer attaches the click handler because only it knows how to
/// select its target and open an editor.
pub(crate) fn add_comment_button(
    id: impl Into<String>,
    hover_group: SharedString,
    palette: &DiffPalette,
) -> gpui::Stateful<Div> {
    div()
        .id(id.into())
        .aria_label("Add comment")
        .absolute()
        .left(rem(4.0))
        .top(rem(1.0))
        .size(rem(18.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .invisible()
        .group_hover(hover_group, gpui::Styled::visible)
        .cursor_pointer()
        .bg(style::color(palette.accent))
        .hover(|button| button.opacity(0.85))
        .child(Icon::new(IconName::Plus, style::color(palette.background)).size(12.0))
}
