use clankerdiff_core::{FileDiff, ViewMode};
use clankerdiff_ratatui::{DiffPreviewOptions, DiffPreviewState};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::ReviewTheme;
use std::{error::Error, sync::Arc};

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
