use crate::{
    style,
    ui::{
        components::{ControlSize, button},
        theme::{UiColors, UiTheme, color},
        tokens::RADIUS_CONTROL,
        typography::{Typography, UiStyles},
    },
};
use clankerdiff_core::{
    DiffDocument, DiffError, DiffPresentation, DiffTone, PresentationOptions, PresentedCell, PresentedRow, RowKind,
    ViewMode, parse_detached_git_diff,
};
use clankerdiff_symbols::DocumentSyntax;
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{AnyView, Div, Styled, View, div, text, view};
use rouge_reactive::use_context;
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

    pub fn render<T: Into<String>, U: Fn(&Arc<DiffDocument>) + 'static>(
        &self,
        id: T,
        theme: &ReviewTheme,
        on_open: U,
    ) -> impl View + use<T, U> {
        let id = id.into();
        let body: Vec<AnyView> = match &self.0 {
            Err((error, head)) => view! {
                <text color=color(theme.diff.muted)>{format!("Could not display patch: {error}")}</text>
                <text code color=color(theme.diff.foreground)>{head.clone()}</text>
            },
            Ok((presentation, syntax)) => {
                let mut rows = presentation
                    .rows(0..presentation.row_count())
                    .iter()
                    .filter(|row| row.kind != RowKind::ExpandGap)
                    .filter_map(|row| Some((row, row.primary_cell()?)));
                let mut body: Vec<AnyView> = rows
                    .by_ref()
                    .take(MAX_PREVIEW_ROWS)
                    .map(|(row, cell)| preview_row(row, cell, syntax, theme).into_any())
                    .collect();
                let hidden = rows.count();
                if hidden > 0 {
                    let document = Arc::clone(presentation.document());
                    if use_context::<UiTheme>().is_none() {
                        UiTheme::provide(UiColors::from_palette(&theme.ui));
                    }
                    body.push(view! {
                        <button id={format!("{id}-more")} label={format!("… {hidden} more rows · Open full diff")}
                                size=ControlSize::Small full_width on_click={move || on_open(&document)}/>
                    });
                }
                body
            }
        };
        view! {
            <div test_id={id.clone()} flex col min_w=0.0 overflow_hidden rounded=RADIUS_CONTROL border=1.0
                 border_color=color(theme.ui.border_subtle) bg=color(theme.diff.background)>
                {..body}
            </div>
        }
    }
}

fn preview_row(row: &PresentedRow, cell: &PresentedCell, syntax: &DocumentSyntax, theme: &ReviewTheme) -> Div {
    if !matches!(row.kind, RowKind::Code | RowKind::ExpandedContext) {
        let typography = if row.kind == RowKind::FileHeader { Typography::BodyStrong } else { Typography::Caption };
        return view! {
            <div px=8.0>
                <text typography=typography color=color(theme.diff.muted)>{cell.text.to_string()}</text>
            </div>
        };
    }
    let tone = theme.diff.tone(cell.tone);
    let marker = match cell.tone {
        DiffTone::Added => "+",
        DiffTone::Removed => "−",
        _ => " ",
    };
    view! {
        <div px=6.0 flex row gap=4.0 bg=color(tone.background) overflow_hidden>
            <text code color=color(tone.foreground)>{marker}</text>
            {style::code(&cell.text, syntax.highlights(row, cell), &theme.syntax).color(color(theme.diff.foreground))}
        </div>
    }
}

fn parse_patch(patch: &str) -> Result<(DiffPresentation, DocumentSyntax), DiffError> {
    let files = parse_detached_git_diff(patch.as_bytes())?;
    let document = Arc::new(DiffDocument { files, ..DiffDocument::default() });
    let presentation = DiffPresentation::new(
        Arc::clone(&document),
        PresentationOptions { view_mode: ViewMode::Unified, ..PresentationOptions::default() },
    );
    Ok((presentation, DocumentSyntax::new(document)))
}
