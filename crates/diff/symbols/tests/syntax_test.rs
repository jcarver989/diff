use clankerdiff_core::{
    DiffPresentation, PresentationOptions, PresentedCell, PresentedRow, RowKind, join_lines, testing::DocumentBuilder,
};
use clankerdiff_symbols::{DocumentSyntax, HUNK_WINDOW_LINES};
use clankerdiff_syntax::{LanguageHint, analyze, thread_work};
use std::{error::Error, sync::Arc};

#[test]
fn cells_with_a_captured_source_are_highlighted_in_its_context() -> Result<(), Box<dyn Error>> {
    let old = "/* open\nold\nclosed */\n";
    let new = "/* open\nnew\nclosed */\n";
    let document = DocumentBuilder::new().renamed("old.rs", "new.rs", old, new).build();
    let presentation = DiffPresentation::new(Arc::clone(&document), PresentationOptions::default());
    let syntax = DocumentSyntax::new(document);
    let mut seen = 0;
    for (row, cell) in code_cells(&presentation) {
        let Some(location) = cell.source_line else {
            continue;
        };
        let file = &presentation.document().files[row.file_index];
        let source = file.source_document(location.side).ok_or("source")?;
        let expected = analyze(LanguageHint::Path(file.path_for_side(location.side).as_str()), source.text());
        assert_eq!(
            syntax.highlights(row, cell),
            expected.highlights.line_shared(location.line_number - 1).ok_or("line")?
        );
        seen += 1;
    }
    assert!(seen > 0);
    Ok(())
}

#[test]
fn each_source_is_analyzed_once_across_repeated_highlights() {
    let document = DocumentBuilder::new().changed("a.rs", "fn a() {}\n", "fn b() {}\n").build();
    let presentation = DiffPresentation::new(Arc::clone(&document), PresentationOptions::default());
    let syntax = DocumentSyntax::new(document);
    let before = thread_work().parses();
    for _ in 0..3 {
        for (row, cell) in code_cells(&presentation) {
            assert!(!syntax.highlights(row, cell).is_empty());
        }
    }
    assert_eq!(thread_work().parses() - before, 2);
}

#[test]
fn patch_only_cells_are_highlighted_within_their_hunk_window() -> Result<(), Box<dyn Error>> {
    for lines in [8, 4 * HUNK_WINDOW_LINES] {
        let document = DocumentBuilder::new().generated("src/lib.rs", lines).build();
        let presentation = DiffPresentation::new(Arc::clone(&document), PresentationOptions::default());
        let syntax = DocumentSyntax::new(document);
        let cells = code_cells(&presentation);
        assert_eq!(cells.len(), lines);
        let before = thread_work().parses();
        assert!(!syntax.highlights(cells[0].0, cells[0].1).is_empty());
        assert_eq!(thread_work().parses() - before, 1, "only the first window is analyzed");
        let highlighted: Vec<_> = cells.iter().map(|(row, cell)| syntax.highlights(row, cell)).collect();
        assert_eq!(thread_work().parses() - before, lines.div_ceil(HUNK_WINDOW_LINES));
        for (window, highlighted) in cells.chunks(HUNK_WINDOW_LINES).zip(highlighted.chunks(HUNK_WINDOW_LINES)) {
            let expected = analyze("src/lib.rs", &join_lines(window.iter().map(|(_, cell)| cell.text.as_ref())));
            for (line, spans) in highlighted.iter().enumerate() {
                assert!(!spans.is_empty());
                assert_eq!(*spans, expected.highlights.line_shared(line).ok_or("line")?);
            }
        }
    }
    Ok(())
}

#[test]
fn synthetic_cells_have_no_highlights() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new().generated("src/lib.rs", 1).build();
    let presentation = DiffPresentation::new(Arc::clone(&document), PresentationOptions::default());
    let (row, cell) = code_cells(&presentation).into_iter().next().ok_or("code cell")?;
    let mut synthetic = cell.clone();
    synthetic.patch_source = None;
    synthetic.source_line = None;
    assert!(DocumentSyntax::new(document).highlights(row, &synthetic).is_empty());
    Ok(())
}

#[test]
fn an_edited_file_keeps_the_analysis_of_its_unchanged_side() {
    let first = DocumentBuilder::new().changed("a.rs", "fn a() {}\n", "fn b() {}\n").build();
    let mut syntax = DocumentSyntax::new(first);
    highlight_every_cell(&syntax);
    let edited = DocumentBuilder::new().changed("a.rs", "fn a() {}\n", "fn c() {}\n").build();
    syntax.sync(&edited);
    let before = thread_work().parses();
    highlight_every_cell(&syntax);
    assert_eq!(thread_work().parses() - before, 1, "only the edited new side is analyzed again");
}

fn highlight_every_cell(syntax: &DocumentSyntax) {
    let presentation = DiffPresentation::new(Arc::clone(syntax.document()), PresentationOptions::default());
    for (row, cell) in code_cells(&presentation) {
        let _ = syntax.highlights(row, cell);
    }
}

fn code_cells(presentation: &DiffPresentation) -> Vec<(&PresentedRow, &PresentedCell)> {
    presentation
        .rows(0..presentation.row_count())
        .iter()
        .filter(|row| row.kind == RowKind::Code)
        .flat_map(|row| row.cells().map(move |cell| (row, cell)))
        .collect()
}
