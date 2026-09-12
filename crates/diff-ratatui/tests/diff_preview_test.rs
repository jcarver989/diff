use clankerdiff_core::{FileDiff, ViewMode};
use clankerdiff_ratatui::{DiffPreviewOptions, DiffPreviewState, page_color, render_diff_preview};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::{DiffTone, ReviewTheme};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Paragraph, Widget},
};
use std::{error::Error, sync::Arc};

#[test]
fn split_additions_fill_missing_left_side_and_empty_source_lines() -> Result<(), Box<dyn Error>> {
    assert_split_backgrounds(
        "",
        "let added = 1;\n\n",
        &[(None, Some(DiffTone::Added)); 2],
    )
}

#[test]
fn split_deletions_fill_missing_right_side_and_empty_source_lines() -> Result<(), Box<dyn Error>> {
    assert_split_backgrounds(
        "let removed = 1;\n\n",
        "",
        &[(Some(DiffTone::Removed), None); 2],
    )
}

#[test]
fn split_replacements_fill_unequal_sides() -> Result<(), Box<dyn Error>> {
    let short = "let value = 1;\n";
    let long = "let value = 2;\nlet extra = 3;\n";
    assert_split_backgrounds(
        short,
        long,
        &[
            (Some(DiffTone::Removed), Some(DiffTone::Added)),
            (None, Some(DiffTone::Added)),
        ],
    )?;
    assert_split_backgrounds(
        long,
        short,
        &[
            (Some(DiffTone::Removed), Some(DiffTone::Added)),
            (Some(DiffTone::Removed), None),
        ],
    )
}

#[test]
fn split_divider_does_not_inherit_parent_background() -> Result<(), Box<dyn Error>> {
    assert_split_backgrounds(
        "let value = 1;\n",
        "let value = 2;\n",
        &[(Some(DiffTone::Removed), Some(DiffTone::Added))],
    )
}

#[test]
fn tabs_use_source_relative_stops_and_invalidate_cached_rows() -> Result<(), Box<dyn Error>> {
    assert_eq!(DiffPreviewOptions::default().tab_width, 2);
    let mut state = DiffPreviewState::new(FileDiff::from_texts(
        "tabs.rs",
        "",
        "\tlet a = 1;\na\tb\n界\tx\n    spaces\n",
    )?);
    let theme = ReviewTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    for width in [80, 120] {
        let mut previous = None;
        for (tab_width, indent, middle, wide) in [
            (2, "  ", "a b", "界  x"),
            (4, "    ", "a   b", "界  x"),
            (0, " ", "a b", "界 x"),
        ] {
            let options = DiffPreviewOptions {
                tab_width,
                include_hunk_headers: false,
                ..Default::default()
            };
            let rows = state.render(width, &theme, &mut highlighter, options);
            let rendered = text(&rows);
            for expected in [
                format!("+ {indent}let a = 1;"),
                format!("+ {middle}"),
                format!("+ {wide}"),
                "+     spaces".to_owned(),
            ] {
                assert!(
                    rendered.iter().any(|line| line.contains(&expected)),
                    "{expected:?}: {rendered:?}"
                );
            }
            assert!(rows.iter().all(|row| row.width() <= usize::from(width)));
            if let Some(previous) = previous {
                assert!(!Arc::ptr_eq(&previous, &rows));
            }
            let cached = state.render(width, &theme, &mut highlighter, options);
            assert!(Arc::ptr_eq(&rows, &cached));
            previous = Some(rows);
        }
    }
    Ok(())
}

#[test]
fn expanded_tabs_preserve_syntax_styles() -> Result<(), Box<dyn Error>> {
    let theme = ReviewTheme::default();
    let options = DiffPreviewOptions::default();
    let mut highlighter = SyntaxHighlighter::default();
    let mut actual = DiffPreviewState::new(FileDiff::from_texts("tabs.rs", "", "\tlet a = 1;\n")?);
    let mut expected =
        DiffPreviewState::new(FileDiff::from_texts("tabs.rs", "", "  let a = 1;\n")?);
    assert_eq!(
        actual.render(80, &theme, &mut highlighter, options),
        expected.render(80, &theme, &mut highlighter, options),
    );
    Ok(())
}

#[test]
fn previews_reuse_rows_and_invalidate_replaced_files() -> Result<(), Box<dyn Error>> {
    let file = FileDiff::from_texts("src/main.rs", "old\n", "new\n")?;
    let mut state = DiffPreviewState::new(file);
    let mut highlighter = SyntaxHighlighter::default();
    let theme = ReviewTheme::default();
    let options = DiffPreviewOptions::default();
    let first = state.render(80, &theme, &mut highlighter, options);
    let unchanged = state.render(80, &theme, &mut highlighter, options);
    assert!(Arc::ptr_eq(&first, &unchanged));
    assert_eq!(state.take_stats().cache_hits, 1);
    state.set_file(FileDiff::from_texts(
        "src/main.rs",
        "new\n",
        "replacement\n",
    )?);
    let replaced = state.render(80, &theme, &mut highlighter, options);
    assert!(!Arc::ptr_eq(&first, &replaced));
    assert!(
        replaced
            .iter()
            .any(|row| row.to_string().contains("replacement"))
    );
    Ok(())
}

#[test]
fn theme_changes_recolor_previews_without_parsing() -> Result<(), Box<dyn Error>> {
    let mut state = DiffPreviewState::new(FileDiff::from_texts(
        "src/main.rs",
        "let before = 1;\n",
        "let after = 2;\n",
    )?);
    let mut highlighter = SyntaxHighlighter::default();
    let options = DiffPreviewOptions::default();
    let first = state.render(80, &ReviewTheme::default(), &mut highlighter, options);
    assert!(highlighter.take_stats().bytes > 0);
    let changed = state.render(80, &ReviewTheme::ayu()?, &mut highlighter, options);
    assert_ne!(first, changed);
    let work = highlighter.take_stats();
    assert_eq!(work.bytes, 0);
    assert_eq!(work.misses, 0);
    assert!(work.hits > 0);
    Ok(())
}

#[test]
fn preview_rows_remain_bounded_at_tiny_and_split_widths() -> Result<(), Box<dyn Error>> {
    let mut state = DiffPreviewState::new(FileDiff::from_texts(
        "wide.rs",
        "界界界\te\u{301}\n",
        "changed\n",
    )?);
    for (width, expected_rows) in [
        (0, 0),
        (1, 2),
        (2, 2),
        (5, 2),
        (10, 9),
        (95, 2),
        (96, 1),
        (120, 1),
    ] {
        let rows = state.render(
            width,
            &ReviewTheme::default(),
            &mut SyntaxHighlighter::default(),
            DiffPreviewOptions {
                view_mode: ViewMode::Auto,
                include_hunk_headers: false,
                ..DiffPreviewOptions::default()
            },
        );
        assert!(
            rows.iter().all(|row| row.width() <= usize::from(width)),
            "width: {width}"
        );
        assert_eq!(rows.len(), expected_rows, "width: {width}");
    }
    Ok(())
}

#[test]
fn unified_previews_wrap_source_rows_and_preserve_gutters() -> Result<(), Box<dyn Error>> {
    let file = FileDiff::from_texts(
        "example.txt",
        "abcdefghij\ncontext123\n",
        "ABCDEFGHIJ\ncontext123\n",
    )?;
    let rows = render_diff_preview(
        file,
        13,
        &ReviewTheme::default(),
        &mut SyntaxHighlighter::default(),
        DiffPreviewOptions {
            view_mode: ViewMode::Unified,
            include_hunk_headers: false,
            ..Default::default()
        },
    );
    assert_eq!(
        text(&rows),
        [
            "▌   1 - abcde",
            "▌   ↪ - fghij",
            "▌   1 + ABCDE",
            "▌   ↪ + FGHIJ",
            "    2   conte",
            "    ↪   xt123",
        ]
    );
    assert!(rows.iter().all(|row| row.width() == 13));
    Ok(())
}

#[test]
fn split_previews_align_continuations_before_the_next_source_row() -> Result<(), Box<dyn Error>> {
    for (before, after, expected) in [
        (
            "abcdefghij\nsame\n",
            "XYZ\nsame\n",
            vec![
                "▌   1 - abcde│▌   1 + XYZ  ",
                "▌   ↪ - fghij│             ",
                "    2   same │    2   same ",
            ],
        ),
        (
            "XYZ\nsame\n",
            "abcdefghij\nsame\n",
            vec![
                "▌   1 - XYZ  │▌   1 + abcde",
                "             │▌   ↪ + fghij",
                "    2   same │    2   same ",
            ],
        ),
        (
            "abcdefghij\nsame\n",
            "ABCDEFGHIJKLM\nsame\n",
            vec![
                "▌   1 - abcde│▌   1 + ABCDE",
                "▌   ↪ - fghij│▌   ↪ + FGHIJ",
                "             │▌   ↪ + KLM  ",
                "    2   same │    2   same ",
            ],
        ),
    ] {
        let rows = render_diff_preview(
            FileDiff::from_texts("example.txt", before, after)?,
            27,
            &ReviewTheme::default(),
            &mut SyntaxHighlighter::default(),
            DiffPreviewOptions {
                view_mode: ViewMode::Split,
                include_hunk_headers: false,
                ..Default::default()
            },
        );
        assert_eq!(text(&rows), expected);
        assert!(rows.iter().all(|row| row.width() == 27));
    }
    Ok(())
}

#[test]
fn preview_budget_counts_visual_rows_and_reports_partial_source_rows() -> Result<(), Box<dyn Error>>
{
    for (view_mode, width) in [(ViewMode::Unified, 24), (ViewMode::Split, 49)] {
        let file = FileDiff::from_texts(
            "example.txt",
            "",
            &format!("{}\nnext\n", "x".repeat(100_000)),
        )?;
        let mut state = DiffPreviewState::new(file);
        for max_content_rows in [0, 1, 2, 20] {
            for overflow_summary in [false, true] {
                let mut highlighter = SyntaxHighlighter::default();
                let rows = state.render(
                    width,
                    &ReviewTheme::default(),
                    &mut highlighter,
                    DiffPreviewOptions {
                        max_content_rows,
                        view_mode,
                        include_hunk_headers: false,
                        overflow_summary,
                        ..Default::default()
                    },
                );
                if max_content_rows == 0 {
                    assert_eq!(highlighter.take_stats().bytes, 0);
                }
                assert_eq!(rows.len(), max_content_rows + usize::from(overflow_summary));
                assert!(rows.iter().all(|row| row.width() == usize::from(width)));
                if overflow_summary {
                    assert!(
                        rows.last()
                            .is_some_and(|row| row.to_string().starts_with("… 2 more rows"))
                    );
                }
                assert!(rows.iter().all(|row| !row.to_string().contains("next")));
            }
        }
    }
    Ok(())
}

#[test]
fn exact_preview_budget_does_not_report_overflow() -> Result<(), Box<dyn Error>> {
    for (after, budget, count) in [
        ("abcde\n", 1, 1),
        ("abcdef\n", 2, 2),
        ("\n", 1, 1),
        ("abcdef\n", usize::MAX, 2),
        ("abcde\nnext\n", 1, 2),
    ] {
        let rows = render_diff_preview(
            FileDiff::from_texts("example.txt", "", after)?,
            13,
            &ReviewTheme::default(),
            &mut SyntaxHighlighter::default(),
            DiffPreviewOptions {
                max_content_rows: budget,
                view_mode: ViewMode::Unified,
                include_hunk_headers: false,
                ..Default::default()
            },
        );
        assert_eq!(rows.len(), count);
        if count <= budget {
            assert!(rows.iter().all(|row| !row.to_string().contains('…')));
        }
    }
    Ok(())
}

#[test]
fn wrapped_tabs_unicode_and_styles_match_expanded_source() -> Result<(), Box<dyn Error>> {
    let theme = ReviewTheme::default();
    let options = DiffPreviewOptions {
        view_mode: ViewMode::Unified,
        include_hunk_headers: false,
        tab_width: 4,
        ..Default::default()
    };
    let mut actual = DiffPreviewState::new(FileDiff::from_texts(
        "example.rs",
        "",
        "\tlet a = \"界e\u{301}👩‍💻\";  \n",
    )?);
    let mut expanded = DiffPreviewState::new(FileDiff::from_texts(
        "example.rs",
        "",
        "    let a = \"界e\u{301}👩‍💻\";  \n",
    )?);
    let mut highlighter = SyntaxHighlighter::default();
    for width in [11, 13, 80, 13] {
        let rows = actual.render(width, &theme, &mut highlighter, options);
        assert_eq!(
            rows,
            expanded.render(width, &theme, &mut highlighter, options)
        );
        assert!(rows.iter().all(|row| row.width() == usize::from(width)));
        assert!(rows.iter().any(|row| row.to_string().contains("e\u{301}")));
        assert!(rows.iter().any(|row| row.to_string().contains("👩‍💻")));
        let cached = actual.render(width, &theme, &mut highlighter, options);
        assert!(Arc::ptr_eq(&rows, &cached));
    }
    Ok(())
}

#[test]
fn split_wrapped_rows_keep_diff_backgrounds_on_padding() -> Result<(), Box<dyn Error>> {
    let long = format!("{}\n", "x".repeat(110));
    assert_split_backgrounds("", &long, &[(None, Some(DiffTone::Added)); 3])?;
    assert_split_backgrounds(&long, "", &[(Some(DiffTone::Removed), None); 3])?;
    assert_split_backgrounds(
        "short\n",
        &long,
        &[(Some(DiffTone::Removed), Some(DiffTone::Added)); 3],
    )?;
    assert_split_backgrounds(
        &long,
        "short\n",
        &[(Some(DiffTone::Removed), Some(DiffTone::Added)); 3],
    )
}

fn text(rows: &[Line<'_>]) -> Vec<String> {
    rows.iter().map(ToString::to_string).collect()
}

fn assert_split_backgrounds(
    before: &str,
    after: &str,
    expected: &[(Option<DiffTone>, Option<DiffTone>)],
) -> Result<(), Box<dyn Error>> {
    let mut state = DiffPreviewState::new(FileDiff::from_texts("example.rs", before, after)?);
    let mut highlighter = SyntaxHighlighter::default();
    let options = DiffPreviewOptions {
        view_mode: ViewMode::Split,
        include_hunk_headers: false,
        overflow_summary: false,
        ..Default::default()
    };
    for width in [96, 97, 120, 121] {
        for theme in [ReviewTheme::default(), ReviewTheme::ayu()?] {
            let rows = state.render(width, &theme, &mut highlighter, options);
            assert_eq!(rows.len(), expected.len());
            let area = Rect::new(0, 0, width, u16::try_from(expected.len())?);
            let mut buffer = Buffer::empty(area);
            buffer.set_style(area, Style::new().bg(Color::Magenta));
            Paragraph::new(rows.to_vec()).render(area, &mut buffer);
            let divider = (width - 1) / 2;
            let background = page_color(&theme, theme.diff.background);
            for (y, &(left, right)) in expected.iter().enumerate() {
                let y = u16::try_from(y)?;
                for (range, tone) in [(0..divider, left), (divider + 1..width, right)] {
                    let expected_bg = tone.map_or(background, |tone| {
                        page_color(&theme, theme.diff.tone(tone).background)
                    });
                    for x in range {
                        assert_eq!(
                            buffer[(x, y)].bg,
                            expected_bg,
                            "width {width}, cell ({x}, {y}), tone {tone:?}"
                        );
                        if tone.is_none() {
                            assert_eq!(buffer[(x, y)].symbol(), " ");
                        }
                    }
                }
                assert_eq!(buffer[(divider, y)].symbol(), "│");
                assert_eq!(buffer[(divider, y)].bg, background);
                assert_eq!(
                    buffer[(divider, y)].fg,
                    page_color(&theme, theme.diff.border)
                );
            }
        }
    }
    Ok(())
}
