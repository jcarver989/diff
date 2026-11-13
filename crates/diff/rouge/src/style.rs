use clankerdiff_core::{FileStatus, LineStat};
use clankerdiff_ui_core::{DiffPalette, ReviewTheme, Rgba};
use rouge_core::{Color, Div, Styled, Text, component, div, rich_text, text, view};
use rouge_reactive::ReadSignal;
use rouge_syntax::{HighlightLine, SyntaxTheme};
use rouge_widgets::{UiStyles, syntax_spans};

pub(crate) fn code(line: &str, spans: HighlightLine<'_>, theme: &SyntaxTheme) -> Text {
    rich_text(line.to_owned(), syntax_spans(line, spans, theme)).code().no_wrap()
}

pub(crate) const fn file_status_color(status: FileStatus, palette: &DiffPalette) -> Rgba {
    match status {
        FileStatus::Added | FileStatus::Untracked => palette.addition,
        FileStatus::Deleted => palette.deletion,
        FileStatus::Modified | FileStatus::Renamed | FileStatus::Copied => palette.accent,
    }
}

#[must_use]
#[component]
pub fn stat_badges(stat: LineStat, palette: ReadSignal<ReviewTheme>) -> Div {
    view! {
        <div flex row flex_none gap=8.0>
            <text code no_wrap color={move || palette.with(|t| Color::from(t.diff.addition))}>{format!("+{}", stat.added)}</text>
            <text code no_wrap color={move || palette.with(|t| Color::from(t.diff.deletion))}>{format!("−{}", stat.deleted)}</text>
        </div>
    }
}
