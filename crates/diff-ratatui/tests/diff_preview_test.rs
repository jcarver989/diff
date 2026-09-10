use clankerdiff_core::{FileDiff, ViewMode};
use clankerdiff_ratatui::{DiffPreviewOptions, DiffPreviewState};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::ReviewTheme;
use std::{error::Error, sync::Arc};

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
            let rendered: Vec<_> = rows.iter().map(ToString::to_string).collect();
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
    for width in [0, 1, 2, 5, 10, 95, 96, 120] {
        let rows = state.render(
            width,
            &ReviewTheme::default(),
            &mut SyntaxHighlighter::default(),
            DiffPreviewOptions {
                view_mode: ViewMode::Auto,
                ..DiffPreviewOptions::default()
            },
        );
        assert!(
            rows.iter().all(|row| row.width() <= usize::from(width)),
            "width: {width}"
        );
    }
    Ok(())
}
