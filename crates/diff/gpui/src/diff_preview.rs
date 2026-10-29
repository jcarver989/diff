use crate::style::{color, highlight_style};
use crate::ui::icon::{Icon, IconName};
use crate::ui::prelude::{Button, ControlSize, UiTheme};
use crate::ui::tokens::{ICON_SIZE_SMALL, RADIUS_CONTROL, rem};
use clankerdiff_core::{
    DiffDocument, DiffError, DiffPresentation, DiffTone, Fingerprint, PresentationOptions, RowKind, ViewMode,
    parse_detached_git_diff,
};
use clankerdiff_syntax::{LanguageHint, SyntaxHighlighter};
use clankerdiff_ui_core::ReviewTheme;
use gpui::{AnyElement, App, Div, ElementId, FontWeight, Stateful, StyledText, Window, div, prelude::*, px};
use std::{collections::VecDeque, sync::Arc};

const MAX_CACHED_PATCHES: usize = 32;
const MAX_PREVIEW_ROWS: usize = 120;

#[derive(Default)]
pub struct DiffPreviewCache {
    entries: VecDeque<(Fingerprint, Result<DiffPresentation, DiffError>)>,
}

impl DiffPreviewCache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn render(
        &mut self,
        id: impl Into<ElementId>,
        patch: &str,
        theme: &ReviewTheme,
        highlighter: &mut SyntaxHighlighter,
        on_open: impl Fn(&Arc<DiffDocument>, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let id = id.into();
        let more = ElementId::NamedChild(Arc::new(id.clone()), "more".into());
        let selector = id.to_string();
        let view = div().id(id).debug_selector(move || selector).flex().flex_col().min_w_0();
        match self.entry(patch) {
            Ok(presentation) => render_preview(view, more, presentation, theme, highlighter, on_open),
            Err(error) => view
                .text_color(color(theme.diff.foreground))
                .child(div().text_color(color(theme.diff.muted)).child(format!("Could not display patch: {error}")))
                .child(patch.split_inclusive('\n').take(MAX_PREVIEW_ROWS).collect::<String>())
                .into_any_element(),
        }
    }

    /// The lines `patch` adds and removes, or `None` when it does not parse.
    pub fn stats(&mut self, patch: &str) -> Option<(usize, usize)> {
        let stat = self.entry(patch).as_ref().ok()?.document().line_stat();
        Some((stat.added, stat.deleted))
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
    view: Stateful<Div>,
    more: ElementId,
    presentation: &DiffPresentation,
    theme: &ReviewTheme,
    highlighter: &mut SyntaxHighlighter,
    on_open: impl Fn(&Arc<DiffDocument>, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let palette = &theme.diff;
    let mut rows = presentation
        .rows(0..presentation.row_count())
        .iter()
        .filter(|row| row.kind != RowKind::ExpandGap)
        .filter_map(|row| Some((row, row.primary_cell()?)));
    let mut view = view.gap_2();
    let mut card: Option<Div> = None;
    for (row, cell) in rows.by_ref().take(MAX_PREVIEW_ROWS) {
        if row.kind == RowKind::FileHeader {
            view = view.children(card.take());
            card = Some(file_card(theme).child(file_header(&cell.text, theme)));
            continue;
        }
        let body = card.take().unwrap_or_else(|| file_card(theme));
        if !matches!(row.kind, RowKind::Code | RowKind::ExpandedContext) {
            card = Some(body.child(div().px_2().text_color(color(palette.muted)).child(cell.text.to_string())));
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
            body.child(
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
    let hidden = rows.count();
    view.children(card)
        .when(hidden > 0, |view| {
            let document = presentation.document().clone();
            view.child(
                Button::new(more, format!("… {hidden} more rows · Open full diff"), UiTheme::new(theme))
                    .size(ControlSize::Small)
                    .full_width()
                    .aria_label("Open full diff")
                    .on_click(move |_, window, cx| on_open(&document, window, cx)),
            )
        })
        .into_any_element()
}

const CHANGE_BAR_WIDTH: f32 = 2.0;

fn file_card(theme: &ReviewTheme) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .overflow_hidden()
        .rounded(rem(RADIUS_CONTROL))
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
