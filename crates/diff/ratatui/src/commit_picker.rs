use crate::{
    RatatuiTheme,
    ui::{Modal, ModalList, ModalSize},
};
use clankerdiff_core::{CommitPage, CommitPicker, LineStat};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const ID_WIDTH: usize = 13;
const AGE_WIDTH: usize = 5;
const STAT_WIDTH: usize = 12;
const COMMENTS_WIDTH: usize = 4;
const SHOW_ID_FROM: usize = 40;
const SHOW_AGE_FROM: usize = 48;
const SHOW_STAT_FROM: usize = 56;

pub(crate) fn render_commit_picker(area: Rect, buffer: &mut Buffer, picker: &CommitPicker, theme: &RatatuiTheme) {
    let modal = Modal::new("Commits", ModalSize::Wide, theme).hint("j/k move · Enter open · 1-9 jump · Esc close");
    ModalList::new(modal, picker.highlighted(), theme)
        .render(area, buffer, |width| commit_rows(picker.pages(), width, theme));
}

fn commit_rows(pages: &[CommitPage], width: usize, theme: &RatatuiTheme) -> Vec<Line<'static>> {
    let muted = Style::new().fg(theme.ui.text_muted);
    let number_width = pages.len().to_string().len() + 1;
    let show_id = width >= SHOW_ID_FROM;
    let show_age = width >= SHOW_AGE_FROM && pages.iter().any(|page| page.age.is_some());
    let show_stat = width >= SHOW_STAT_FROM;
    let show_comments = pages.iter().any(|page| page.comments > 0);
    let fixed = 2
        + number_width
        + if show_id { ID_WIDTH } else { 0 }
        + if show_age { AGE_WIDTH } else { 0 }
        + if show_stat { STAT_WIDTH } else { 0 }
        + if show_comments { COMMENTS_WIDTH } else { 0 };
    let subject_width = width.saturating_sub(fixed);
    pages
        .iter()
        .map(|page| {
            let (marker, marker_style) = if page.current {
                ("● ", Style::new().fg(theme.ui.accent))
            } else if page.is_working_tree() {
                ("◌ ", Style::new().fg(theme.ui.warning))
            } else {
                ("○ ", muted)
            };
            let mut spans = vec![
                Span::styled(marker, marker_style),
                Span::styled(format!("{:<number_width$}", page.number), muted),
            ];
            if show_id {
                spans.push(Span::styled(format!("{:<ID_WIDTH$}", page.id_label()), muted));
            }
            spans.push(Span::raw(pad(&truncate(&page.subject, subject_width), subject_width)));
            if show_age {
                spans.push(Span::styled(format!("{:>AGE_WIDTH$}", page.age.as_deref().unwrap_or_default()), muted));
            }
            if show_comments {
                let comments = if page.comments == 0 { String::new() } else { format!("{}✎", page.comments) };
                spans.push(Span::styled(format!("{comments:>COMMENTS_WIDTH$}"), Style::new().fg(theme.ui.accent)));
            }
            if show_stat {
                spans.extend(stat_spans(page.stat, theme));
            }
            Line::from(spans)
        })
        .collect()
}

fn stat_spans(stat: Option<LineStat>, theme: &RatatuiTheme) -> Vec<Span<'static>> {
    let Some(stat) = stat else {
        return vec![Span::raw(" ".repeat(STAT_WIDTH))];
    };
    let added = format!("+{}", stat.added);
    let deleted = format!("−{}", stat.deleted);
    let padding = STAT_WIDTH.saturating_sub(added.width() + 1 + deleted.width());
    vec![
        Span::raw(" ".repeat(padding)),
        Span::styled(added, Style::new().fg(theme.addition)),
        Span::raw(" "),
        Span::styled(deleted, Style::new().fg(theme.deletion)),
    ]
}

fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for character in text.chars() {
        let next = character.width().unwrap_or(0);
        if used + next + 1 > width {
            break;
        }
        out.push(character);
        used += next;
    }
    if width > 0 {
        out.push('…');
    }
    out
}

fn pad(text: &str, width: usize) -> String {
    format!("{text}{}", " ".repeat(width.saturating_sub(text.width())))
}
