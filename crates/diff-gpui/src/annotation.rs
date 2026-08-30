use crate::style;
use diff_theme::DiffPalette;
use gpui::{Div, SharedString, div, prelude::*, px};

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
        .left(px(5.0))
        .top(px(1.0))
        .size(px(18.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .invisible()
        .group_hover(hover_group, gpui::Styled::visible)
        .cursor_pointer()
        .bg(style::color(palette.accent))
        .text_color(style::color(palette.background))
        .font_weight(gpui::FontWeight::BOLD)
        .hover(|button| button.opacity(0.85))
        .child("+")
}
