#![cfg(feature = "test-support")]

use clankerdiff_core::{
    ContentProjection, DiffPresentation, DiffSide, DiffTone, Layout, PresentationOptions, RowKind, SourceUnavailable,
    ViewMode, ViewportAnchor, testing::DocumentBuilder,
};
use std::error::Error;

#[test]
fn source_projection_is_complete_neutral_and_has_no_patch_provenance() -> Result<(), Box<dyn Error>> {
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
            PresentationOptions { view_mode: ViewMode::Split, include_file_headers: false, ..Default::default() },
            &projection,
        );
        assert_eq!(presentation.layout(), Layout::Unified);
        let source = document.files[0].source_document(side).ok_or("source")?;
        assert_eq!(presentation.row_count(), source.line_count());
        assert!(presentation.hunk_range(0, 0).is_none());
        for (offset, row) in presentation.rows(0..presentation.row_count()).iter().enumerate() {
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
            let location = presentation.source_location(row, cell).ok_or("location")?;
            assert_eq!(presentation.row_showing_source(&location), Some(offset));
        }
    }
    Ok(())
}

#[test]
fn source_projection_reports_empty_and_every_unavailable_reason_without_fallback() -> Result<(), Box<dyn Error>> {
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
        let message = result.as_ref().err().map_or_else(|| "Empty file".to_owned(), ToString::to_string);
        let document = DocumentBuilder::new()
            .changed("a.rs", "old source must not appear", "patch must not appear")
            .source("a.rs", DiffSide::New, result)
            .build();
        let mut projection = ContentProjection::default();
        projection.set_source_view(Some(document.files[0].path.clone()));
        let presentation = DiffPresentation::with_projection(
            document,
            PresentationOptions { include_file_headers: false, ..Default::default() },
            &projection,
        );
        assert_eq!(presentation.row_count(), 1);
        let row = presentation.row(0).ok_or("state row")?;
        assert_eq!(row.kind, RowKind::Meta);
        assert_eq!(row.primary_cell().ok_or("state text")?.text.as_ref(), message);
    }
    Ok(())
}

#[test]
#[allow(clippy::format_collect)]
fn unchanged_gaps_separate_hunks_and_carry_function_context() -> Result<(), Box<dyn Error>> {
    let old = (1..=60).map(|line| format!("line {line}\n")).collect::<String>();
    let new = old.replace("line 10\n", "changed 10\n").replace("line 50\n", "changed 50\n");
    let presentation = DiffPresentation::new(
        DocumentBuilder::new()
            .changed_with("a.rs", &old, &new, |mut file| {
                file.hunks[1].function_context = Some("fn later()".to_owned());
                file
            })
            .build(),
        PresentationOptions { view_mode: ViewMode::Unified, ..Default::default() },
    );
    let gaps = (0..presentation.row_count())
        .filter_map(|index| presentation.gap_info(index).map(|info| (index, info)))
        .collect::<Vec<_>>();
    assert_eq!(gaps.iter().map(|(_, info)| info.id.gap_index).collect::<Vec<_>>(), [0, 1, 2]);
    let (between, info) = gaps[1];
    assert_eq!(info.hidden_lines, 33);
    assert_eq!(info.message(), "⋯ 33 unchanged lines · fn later()");
    let row = presentation.row(between).ok_or("gap row")?;
    assert_eq!(row.kind, RowKind::ExpandGap);
    assert_eq!(row.primary_cell().map(|cell| cell.text.as_ref()), Some(info.message().as_str()));
    let hunk = presentation.hunk_range(0, 1).ok_or("second hunk")?;
    assert_eq!(hunk.start, between + 1);
    assert_eq!(presentation.row(hunk.start).map(|row| row.kind), Some(RowKind::Code));
    Ok(())
}

#[test]
fn viewport_anchors_fall_back_to_their_file_start() -> Result<(), Box<dyn Error>> {
    let presentation = DiffPresentation::new(
        DocumentBuilder::new().changed("a.rs", "old\n", "new\n").build(),
        PresentationOptions::default(),
    );
    let all = 0..presentation.row_count();
    let row = presentation.last_navigable(all.clone()).ok_or("row")?;
    let anchor = ViewportAnchor::capture(&presentation, row).ok_or("anchor")?;
    assert_eq!(anchor.resolve(&presentation, all), Some(row));
    assert_eq!(anchor.resolve(&presentation, 0..row), None);
    let changed = DiffPresentation::new(DocumentBuilder::new().binary("a.rs").build(), PresentationOptions::default());
    assert_eq!(anchor.resolve(&changed, 0..changed.row_count()), Some(0));
    let removed = DiffPresentation::new(
        DocumentBuilder::new().changed("z.rs", "a\n", "b\n").build(),
        PresentationOptions::default(),
    );
    assert_eq!(anchor.resolve(&removed, 0..removed.row_count()), None);
    Ok(())
}

#[test]
fn collapsed_files_project_only_a_navigable_header() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new().changed("a.rs", "old\n", "new\n").changed("b.rs", "old\n", "new\n").build();
    let mut projection = ContentProjection::default();
    projection.set_collapsed(document.files[0].path.clone(), true);
    let presentation = DiffPresentation::with_projection(document, PresentationOptions::default(), &projection);
    let collapsed = presentation.file_range(0).ok_or("collapsed range")?;
    assert_eq!(collapsed.len(), 1);
    assert_eq!(presentation.row(collapsed.start).ok_or("header")?.kind, RowKind::FileHeader);
    assert!(presentation.is_collapsed(0));
    assert!(presentation.is_navigable(collapsed.start));
    assert_eq!(presentation.hunk_range(0, 0), None);
    let expanded = presentation.file_range(1).ok_or("expanded range")?;
    assert!(!presentation.is_collapsed(1));
    assert!(!presentation.is_navigable(expanded.start));
    assert!(expanded.len() > 1);
    Ok(())
}

#[test]
fn split_presentations_show_one_sided_files_unified() {
    let document = DocumentBuilder::new()
        .changed("changed.rs", "old\n", "new\n")
        .added("added.rs", "new\n")
        .untracked("untracked.rs", "new\n")
        .deleted("deleted.rs", "old\n")
        .build();
    let options = PresentationOptions { view_mode: ViewMode::Split, ..Default::default() };
    let presentation = DiffPresentation::new(document, options);
    let layouts: Vec<Layout> = (0..4).map(|index| presentation.file_layout(index)).collect();
    assert_eq!(layouts, [Layout::Split, Layout::Unified, Layout::Unified, Layout::Unified]);
}
