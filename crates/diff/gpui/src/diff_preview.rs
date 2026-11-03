use crate::style::{self, color};
use crate::ui::icon::{Icon, IconName};
use crate::ui::prelude::{Button, ControlSize, UiTheme};
use crate::ui::tokens::{ICON_SIZE_SMALL, RADIUS_CONTROL, rem};
use clankerdiff_core::{
    DiffDocument, DiffError, DiffPresentation, DiffTone, PresentationOptions, RowKind, ViewMode,
    parse_detached_git_diff,
};
use clankerdiff_symbols::DocumentSyntax;
use clankerdiff_ui_core::ReviewTheme;
use gpui::{AnyElement, App, Div, ElementId, FontWeight, Stateful, StyledText, Window, div, prelude::*, px};
use std::sync::Arc;

const MAX_PREVIEW_ROWS: usize = 120;

pub struct PatchPreview(Result<(DiffPresentation, DocumentSyntax), (DiffError, String)>);

impl PatchPreview {
    #[must_use]
    pub fn parse(patch: &str) -> Self {
        Self(parse_patch(patch).map_err(|error| (error, patch.split_inclusive('\n').take(MAX_PREVIEW_ROWS).collect())))
    }

    #[must_use]
    pub fn stats(&self) -> Option<(usize, usize)> {
        let stat = self.0.as_ref().ok()?.0.document().line_stat();
        Some((stat.added, stat.deleted))
    }

    pub fn render(
        &self,
        id: impl Into<ElementId>,
        theme: &ReviewTheme,
        on_open: impl Fn(&Arc<DiffDocument>, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let id = id.into();
        let more = ElementId::NamedChild(Arc::new(id.clone()), "more".into());
        let selector = id.to_string();
        let view = div().id(id).debug_selector(move || selector).flex().flex_col().min_w_0();
        match &self.0 {
            Ok((presentation, syntax)) => render_preview(view, more, presentation, syntax, theme, on_open),
            Err((error, head)) => view
                .text_color(color(theme.diff.foreground))
                .child(div().text_color(color(theme.diff.muted)).child(format!("Could not display patch: {error}")))
                .child(head.clone())
                .into_any_element(),
        }
    }
}

fn parse_patch(patch: &str) -> Result<(DiffPresentation, DocumentSyntax), DiffError> {
    let files = parse_detached_git_diff(patch.as_bytes())?;
    let document = Arc::new(DiffDocument { repo_root: String::new(), files, ..DiffDocument::default() });
    let presentation = DiffPresentation::new(
        Arc::clone(&document),
        PresentationOptions { view_mode: ViewMode::Unified, ..PresentationOptions::default() },
    );
    Ok((presentation, DocumentSyntax::new(document)))
}

fn render_preview(
    view: Stateful<Div>,
    more: ElementId,
    presentation: &DiffPresentation,
    syntax: &DocumentSyntax,
    theme: &ReviewTheme,
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
        let highlights = style::highlights(&syntax.highlights(row, cell), &theme.syntax);
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
