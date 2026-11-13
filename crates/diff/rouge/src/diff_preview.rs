use crate::style;
use clankerdiff_core::{
    DiffDocument, DiffError, DiffPresentation, DiffTone, PresentationOptions, PresentedCell, PresentedRow, RowKind,
    ViewMode, parse_detached_git_diff,
};
use clankerdiff_symbols::DocumentSyntax;
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{AnyView, Color, Div, Styled, Theme, View, div, text, view};
use rouge_reactive::use_context;
use rouge_widgets::{
    ControlSize, UiStyles, button,
    tokens::RADIUS_CONTROL,
    typography::{BODY_STRONG, CAPTION},
};
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
                <text color=Color::from(theme.diff.muted)>{format!("Could not display patch: {error}")}</text>
                <text code color=Color::from(theme.diff.foreground)>{head.clone()}</text>
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
                    if use_context::<Theme>().is_none() {
                        Theme::provide(theme.ui);
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
                 border_color=Color::from(theme.ui.border_subtle) bg=Color::from(theme.diff.background)>
                {..body}
            </div>
        }
    }
}

fn preview_row(row: &PresentedRow, cell: &PresentedCell, syntax: &DocumentSyntax, theme: &ReviewTheme) -> Div {
    if !matches!(row.kind, RowKind::Code | RowKind::ExpandedContext) {
        let typography = if row.kind == RowKind::FileHeader { BODY_STRONG } else { CAPTION };
        return view! {
            <div px=8.0>
                <text typography=typography color=Color::from(theme.diff.muted)>{cell.text.to_string()}</text>
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
        <div px=6.0 flex row gap=4.0 bg=Color::from(tone.background) overflow_hidden>
            <text code color=Color::from(tone.foreground)>{marker}</text>
            {style::code(&cell.text, syntax.highlights(row, cell), &theme.syntax).color(Color::from(theme.diff.foreground))}
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
