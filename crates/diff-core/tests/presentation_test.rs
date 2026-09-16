#![cfg(feature = "test-support")]

use clankerdiff_core::{
    ContentProjection, DiffPresentation, DiffSide, DiffTone, Layout, MAX_HUNK_SEQUENCE_LINES,
    PresentationOptions, RowKind, SourceDocument, SourceUnavailable, ViewMode,
    testing::DocumentBuilder,
};
use std::{borrow::Cow, error::Error};

#[test]
fn source_projection_is_complete_neutral_and_has_no_patch_provenance() -> Result<(), Box<dyn Error>>
{
    let text = "before\r\n\r\n+ literal\r\n- literal\r\n@@ literal\r\n界 after";
    for side in [DiffSide::Old, DiffSide::New] {
        let document = match side {
            DiffSide::Old => DocumentBuilder::new().deleted("a.rs", "old\n"),
            DiffSide::New => DocumentBuilder::new().changed("a.rs", "old\n", "new\n"),
        }
        .source("a.rs", side, Ok(text))
        .build();
        let mut projection = ContentProjection::default();
        projection.set_source_view(Some(document.files[0].path.clone()));
        let presentation = DiffPresentation::with_projection(
            document.clone(),
            PresentationOptions {
                view_mode: ViewMode::Split,
                include_file_headers: false,
                ..Default::default()
            },
            &projection,
        );
        assert_eq!(presentation.layout(), Layout::Unified);
        let source = document.files[0].source_document(side).ok_or("source")?;
        assert_eq!(presentation.row_count(), source.line_count());
        assert!(presentation.hunk_range(0, 0).is_none());
        for (offset, row) in presentation
            .rows(0..presentation.row_count())
            .iter()
            .enumerate()
        {
            assert_eq!(presentation.row_with_id(row.id), Some(offset));
            assert_eq!(row.kind, RowKind::ExpandedContext);
            assert!(row.is_navigable());
            assert!(!row.is_commentable());
            assert!(row.hunk_index.is_none());
            let cell = row.cell(side).ok_or("source cell")?;
            assert_eq!(Some(cell.text.as_ref()), source.line(offset + 1));
            assert_eq!(cell.tone, DiffTone::Context);
            assert!(cell.patch_source.is_none());
            assert!(presentation.cell_anchor(row, cell).is_none());
            assert_eq!(presentation.cell_context(row, cell).text(), text);
            let location = presentation.source_location(row, cell).ok_or("location")?;
            assert_eq!(presentation.row_showing_source(&location), Some(offset));
        }
    }
    Ok(())
}

#[test]
fn source_projection_reports_empty_and_every_unavailable_reason_without_fallback()
-> Result<(), Box<dyn Error>> {
    for result in [
        Ok(""),
        Err(SourceUnavailable::Absent),
        Err(SourceUnavailable::NotCaptured),
        Err(SourceUnavailable::Binary),
        Err(SourceUnavailable::TooLarge { bytes: 9_000_000 }),
        Err(SourceUnavailable::TooManyLines { lines: 1_000_001 }),
        Err(SourceUnavailable::SnapshotBudgetExceeded),
        Err(SourceUnavailable::UnstableSnapshot),
        Err(SourceUnavailable::Error("capture failed".into())),
    ] {
        let message = result
            .as_ref()
            .err()
            .map_or_else(|| "Empty file".to_owned(), ToString::to_string);
        let document = DocumentBuilder::new()
            .changed(
                "a.rs",
                "old source must not appear",
                "patch must not appear",
            )
            .source("a.rs", DiffSide::New, result)
            .build();
        let mut projection = ContentProjection::default();
        projection.set_source_view(Some(document.files[0].path.clone()));
        let presentation = DiffPresentation::with_projection(
            document,
            PresentationOptions {
                include_file_headers: false,
                ..Default::default()
            },
            &projection,
        );
        assert_eq!(presentation.row_count(), 1);
        let row = presentation.row(0).ok_or("state row")?;
        assert_eq!(row.kind, RowKind::Meta);
        assert_eq!(
            row.primary_cell().ok_or("state text")?.text.as_ref(),
            message
        );
    }
    Ok(())
}

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
