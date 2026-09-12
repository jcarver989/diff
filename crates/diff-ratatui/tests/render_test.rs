#[cfg(feature = "test-support")]
use clankerdiff_core::{DiffDocument, DiffSide, RowKind};
use clankerdiff_core::{ViewMode, testing::DocumentBuilder};
#[cfg(feature = "test-support")]
use clankerdiff_ratatui::{
    DiffReviewCommand, InteractionPhase, KeyCode, ReviewCommand, composite_color, page_color,
    testing::ReviewHarnessBuilder,
};
use clankerdiff_ratatui::{DiffReviewState, DiffReviewWidget, NavigationPane, ReviewOptions};
#[cfg(feature = "test-support")]
use clankerdiff_theme::{ReviewTheme, Rgba};
#[cfg(feature = "test-support")]
use ratatui::style::Color;
use ratatui::{buffer::Buffer, layout::Rect, widgets::StatefulWidget};
use std::error::Error;
#[cfg(feature = "test-support")]
use std::sync::Arc;

#[test]
fn diff_tabs_match_explicit_spaces_including_syntax_styles() {
    for width in [40, 120] {
        for (tab_width, expanded) in [
            (2, "  let x = 1;\na b\n界  x\n    spaces\n"),
            (4, "    let x = 1;\na   b\n界  x\n    spaces\n"),
            (0, " let x = 1;\na b\n界 x\n    spaces\n"),
        ] {
            let source = "\tlet x = 1;\na\tb\n界\tx\n    spaces\n";
            let actual = render("", source, tab_width, width);
            let expected = render("", expanded, tab_width, width);
            assert_eq!(
                actual, expected,
                "tab width {tab_width}, area width {width}"
            );
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
        assert_eq!(actual, render("", "\tlet x = 1;\n", tab_width, 40));
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

#[test]
fn unified_wrapping_preserves_added_and_removed_source_text() {
    let source = "alpha_beta_gamma_delta_epsilon_zeta_eta_theta";
    for (old, new) in [("", source), (source, "")] {
        let buffer = render(old, new, 2, 20);
        let actual: String = source_rows(&buffer)
            .iter()
            .map(|row| {
                row.chars()
                    .skip(6)
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect();
        assert_eq!(actual, source);
    }
}

#[test]
fn split_cells_wrap_independently_and_pad_the_shorter_side() -> Result<(), Box<dyn Error>> {
    let mut state = DiffReviewState::new(
        DocumentBuilder::new()
            .changed(
                "split.rs",
                "short\nnext old\n",
                "a_very_long_replacement_value_that_wraps\nnext new\n",
            )
            .build(),
    );
    state.set_options(ReviewOptions {
        navigation: NavigationPane::Hidden,
        footer: false,
        ..Default::default()
    });
    state.set_view_mode(ViewMode::Split);
    let area = Rect::new(0, 0, 40, 20);
    let mut buffer = Buffer::empty(area);
    DiffReviewWidget::new()
        .borders(false)
        .render(area, &mut buffer, &mut state);
    let continuation = (0..area.height)
        .find(|row| buffer_row(&buffer, *row).contains('↪'))
        .ok_or("missing continuation")?;
    let following = (0..area.height)
        .find(|row| buffer_row(&buffer, *row).contains("next new"))
        .ok_or("missing following row")?;
    assert!(following > continuation, "missing continuation run");
    let background = buffer[(0, continuation - 1)].bg;
    for y in continuation..following {
        assert_eq!(buffer[(19, y)].symbol(), "│");
        for x in 0..19 {
            assert_eq!(buffer[(x, y)].symbol(), " ");
            assert_eq!(buffer[(x, y)].bg, background);
        }
    }
    Ok(())
}

#[cfg(feature = "test-support")]
#[test]
fn annotations_follow_the_taller_split_cell() -> Result<(), Box<dyn Error>> {
    for draft in [false, true] {
        let mut harness = ReviewHarnessBuilder {
            document: DocumentBuilder::new()
                .changed(
                    "draft.rs",
                    "short\n",
                    "abcdefghijklmnopqrstuvwxyz0123456789\n",
                )
                .build(),
            width: 40,
            height: 20,
        }
        .build();
        harness.state_mut().set_options(ReviewOptions {
            navigation: NavigationPane::Hidden,
            footer: false,
            ..Default::default()
        });
        harness.state_mut().set_view_mode(ViewMode::Split);
        harness.draw();
        if draft {
            harness.press(KeyCode::Char('c'));
            harness.type_text("note");
        } else {
            let anchor = harness
                .state()
                .session()
                .selected_anchor()
                .ok_or("missing anchor")?;
            harness.state_mut().review_mut().add_comment(anchor, "note");
        }
        harness.draw();
        let last = (0..20)
            .rfind(|y| harness.row_text(*y).contains('↪'))
            .ok_or("missing continuation")?;
        let annotation = (0..20)
            .find(|y| {
                harness
                    .row_text(*y)
                    .contains(if draft { "Draft" } else { "Comment" })
            })
            .ok_or("missing annotation")?;
        assert!(annotation > last);
        if draft {
            let cursor = harness
                .state()
                .cursor_position()
                .ok_or("missing draft cursor")?;
            assert!(harness.row_text(cursor.y).contains("note"));
        }
    }
    Ok(())
}

#[test]
fn width_changes_reflow_cached_source_rows() {
    let source = "abcdefghijklmnopqrstuvwxyz0123456789\n";
    let document = DocumentBuilder::new()
        .changed("tabs.rs", "", source)
        .build();
    let mut state = DiffReviewState::new(document);
    for width in [0, 5, 6, 7, 8, 20, 8, 40] {
        let tab_width = 2;
        state.set_options(ReviewOptions {
            tab_width,
            navigation: NavigationPane::Hidden,
            footer: false,
        });
        let area = Rect::new(0, 0, width, 20);
        let mut actual = Buffer::empty(area);
        DiffReviewWidget::new()
            .borders(false)
            .render(area, &mut actual, &mut state);
        assert_eq!(
            source_rows(&actual),
            source_rows(&render("", source, tab_width, width)),
            "width {width}"
        );
    }
}

#[test]
fn empty_split_cells_have_gutters_but_missing_cells_do_not() -> Result<(), Box<dyn Error>> {
    for (old, new, present_x, missing_x) in [("\n", "", 0, 13), ("", "\n", 13, 0)] {
        let mut state = DiffReviewState::new(
            DocumentBuilder::new()
                .changed("plain.txt", old, new)
                .build(),
        );
        state.set_options(ReviewOptions {
            navigation: NavigationPane::Hidden,
            footer: false,
            ..Default::default()
        });
        state.set_view_mode(ViewMode::Split);
        let area = Rect::new(0, 0, 26, 12);
        let mut buffer = Buffer::empty(area);
        DiffReviewWidget::new()
            .borders(false)
            .render(area, &mut buffer, &mut state);
        let row = (0..area.height)
            .find(|y| buffer[(present_x, *y)].symbol() == "▌")
            .ok_or("missing empty source gutter")?;
        let missing: String = (missing_x..missing_x + 12)
            .map(|x| buffer[(x, row)].symbol())
            .collect();
        assert_eq!(missing.trim(), "");
    }
    Ok(())
}

fn source_rows(buffer: &Buffer) -> Vec<String> {
    (buffer.area.top()..buffer.area.bottom())
        .filter(|y| buffer.area.width > 0 && buffer[(buffer.area.x, *y)].symbol() == "▌")
        .map(|y| {
            (buffer.area.left()..buffer.area.right() - 1)
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

fn buffer_row(buffer: &Buffer, row: u16) -> String {
    (buffer.area.left()..buffer.area.right())
        .map(|column| buffer[(column, row)].symbol())
        .collect()
}

fn render(old: &str, new: &str, tab_width: u16, width: u16) -> Buffer {
    let mut state =
        DiffReviewState::new(DocumentBuilder::new().changed("tabs.rs", old, new).build());
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
