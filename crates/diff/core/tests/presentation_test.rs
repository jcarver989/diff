#![cfg(feature = "test-support")]

use clankerdiff_core::{
    ContentProjection, DiffFileView, DiffPresentation, DiffSide, DiffTone, FilePresentation, Layout,
    MAX_CELL_TEXT_BYTES, ModeChange, PresentationOptions, PresentedCell, PresentedRow, RowKind, SourceFocus,
    SourceUnavailable, ViewMode, ViewportAnchor, testing::DocumentBuilder,
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
        let source = Some(SourceFocus { file_index: 0, preview: false });
        let presentation = DiffPresentation::new(
            document.clone(),
            PresentationOptions {
                view_mode: ViewMode::Split,
                include_file_headers: false,
                source,
                ..Default::default()
            },
        );
        assert_eq!(presentation.layout(), Layout::Unified);
        assert_eq!(presentation.file_presentation(0), FilePresentation::Source);
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
        let source = Some(SourceFocus { file_index: 0, preview: false });
        let presentation = DiffPresentation::new(
            document,
            PresentationOptions { include_file_headers: false, source, ..Default::default() },
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
    projection.set_file_view(document.files[0].path.clone(), DiffFileView::Collapsed);
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
fn collapsed_files_keep_their_patch_when_headers_are_disabled() {
    let document = DocumentBuilder::new().changed("a.rs", "old\n", "new\n").build();
    let options = PresentationOptions { include_file_headers: false, ..Default::default() };
    let mut projection = ContentProjection::default();
    projection.set_file_view(document.files[0].path.clone(), DiffFileView::Collapsed);

    let patch = DiffPresentation::new(document.clone(), options);
    let collapsed = DiffPresentation::with_projection(document, options, &projection);
    assert_eq!(collapsed.file_presentation(0), FilePresentation::Patch);
    assert_eq!(collapsed.file_range(0), patch.file_range(0));
    assert_eq!(collapsed.hunk_range(0, 0), patch.hunk_range(0, 0));
    assert_eq!(collapsed.rows(0..collapsed.row_count()), patch.rows(0..patch.row_count()));
}

#[test]
fn markdown_body_uses_complete_source_or_falls_back_to_the_requested_view() {
    use FilePresentation::{Markdown, Patch, Source};
    for (focused, available, expected) in
        [(false, true, Markdown), (true, true, Markdown), (false, false, Patch), (true, false, Source)]
    {
        let builder = DocumentBuilder::new().changed("README.md", "old", "# New");
        let document = if available {
            builder
        } else {
            builder.source("README.md", DiffSide::New, Err::<&str, _>(SourceUnavailable::NotCaptured))
        }
        .build();
        let mut projection = ContentProjection::default();
        projection.set_file_view(document.files[0].path.clone(), DiffFileView::Preview);
        let source = focused.then_some(SourceFocus { file_index: 0, preview: true });
        let options = PresentationOptions { include_file_headers: false, source, ..Default::default() };
        let presentation = DiffPresentation::with_projection(document, options, &projection);
        assert_eq!(presentation.file_presentation(0), expected);
        assert_eq!(presentation.hunk_range(0, 0).is_some(), expected == Patch);
        if expected != Patch {
            let kind = if available { RowKind::MarkdownPreview } else { RowKind::Meta };
            assert_eq!(presentation.row_count(), 1);
            assert_eq!(presentation.row(0).map(|r| r.kind), Some(kind));
        }
    }
}

#[test]
fn source_focus_projects_only_the_focused_file_with_its_original_file_index() {
    let document = DocumentBuilder::new()
        .changed("a.rs", "old a\n", "new a\n")
        .changed("b.rs", "old b\n", "line 1\nline 2\n")
        .build();
    let options =
        PresentationOptions { source: Some(SourceFocus { file_index: 1, preview: false }), ..Default::default() };
    let presentation = DiffPresentation::new(document, options);
    assert_eq!(presentation.file_range(0), Some(0..0));
    assert_eq!(presentation.file_range(1), Some(0..3));
    assert_eq!(presentation.row_count(), 3);
    assert!(presentation.rows(0..3).iter().all(|row| row.file_index == 1));
    assert_eq!(presentation.file_presentation(1), FilePresentation::Source);
}

#[test]
fn mode_only_files_show_a_placeholder_until_full_file_expansion() -> Result<(), Box<dyn Error>> {
    let text = "#!/bin/sh\necho ok\n";
    let builder = DocumentBuilder::new().changed_with("script.sh", text, text, |mut file| {
        file.mode = Some(ModeChange { old: Some("100644".into()), new: Some("100755".into()) });
        file
    });
    let document = builder.clone().build();
    let options = PresentationOptions { include_file_headers: false, ..Default::default() };

    let patch = DiffPresentation::new(document.clone(), options);
    assert_eq!(patch.file_range(0), Some(0..1));
    let placeholder = patch.row(0).ok_or("placeholder")?;
    assert_eq!(placeholder.kind, RowKind::Meta);
    assert_eq!(placeholder.primary_cell().ok_or("placeholder cell")?.text.as_ref(), "File mode changed");
    assert_eq!(patch.hunk_range(0, 0), None);

    let mut projection = ContentProjection::default();
    projection.set_full_file(document.files[0].path.clone(), true);
    let expanded = DiffPresentation::with_projection(document, options, &projection);
    assert_eq!(expanded.file_range(0), Some(0..2));
    assert_eq!(expanded.hunk_range(0, 0), None);
    for (row, line) in expanded.rows(0..expanded.row_count()).iter().zip(text.lines()) {
        assert_eq!(row.kind, RowKind::ExpandedContext);
        assert_eq!(row.primary_cell().ok_or("expanded cell")?.text.as_ref(), line);
    }
    for reason in [SourceUnavailable::Absent, SourceUnavailable::Binary] {
        let unavailable = builder
            .clone()
            .source("script.sh", DiffSide::Old, Err::<&str, _>(SourceUnavailable::Absent))
            .source("script.sh", DiffSide::New, Err::<&str, _>(reason.clone()))
            .build();
        let expanded = DiffPresentation::with_projection(unavailable, options, &projection);
        assert_eq!(expanded.gap_info(0).ok_or("unavailable gap")?.unavailable, Some(reason));
    }
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

#[test]
fn overlong_lines_show_a_bounded_prefix_and_how_much_is_hidden() -> Result<(), Box<dyn Error>> {
    let shown = "a".repeat(MAX_CELL_TEXT_BYTES - 1);
    let long = format!("{shown}界{}", "b".repeat(10_000));
    let document = DocumentBuilder::new().changed("data.json", "short\n", &format!("{long}\n")).build();
    let presentation = DiffPresentation::new(document, PresentationOptions::default());
    let rows = presentation.rows(0..presentation.row_count());
    let cells: Vec<&PresentedCell> =
        rows.iter().flat_map(PresentedRow::cells).filter(|cell| cell.patch_source.is_some()).collect();
    let short = cells.iter().find(|cell| cell.text.as_ref() == "short").ok_or("short cell")?;
    assert_eq!((short.truncated_at, short.source_text()), (None, "short"));
    let truncated = cells.iter().find(|cell| cell.truncated_at.is_some()).ok_or("truncated cell")?;
    assert_eq!(truncated.source_text(), shown);
    assert_eq!(truncated.text.as_ref(), format!("{shown} … {} more bytes", long.len() - shown.len()));
    Ok(())
}
