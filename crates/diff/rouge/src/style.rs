use clankerdiff_core::{FileStatus, LineStat};
use clankerdiff_rouge_ui::{
    theme::color,
    typography::{Typography, TypographyExt},
};
use clankerdiff_syntax::{HighlightLine, SyntaxTheme};
use clankerdiff_ui_core::{DiffPalette, ReviewTheme, Rgba};
use rouge_core::{Div, SpanStyle, Styled, Text, Underline, div, rich_text, text};
use rouge_reactive::ReadSignal;
use std::ops::Range;

pub(crate) fn highlights(line: &str, spans: HighlightLine<'_>, theme: &SyntaxTheme) -> Vec<(Range<usize>, SpanStyle)> {
    spans
        .iter()
        .filter_map(|span| {
            let style = theme.highlight(span.highlight)?;
            let range = span.range.start..span.range.end.min(line.len());
            let foreground = color(style.foreground);
            (!range.is_empty() && line.get(range.clone()).is_some()).then_some((
                range,
                SpanStyle {
                    color: Some(foreground),
                    weight: style.font_style.bold.then_some(700.0),
                    italic: style.font_style.italic.then_some(true),
                    underline: style.font_style.underline.then_some(Underline { color: foreground, thickness: 1.0 }),
                    ..SpanStyle::default()
                },
            ))
        })
        .collect()
}

pub(crate) fn code(line: &str, spans: HighlightLine<'_>, theme: &SyntaxTheme) -> Text {
    rich_text(line.to_owned(), highlights(line, spans, theme)).typography(Typography::Code).no_wrap()
}

pub(crate) const fn file_status_color(status: FileStatus, palette: &DiffPalette) -> Rgba {
    match status {
        FileStatus::Added | FileStatus::Untracked => palette.addition,
        FileStatus::Deleted => palette.deletion,
        FileStatus::Modified | FileStatus::Renamed | FileStatus::Copied => palette.accent,
    }
}

pub(crate) fn stat_badges(stat: LineStat, palette: ReadSignal<ReviewTheme>) -> Div {
    div()
        .flex()
        .row()
        .flex_none()
        .gap(clankerdiff_rouge_ui::tokens::rem(8.0))
        .child(
            text(format!("+{}", stat.added))
                .typography(Typography::Code)
                .no_wrap()
                .color(move || palette.with(|t| color(t.diff.addition))),
        )
        .child(
            text(format!("−{}", stat.deleted))
                .typography(Typography::Code)
                .no_wrap()
                .color(move || palette.with(|t| color(t.diff.deletion))),
        )
}
