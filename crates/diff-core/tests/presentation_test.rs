#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DiffPresentation, DiffSide, MAX_HUNK_SEQUENCE_LINES, PresentationOptions, RowKind,
    SourceDocument, testing::DocumentBuilder,
};
use std::{borrow::Cow, error::Error};

#[test]
fn cell_context_prefers_complete_source_and_side_specific_paths() {
    let old = "/* open\nold\nclosed */\n";
    let new = "/* open\nnew\nclosed */\n";
    let presentation = DiffPresentation::new(
        DocumentBuilder::new()
            .renamed("old.rs", "new.rs", old, new)
            .build(),
        PresentationOptions::default(),
    );
    let mut seen = 0;
    for row in (0..presentation.row_count()).filter_map(|index| presentation.row(index)) {
        for cell in row.cells() {
            let Some(location) = cell.source_line else {
                continue;
            };
            let context = presentation.cell_context(row, cell);
            let (path, source) = match location.side {
                DiffSide::Old => ("old.rs", old),
                DiffSide::New => ("new.rs", new),
            };
            assert_eq!(context.path, path);
            assert_eq!(context.target_line, location.line_number - 1);
            assert_eq!(context.text(), source);
            assert!(matches!(context.text(), Cow::Borrowed(_)));
            assert_eq!(
                context.text().lines().nth(context.target_line),
                Some(cell.text.as_ref())
            );
            seen += 1;
        }
    }
    assert!(seen > 0);
}

#[test]
fn patch_only_cells_share_their_bounded_hunk_context() -> Result<(), Box<dyn Error>> {
    let presentation = DiffPresentation::new(
        DocumentBuilder::new().generated("src/lib.rs", 8).build(),
        PresentationOptions::default(),
    );
    let mut previous = None;
    for row in (0..presentation.row_count()).filter_map(|index| presentation.row(index)) {
        if row.kind != RowKind::Code {
            continue;
        }
        let cell = row.primary_cell().ok_or("missing code cell")?;
        let context = presentation.cell_context(row, cell);
        assert_eq!(context.path, "src/lib.rs");
        assert_eq!(context.text().lines().count(), 8);
        assert!(matches!(context.text(), Cow::Owned(_)));
        assert_eq!(
            context.text().lines().nth(context.target_line),
            Some(cell.text.as_ref())
        );
        if let Some(id) = previous {
            assert_eq!(context.id, id);
        }
        previous = Some(context.id);
    }
    assert!(previous.is_some());
    Ok(())
}

#[test]
fn context_identity_distinguishes_exact_source_line_endings() -> Result<(), Box<dyn Error>> {
    let mut ids = Vec::new();
    for text in ["let x = 1;", "let x = 1;\n", "let x = 1;\r\n"] {
        let source = SourceDocument::new(text)?;
        let presentation = DiffPresentation::new(
            DocumentBuilder::new()
                .changed("src/lib.rs", "", text)
                .build(),
            PresentationOptions::default(),
        );
        let row = (0..presentation.row_count())
            .filter_map(|index| presentation.row(index))
            .find(|row| row.kind == RowKind::Code)
            .ok_or("missing code row")?;
        let cell = row.primary_cell().ok_or("missing code cell")?;
        let context = presentation.cell_context(row, cell);
        assert_eq!(context.text(), text);
        assert_eq!(context.id, source.content_id());
        assert!(!ids.contains(&context.id));
        ids.push(context.id);
    }
    Ok(())
}

#[test]
fn hunk_and_cell_contexts_have_distinct_identities() -> Result<(), Box<dyn Error>> {
    let presentation = DiffPresentation::new(
        DocumentBuilder::new().generated("src/lib.rs", 1).build(),
        PresentationOptions::default(),
    );
    let row = (0..presentation.row_count())
        .filter_map(|index| presentation.row(index))
        .find(|row| row.kind == RowKind::Code)
        .ok_or("missing code row")?;
    let cell = row.primary_cell().ok_or("missing code cell")?;
    let hunk = presentation.cell_context(row, cell);
    let mut synthetic = cell.clone();
    synthetic.patch_source = None;
    synthetic.source_line = None;
    let fallback = presentation.cell_context(row, &synthetic);
    assert_eq!(hunk.text(), format!("{}\n", fallback.text()));
    assert_ne!(hunk.id, fallback.id);
    assert_eq!(hunk.id, presentation.cell_context(row, cell).id);
    Ok(())
}

#[test]
fn oversized_hunks_and_synthetic_cells_use_only_the_cell_text() {
    let presentation = DiffPresentation::new(
        DocumentBuilder::new()
            .generated("src/lib.rs", MAX_HUNK_SEQUENCE_LINES + 1)
            .build(),
        PresentationOptions::default(),
    );
    for row in (0..presentation.row_count()).filter_map(|index| presentation.row(index)) {
        for cell in row.cells() {
            let context = presentation.cell_context(row, cell);
            assert_eq!(context.target_line, 0);
            assert_eq!(context.text(), cell.text.as_ref());
            assert!(matches!(context.text(), Cow::Borrowed(_)));
        }
    }
}
