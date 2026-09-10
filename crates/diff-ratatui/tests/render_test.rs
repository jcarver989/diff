use clankerdiff_core::testing::DocumentBuilder;
use clankerdiff_ratatui::{DiffReviewState, DiffReviewWidget, NavigationPane, ReviewOptions};
use ratatui::{
    buffer::{Buffer, Cell},
    layout::Rect,
    widgets::StatefulWidget,
};

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
