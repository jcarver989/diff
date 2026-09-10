use clankerdiff_core::testing::DocumentBuilder;
#[cfg(feature = "test-support")]
use clankerdiff_core::{DiffDocument, DiffSide, RowKind};
#[cfg(feature = "test-support")]
use clankerdiff_ratatui::{
    DiffReviewCommand, InteractionPhase, ReviewCommand, composite_color, page_color,
    testing::ReviewHarnessBuilder,
};
use clankerdiff_ratatui::{DiffReviewState, DiffReviewWidget, NavigationPane, ReviewOptions};
#[cfg(feature = "test-support")]
use clankerdiff_theme::{ReviewTheme, Rgba};
#[cfg(feature = "test-support")]
use ratatui::style::Color;
use ratatui::{
    buffer::{Buffer, Cell},
    layout::Rect,
    widgets::StatefulWidget,
};
#[cfg(feature = "test-support")]
use std::{error::Error, sync::Arc};

#[test]
fn diff_tabs_match_explicit_spaces_including_syntax_styles() {
    assert_eq!(ReviewOptions::default().tab_width, 2);
    for width in [1, 5, 8, 40, 120] {
        for (tab_width, expanded) in [
            (2, "  let x = 1;\na b\n界  x\n    spaces\n"),
            (4, "    let x = 1;\na   b\n界  x\n    spaces\n"),
            (0, " let x = 1;\na b\n界 x\n    spaces\n"),
        ] {
            let source = "\tlet x = 1;\na\tb\n界\tx\n    spaces\n";
            let actual = render(source, tab_width, width);
            let expected = render(expanded, tab_width, width);
            assert_eq!(
                actual, expected,
                "tab width {tab_width}, area width {width}"
            );
            if width == 40 {
                let text: String = actual.content.iter().map(Cell::symbol).collect();
                assert!(text.contains(expanded.lines().next().unwrap_or_default()));
            }
        }
    }
}

#[test]
fn changing_tab_width_updates_existing_review_state() {
    let mut state = DiffReviewState::new(
        DocumentBuilder::new()
            .changed("tabs.rs", "", "\tlet x = 1;\n")
            .build(),
    );
    let area = Rect::new(0, 0, 40, 20);
    for tab_width in [2, 4, 0, 2] {
        state.set_options(ReviewOptions {
            tab_width,
            navigation: NavigationPane::Hidden,
            footer: false,
        });
        let mut actual = Buffer::empty(area);
        DiffReviewWidget::new()
            .borders(false)
            .render(area, &mut actual, &mut state);
        assert_eq!(actual, render("\tlet x = 1;\n", tab_width, 40));
    }
}

#[cfg(feature = "test-support")]
#[test]
fn ui_only_changes_recolor_loading_and_empty_states() {
    let mut harness = ReviewHarnessBuilder::default().dimensions(60, 8).build();
    harness.state_mut().set_options(ReviewOptions {
        footer: false,
        navigation: NavigationPane::Hidden,
        ..Default::default()
    });
    let mut theme = ReviewTheme::default();
    let syntax_revision = theme.syntax.revision();
    for (info, secondary) in [
        (Rgba::new(1, 150, 230, 255), Rgba::new(130, 140, 150, 255)),
        (Rgba::new(80, 190, 240, 255), Rgba::new(170, 180, 190, 255)),
    ] {
        theme.ui.info = info;
        theme.ui.text_secondary = secondary;
        harness.state_mut().set_theme(theme.clone());
        harness.draw();
        harness.assert_contains("No changes");
        assert!(harness.buffer().content().iter().any(|cell| {
            cell.symbol() == "N" && cell.fg == composite_color(secondary, theme.ui.canvas)
        }));
        harness.state_mut().set_loading();
        harness.draw();
        harness.assert_contains("Loading");
        assert!(harness.buffer().content().iter().any(|cell| {
            cell.symbol() == "L" && cell.fg == composite_color(info, theme.ui.canvas)
        }));
        harness
            .state_mut()
            .set_document(Arc::new(DiffDocument::empty()));
    }
    assert_eq!(theme.syntax.revision(), syntax_revision);
}

#[cfg(feature = "test-support")]
#[test]
fn selected_source_rows_use_diff_selection_independently_of_ui() -> Result<(), Box<dyn Error>> {
    for width in [60, 120] {
        let mut harness = ReviewHarnessBuilder {
            document: DocumentBuilder::new()
                .changed("plain.txt", "context\nold\n", "context\nnew\n")
                .build(),
            width,
            ..Default::default()
        }
        .build();
        harness.state_mut().set_options(ReviewOptions {
            footer: false,
            navigation: NavigationPane::Hidden,
            ..Default::default()
        });
        let mut theme = ReviewTheme::default();
        theme.diff.selection = Rgba::new(40, 60, 80, 128);
        for ui_background in [Rgba::new(250, 250, 250, 255), Rgba::new(0, 240, 0, 255)] {
            theme.ui.surface_selected = ui_background;
            harness.state_mut().set_theme(theme.clone());
            harness.draw();
            let presentation = harness.state().presentation();
            let row = presentation
                .rows(0..presentation.row_count())
                .iter()
                .position(|row| {
                    row.kind == RowKind::Code
                        && row.cells().any(|cell| cell.text.as_ref() == "context")
                })
                .ok_or("missing context row")?;
            harness
                .state_mut()
                .handle_command(DiffReviewCommand::SelectRow(row));
            harness
                .state_mut()
                .handle_command(DiffReviewCommand::SelectSide(DiffSide::New));
            harness.draw();
            assert_eq!(harness.state().selected_row(), Some(row));
            assert!(
                harness.buffer().content().iter().any(|cell| {
                    cell.symbol() == "c"
                        && cell.bg == page_color(&theme, theme.diff.selection)
                        && cell.fg == page_color(&theme, theme.diff.foreground)
                }),
                "width {width}: {}",
                harness.text()
            );
        }
    }
    Ok(())
}

#[cfg(feature = "test-support")]
#[test]
fn modal_text_composites_against_the_rendered_surface() {
    let mut harness = ReviewHarnessBuilder::default().build();
    harness.state_mut().set_options(ReviewOptions {
        footer: false,
        navigation: NavigationPane::Hidden,
        ..Default::default()
    });
    let mut theme = ReviewTheme::default();
    theme.ui.canvas = Rgba::new(0, 0, 0, 255);
    theme.ui.surface = Rgba::new(255, 255, 255, 128);
    theme.ui.text = Rgba::new(0, 0, 0, 128);
    theme.ui.border = theme.ui.text;
    harness.state_mut().set_theme(theme);
    harness.state_mut().handle_command(ReviewCommand::ShowHelp);
    harness.draw();
    assert_eq!(harness.state().interaction_phase(), InteractionPhase::Help);
    harness.assert_contains("Review shortcuts");
    let modal_cells: Vec<_> = harness
        .buffer()
        .content()
        .iter()
        .filter(|cell| cell.bg == Color::Rgb(128, 128, 128) && cell.symbol() != " ")
        .collect();
    assert!(!modal_cells.is_empty());
    for cell in modal_cells {
        assert_eq!(cell.fg, Color::Rgb(64, 64, 64), "{}", cell.symbol());
    }
}

fn render(source: &str, tab_width: u16, width: u16) -> Buffer {
    let mut state = DiffReviewState::new(
        DocumentBuilder::new()
            .changed("tabs.rs", "", source)
            .build(),
    );
    state.set_options(ReviewOptions {
        tab_width,
        navigation: NavigationPane::Hidden,
        footer: false,
    });
    let area = Rect::new(0, 0, width, 20);
    let mut buffer = Buffer::empty(area);
    DiffReviewWidget::new()
        .borders(false)
        .render(area, &mut buffer, &mut state);
    buffer
}
