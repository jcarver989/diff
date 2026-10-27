use crate::style::{color, highlight_style};
use crate::ui::icon::{Icon, IconName};
use crate::ui::tokens::{ICON_SIZE_SMALL, RADIUS_CARD, rem};
use clankerdiff_core::{
    DiffDocument, DiffError, DiffPresentation, DiffTone, Fingerprint, PresentationOptions, RowKind, ViewMode,
    parse_detached_git_diff,
};
use clankerdiff_syntax::{LanguageHint, SyntaxHighlighter};
use clankerdiff_theme::ReviewTheme;
use gpui::{AnyElement, Div, FontWeight, StyledText, div, prelude::*, px};
use std::{collections::VecDeque, sync::Arc};

const MAX_CACHED_PATCHES: usize = 32;

#[derive(Default)]
pub struct DiffPreviewCache {
    entries: VecDeque<(Fingerprint, Result<DiffPresentation, DiffError>)>,
}

impl DiffPreviewCache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn render(&mut self, patch: &str, theme: &ReviewTheme, highlighter: &mut SyntaxHighlighter) -> AnyElement {
        match self.entry(patch) {
            Ok(presentation) => render_preview(presentation, theme, highlighter),
            Err(error) => div()
                .flex()
                .flex_col()
                .text_color(color(theme.diff.foreground))
                .child(div().text_color(color(theme.diff.muted)).child(format!("Could not display patch: {error}")))
                .child(patch.to_owned())
                .into_any_element(),
        }
    }

    /// The lines `patch` adds and removes, or `None` when it does not parse.
    pub fn stats(&mut self, patch: &str) -> Option<(usize, usize)> {
        let files = &self.entry(patch).as_ref().ok()?.document().files;
        Some(files.iter().fold((0, 0), |(added, removed), file| (added + file.additions(), removed + file.deletions())))
    }

    fn entry(&mut self, patch: &str) -> &Result<DiffPresentation, DiffError> {
        let id = Fingerprint::of([patch.as_bytes()]);
        let index = self.entries.iter().position(|(cached, _)| *cached == id).unwrap_or_else(|| {
            if self.entries.len() == MAX_CACHED_PATCHES {
                self.entries.pop_front();
            }
            self.entries.push_back((id, parse_patch(patch)));
            self.entries.len() - 1
        });
        &self.entries[index].1
    }
}

fn parse_patch(patch: &str) -> Result<DiffPresentation, DiffError> {
    let files = parse_detached_git_diff(patch.as_bytes())?;
    Ok(DiffPresentation::new(
        Arc::new(DiffDocument { repo_root: String::new(), files, ..DiffDocument::default() }),
        PresentationOptions { view_mode: ViewMode::Unified, ..PresentationOptions::default() },
    ))
}

fn render_preview(
    presentation: &DiffPresentation,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
) -> AnyElement {
    let palette = &theme.diff;
    let mut view = div().flex().flex_col().gap_2().min_w_0();
    let mut card: Option<Div> = None;
    for row in presentation.rows(0..presentation.row_count()) {
        if row.kind == RowKind::ExpandGap {
            continue;
        }
        let Some(cell) = row.primary_cell() else {
            continue;
        };
        if row.kind == RowKind::FileHeader {
            view = view.children(card.take());
            card = Some(file_card(theme).child(file_header(&cell.text, theme)));
            continue;
        }
        let rows = card.take().unwrap_or_else(|| file_card(theme));
        if !matches!(row.kind, RowKind::Code | RowKind::ExpandedContext) {
            card = Some(rows.child(div().px_2().text_color(color(palette.muted)).child(cell.text.to_string())));
            continue;
        }
        let source = presentation.cell_context(row, cell);
        let highlights = highlighter
            .with_theme(&theme.syntax)
            .highlight_document(source.id, LanguageHint::Path(source.path), || source.text())
            .ok()
            .map(|document| {
                document
                    .line(source.target_line)
                    .unwrap_or_default()
                    .iter()
                    .map(|span| (span.range.clone(), highlight_style(span.foreground, span.font_style)))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let indicator = match cell.tone {
            DiffTone::Added => "+",
            DiffTone::Removed => "−",
            _ => " ",
        };
        let tone = palette.tone(cell.tone);
        let changed = matches!(cell.tone, DiffTone::Added | DiffTone::Removed);
        let text = if cell.text.is_empty() { " ".to_owned() } else { cell.text.to_string() };
        card = Some(
            rows.child(
                div()
                    .flex()
                    .items_start()
                    .gap_1()
                    .min_w_0()
                    .px_2()
                    .border_l(px(CHANGE_BAR_WIDTH))
                    .border_color(if changed { color(tone.foreground) } else { gpui::transparent_black() })
                    .bg(color(tone.background))
                    .child(div().flex_none().text_color(color(tone.foreground)).child(indicator))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_color(color(palette.foreground))
                            .child(StyledText::new(text).with_highlights(highlights)),
                    ),
            ),
        );
    }
    view.children(card).into_any_element()
}

const CHANGE_BAR_WIDTH: f32 = 2.0;

fn file_card(theme: &ReviewTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .overflow_hidden()
        .rounded(rem(RADIUS_CARD))
        .border_1()
        .border_color(color(theme.ui.border_subtle))
        .bg(color(theme.diff.background))
}

fn file_header(path: &str, theme: &ReviewTheme) -> Div {
    let (directory, name) = match path.rsplit_once('/') {
        Some((directory, name)) if !path.contains('→') => (Some(directory.to_owned()), name.to_owned()),
        _ => (None, path.to_owned()),
    };
    let muted = color(theme.diff.muted);
    div()
        .flex()
        .items_center()
        .gap_2()
        .min_w_0()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(color(theme.ui.border_subtle))
        .child(Icon::new(IconName::File, muted).size(ICON_SIZE_SMALL))
        .child(div().flex_none().font_weight(FontWeight::MEDIUM).text_color(color(theme.diff.foreground)).child(name))
        .children(directory.map(|directory| div().min_w_0().truncate().text_color(muted).child(directory)))
}
