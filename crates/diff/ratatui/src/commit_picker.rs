use crate::{
    DiffReviewCommand, DiffReviewState, RatatuiTheme,
    state::{Hit, Overlay},
    style::stat_spans,
    ui::{Modal, ModalSize, SelectionState, render_picker},
};
use clankerdiff_core::{CommitPage, LineStat};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const ID_WIDTH: usize = 13;
const AGE_WIDTH: usize = 5;
const STAT_WIDTH: usize = 12;
const COMMENTS_WIDTH: usize = 4;
const SHOW_ID_FROM: usize = 40;
const SHOW_AGE_FROM: usize = 48;
const SHOW_STAT_FROM: usize = 56;
const PREVIEW_FROM: u16 = 110;
const PREVIEW_MAX_WIDTH: u16 = 124;
const PREVIEW_MAX_HEIGHT: u16 = 24;

pub(crate) fn render_commit_picker(
    area: Rect,
    buffer: &mut Buffer,
    state: &mut DiffReviewState,
    theme: &RatatuiTheme,
) -> Option<Rect> {
    let Some(Overlay::Commits(picker)) = &state.overlay else {
        return None;
    };
    let preview = area.width >= PREVIEW_FROM;
    let mut modal = Modal::new("Commits", ModalSize::Wide, theme).hint("j/k move · Enter open · 1-9 jump · Esc close");
    if preview {
        modal = modal.width(PREVIEW_MAX_WIDTH).height(PREVIEW_MAX_HEIGHT);
    }
    let content = modal.render(area, buffer);
    let (list, details) = if preview {
        let [list, separator, details] =
            Layout::horizontal([Constraint::Percentage(55), Constraint::Length(1), Constraint::Min(0)]).areas(content);
        for y in separator.top()..separator.bottom() {
            Paragraph::new("│").style(Style::new().fg(theme.ui.border)).render(Rect::new(separator.x, y, 1, 1), buffer);
        }
        (list, Some(details))
    } else {
        (content, None)
    };
    let pages = picker.pages();
    let rows = render_picker(list, buffer, theme, picker.picker(), None, state.pointer, |width| {
        commit_rows(pages, width, theme)
    });
    if let (Some(details), Some(page)) = (details, pages.get(picker.highlighted())) {
        render_preview(details, buffer, page, theme);
    }
    let hits = rows
        .rows
        .into_iter()
        .map(|(row, index)| Hit::new(row, DiffReviewCommand::GoToPage(index), pages[index].subject.clone()));
    state.hits.extend(hits);
    Some(content)
}

fn render_preview(area: Rect, buffer: &mut Buffer, page: &CommitPage, theme: &RatatuiTheme) {
    let base = theme.ui.selection_style(SelectionState::None);
    let muted = base.fg(theme.ui.text_muted);
    let inner = Rect::new(area.x.saturating_add(2), area.y, area.width.saturating_sub(3), area.height);
    let kind = match page.title.kind {
        _ if page.is_working_tree() => "Uncommitted".to_owned(),
        Some(kind) => kind.label().to_owned(),
        None => "Commit".to_owned(),
    };
    let mut eyebrow = vec![Span::styled(kind, base.fg(theme.ui.accent))];
    if let Some(scope) = &page.title.scope {
        eyebrow.push(Span::styled(format!(" · {scope}"), muted));
    }
    let mut lines = vec![
        Line::from(eyebrow),
        Line::styled(page.title.summary.clone(), base.add_modifier(Modifier::BOLD)),
        Line::default(),
    ];
    lines.extend(page.body.lines().map(|line| Line::styled(line.to_owned(), base.fg(theme.ui.text_secondary))));
    let mut facts = vec![Span::styled(page.id_label().to_owned(), muted)];
    if let Some(age) = &page.age {
        facts.push(Span::styled(format!(" · {age}"), muted));
    }
    if let Some(stat) = page.stat {
        facts.push(Span::raw("  "));
        facts.extend(stat_spans(stat, theme));
    }
    let [body, footer] = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(inner);
    Paragraph::new(lines).style(base).wrap(Wrap { trim: false }).render(body, buffer);
    Paragraph::new(vec![
        Line::styled("─".repeat(usize::from(footer.width)), Style::new().fg(theme.ui.border)),
        Line::from(facts),
    ])
    .style(base)
    .render(footer, buffer);
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
                spans.extend(padded_stat(page.stat, theme));
            }
            Line::from(spans)
        })
        .collect()
}

fn padded_stat(stat: Option<LineStat>, theme: &RatatuiTheme) -> Vec<Span<'static>> {
    let spans = stat.map_or_else(Vec::new, |stat| stat_spans(stat, theme).to_vec());
    let width: usize = spans.iter().map(Span::width).sum();
    [vec![Span::raw(" ".repeat(STAT_WIDTH.saturating_sub(width)))], spans].concat()
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
