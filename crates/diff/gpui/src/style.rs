//! Conversion from renderer-neutral diff styles to GPUI styles.

use crate::ui::typography::{Typography, TypographyExt};
use clankerdiff_core::{FileStatus, LineStat};
pub(crate) use clankerdiff_gpui_ui::theme::color;
use clankerdiff_syntax::HighlightSpan;
use clankerdiff_ui_core::{DiffPalette, FontStyle, Rgba};
use gpui::{
    Div, FontStyle as GpuiFontStyle, FontWeight, HighlightStyle, ParentElement, SharedString, Styled, StyledText,
    UnderlineStyle, div, px,
};

pub(crate) fn code_text(line: SharedString, spans: &[HighlightSpan]) -> StyledText {
    StyledText::new(line).with_highlights(
        spans.iter().map(|span| (span.range.clone(), highlight_style(span.foreground, span.font_style))),
    )
}

/// Converts a syntax-highlight modifier and color to a GPUI highlight style.
#[must_use]
pub fn highlight_style(foreground: Rgba, style: FontStyle) -> HighlightStyle {
    HighlightStyle {
        color: Some(color(foreground)),
        font_weight: style.bold.then_some(FontWeight::BOLD),
        font_style: style.italic.then_some(GpuiFontStyle::Italic),
        underline: style.underline.then(|| UnderlineStyle {
            color: Some(color(foreground)),
            thickness: px(1.0),
            wavy: false,
        }),
        ..Default::default()
    }
}

pub(crate) const fn file_status_color(status: FileStatus, palette: &DiffPalette) -> Rgba {
    match status {
        FileStatus::Added | FileStatus::Untracked => palette.addition,
        FileStatus::Deleted => palette.deletion,
        FileStatus::Modified | FileStatus::Renamed | FileStatus::Copied => palette.accent,
    }
}

#[must_use]
pub fn stat_badges(palette: &DiffPalette, stat: LineStat) -> [Div; 2] {
    [
        div()
            .flex_shrink_0()
            .typography(Typography::Code)
            .text_color(color(palette.addition))
            .child(format!("+{}", stat.added)),
        div()
            .flex_shrink_0()
            .typography(Typography::Code)
            .text_color(color(palette.deletion))
            .child(format!("−{}", stat.deleted)),
    ]
}
