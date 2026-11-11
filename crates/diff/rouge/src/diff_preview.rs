use crate::{
    style,
    ui::{
        components::{Button, ControlSize},
        theme::{UiTheme, color},
        tokens::{RADIUS_CONTROL, rem, scaled},
        typography::{Typography, TypographyExt},
    },
};
use clankerdiff_core::{
    DiffDocument, DiffError, DiffPresentation, DiffTone, PresentationOptions, RowKind, ViewMode,
    parse_detached_git_diff,
};
use clankerdiff_symbols::DocumentSyntax;
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Styled, View, div, text};
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
        let view = div()
            .test_id(id.clone())
            .flex()
            .col()
            .min_w(0.0)
            .overflow_hidden()
            .rounded(scaled(RADIUS_CONTROL))
            .border(rem(1.0))
            .border_color(color(theme.ui.border_subtle))
            .bg(color(theme.diff.background));
        let Ok((presentation, syntax)) = &self.0 else {
            return if let Err((error, head)) = &self.0 {
                view.child(text(format!("Could not display patch: {error}")).color(color(theme.diff.muted)))
                    .child(text(head.clone()).typography(Typography::Code).color(color(theme.diff.foreground)))
            } else {
                view
            };
        };
        let mut rows = presentation
            .rows(0..presentation.row_count())
            .iter()
            .filter(|row| row.kind != RowKind::ExpandGap)
            .filter_map(|row| Some((row, row.primary_cell()?)));
        let mut view = view.children(rows.by_ref().take(MAX_PREVIEW_ROWS).map(|(row, cell)| {
            if !matches!(row.kind, RowKind::Code | RowKind::ExpandedContext) {
                let typography =
                    if row.kind == RowKind::FileHeader { Typography::BodyStrong } else { Typography::Caption };
                return div()
                    .px(rem(8.0))
                    .child(text(cell.text.to_string()).typography(typography).color(color(theme.diff.muted)));
            }
            let tone = theme.diff.tone(cell.tone);
            let marker = match cell.tone {
                DiffTone::Added => "+",
                DiffTone::Removed => "−",
                _ => " ",
            };
            div()
                .px(rem(6.0))
                .flex()
                .row()
                .gap(rem(4.0))
                .bg(color(tone.background))
                .overflow_hidden()
                .child(text(marker).typography(Typography::Code).color(color(tone.foreground)))
                .child(
                    style::code(&cell.text, syntax.highlights(row, cell), &theme.syntax)
                        .color(color(theme.diff.foreground)),
                )
        }));
        let hidden = rows.count();
        if hidden > 0 {
            let document = Arc::clone(presentation.document());
            view = view.child(
                Button::new(
                    format!("{id}-more"),
                    format!("… {hidden} more rows · Open full diff"),
                    UiTheme::new(theme),
                )
                .size(ControlSize::Small)
                .full_width()
                .on_click(move || on_open(&document)),
            );
        }
        view
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
