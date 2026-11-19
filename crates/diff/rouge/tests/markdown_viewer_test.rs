use clankerdiff_core::ReviewCommand;
use clankerdiff_markdown::{
    MarkdownDocument, MarkdownFocusPane, MarkdownReviewCommand, MarkdownReviewEvent, MarkdownReviewSession,
    MarkdownTargetId,
};
use clankerdiff_rouge::{MarkdownController, MarkdownHandle, MarkdownOutput, markdown_review_view};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{
    ImeEvent, Styled, div,
    testing::{TestError, TestUi, TestUiBuilder},
};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::tokens::{GRID_COLUMNS, UNIT};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[test]
fn every_named_part_of_the_markdown_review_is_a_whole_number_of_units_tall() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let (mut ui, handle) =
        TestUiBuilder::default().size(1200.0, 900.0).fonts(rouge_widgets::bundled_fonts()).build_with(|| {
            let doc = Arc::new(MarkdownDocument::parse(
                "# Title\n\nA paragraph.\n\n## Section\n\n- one\n- two\n\n> quoted\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\n```rust\nfn main() {}\n```",
            ));
            let handle = MarkdownController::new(doc, ReviewTheme::default()).mount(|_| {});
            (markdown_review_view(handle), handle)
        });
    ui.advance(0.0);
    let mut astray = unit_strays(&ui, "the document");
    assert!(handle.handle_command(ReviewCommand::BeginComment)?);
    ui.advance(0.0);
    astray.extend(unit_strays(&ui, "a comment draft"));
    assert!(astray.is_empty(), "off the unit: {astray:#?}");
    Ok(())
}

#[test]
fn the_reading_column_is_centred_on_whole_grid_columns_however_wide_the_window()
-> Result<(), Box<dyn std::error::Error>> {
    for width in [920.0, 1400.0, 2400.0] {
        let _runtime = Runtime::new().enter();
        let mut ui = TestUiBuilder::default().size(width, 640.0).fonts(rouge_widgets::bundled_fonts()).build(|| {
            let doc = Arc::new(MarkdownDocument::parse("# Title\n\nA paragraph."));
            markdown_review_view(MarkdownController::new(doc, ReviewTheme::default()).mount(|_| {}))
        });
        ui.advance(0.0);
        let document = ui.bounds_of("markdown-document").ok_or("document")?;
        let reading = ui.bounds_of("markdown-column-1").ok_or("column")?;
        let column = document.size.width / f32::from(GRID_COLUMNS);
        let (before, spanned) = ((reading.origin.x - document.origin.x) / column, reading.size.width / column);
        assert!((before - before.round()).abs() < 0.01 && (spanned - spanned.round()).abs() < 0.01, "at {width}");
        assert!((before * 2.0 + spanned - f32::from(GRID_COLUMNS)).abs() < 0.01, "centred at {width}");
        ui.hover_on("markdown-column-1")?;
        ui.click_on("markdown-add-comment-1")?;
        ui.advance(0.0);
        assert!(ui.bounds_of("comment-editor").is_some(), "the gutter beside the column still adds a comment");
    }
    Ok(())
}

#[test]
fn mounted_markdown_uses_semantic_selection_and_single_review_session() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let output = Rc::new(RefCell::new(Vec::new()));
    let (mut ui, handle) =
        TestUiBuilder::default().size(920.0, 640.0).fonts(rouge_widgets::bundled_fonts()).build_with(|| {
            let doc = Arc::new(MarkdownDocument::parse(
                "# Title\n\nA **paragraph** with `code`.\n\n```rust\nfn main() {}\n```",
            ));
            let sink = output.clone();
            let handle =
                MarkdownController::new(doc, ReviewTheme::default()).mount(move |event| sink.borrow_mut().push(event));
            (markdown_review_view(handle), handle)
        });
    ui.advance(0.0);
    ui.center_of("markdown-row-0")?;
    assert!(handle.handle_command(ReviewCommand::BeginComment)?);
    ui.advance(0.0);
    ui.center_of("comment-editor")?;
    ui.ime(ImeEvent::Preedit { text: "あ".into(), cursor: Some((3, 3)) });
    ui.press("enter");
    assert!(handle.with(|s| s.draft().is_some()));
    ui.ime(ImeEvent::Commit("あ".into()));
    ui.press("enter");
    assert_eq!(handle.with(|s| s.review().comments()[0].body.clone()), "あ");
    handle.set_document(Arc::new(MarkdownDocument::parse(
        "\n# Title\n\nA **paragraph** with `code`.\n\n```rust\nfn main() {}\n```",
    )));
    ui.advance(0.0);
    assert_eq!(handle.with(|s| s.review().len()), 1);
    assert!(handle.handle_command(MarkdownReviewCommand::RequestChanges)?);
    assert!(matches!(output.borrow().as_slice(), [MarkdownOutput::Event(MarkdownReviewEvent::Submit(_))]));
    Ok(())
}

#[test]
fn markdown_edits_stay_inside_the_existing_comment_card() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let (mut ui, handle) = mount("# Title\n");
    assert!(handle.handle_command(ReviewCommand::BeginComment)?);
    ui.type_text("original comment");
    ui.press("enter");
    let id = handle.with(|s| s.review().comments()[0].id);
    let card_id = format!("markdown-comment-{id}");
    let original = ui.rect_of(&card_id)?;

    ui.click_on(&format!("Edit-comment-{id}"))?;
    let card = ui.rect_of(&card_id)?;
    let editor = ui.rect_of("comment-editor")?;
    assert_eq!(card.origin, original.origin);
    assert!(card.contains(editor.origin) && card.contains(editor.max()), "{editor:?} outside {card:?}");
    ui.press("cmd-a");
    ui.type_text("edited comment");
    ui.click_on("save-comment")?;

    assert!(ui.bounds_of("comment-editor").is_none());
    assert!(ui.text_bounds("edited comment").is_some());
    assert_eq!(handle.with(|s| s.review().comments()[0].id), id);
    assert_eq!(handle.with(|s| s.review().len()), 1);
    Ok(())
}

#[test]
fn outline_lists_every_heading_that_fits_the_window() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let (mut ui, _handle) = TestUiBuilder::default().size(920.0, 640.0).build_with(|| {
        let doc = Arc::new(MarkdownDocument::parse("# One\n\n## Two\n\n## Three\n\n## Four\n\n## Five\n\n## Six\n"));
        let handle = MarkdownController::new(doc, ReviewTheme::default()).mount(|_| {});
        (markdown_review_view(handle), handle)
    });
    ui.advance(0.0);
    ui.center_of("markdown-heading-5")?;
    Ok(())
}

#[test]
fn host_navigation_commands_move_the_selection_outside_the_view_scope() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let (mut ui, handle) = mount("# One\n\nFirst.\n\nSecond.\n\n## Two\n\nThird.\n");
    ui.advance(0.0);
    assert!(handle.handle_command(MarkdownReviewCommand::MoveSelection(1))?);
    assert_eq!(handle.with(|s| s.selected_target().map(MarkdownTargetId::index)), Some(1));
    assert!(handle.handle_command(MarkdownReviewCommand::Page(1))?);
    ui.advance(0.0);
    assert_eq!(handle.with(|s| s.selected_target().map(MarkdownTargetId::index)), Some(4));
    Ok(())
}

#[test]
fn outline_focus_moves_the_selection_between_headings() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let (mut ui, handle) = mount("# One\n\nFirst.\n\n## Two\n\nSecond.\n\n## Three\n");
    ui.advance(0.0);
    let headings = handle.with(|s| s.outline().iter().map(|h| h.target_id).collect::<Vec<_>>());
    assert!(handle.handle_command(MarkdownReviewCommand::Focus(MarkdownFocusPane::Outline))?);
    assert!(handle.handle_command(MarkdownReviewCommand::MoveSelection(1))?);
    assert_eq!(handle.with(MarkdownReviewSession::selected_target), headings.get(1).copied());
    assert!(handle.handle_command(MarkdownReviewCommand::Last)?);
    assert_eq!(handle.with(MarkdownReviewSession::selected_target), headings.last().copied());
    Ok(())
}

#[test]
fn wrapped_markdown_rows_make_room_for_following_blocks_after_resize() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let (mut ui, width) =
        TestUiBuilder::default().size(920.0, 800.0).fonts(rouge_widgets::bundled_fonts()).build_with(|| {
            let width = signal(920.0);
            let doc = Arc::new(MarkdownDocument::parse(concat!(
                "# Title\n\n",
                "This paragraph is long enough to wrap onto several lines in a narrow preview, ",
                "so the next heading must start below the last line of this paragraph.\n\n",
                "## Next heading\n\n",
                "- This list item contains `inline_code` and enough words to wrap onto several ",
                "lines before the preview reaches the next item in the list.\n",
                "- Next item\n",
            )));
            let handle = MarkdownController::new(doc, ReviewTheme::default()).mount(|_| {});
            let preview = div().w(width).h_full().child(markdown_review_view(handle));
            (div().size_full().child(preview), width)
        });

    let wide = wrapped_row_heights(&mut ui)?;
    width.set(480.0);
    let narrow = wrapped_row_heights(&mut ui)?;
    assert!(narrow[0] > wide[0], "paragraph did not grow: {wide:?} -> {narrow:?}");
    assert!(narrow[1] > wide[1], "list item did not grow: {wide:?} -> {narrow:?}");
    width.set(920.0);
    assert_eq!(wrapped_row_heights(&mut ui)?, wide);
    Ok(())
}

#[test]
fn long_code_block_lines_wrap_instead_of_clipping() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let long = format!("    let call = compute({});", "argument, ".repeat(40));
    let source = format!("```rust\nfn short() {{}}\n{long}\n```");
    let mut ui = TestUiBuilder::default().size(720.0, 640.0).fonts(rouge_widgets::bundled_fonts()).build(|| {
        let doc = Arc::new(MarkdownDocument::parse(&source));
        markdown_review_view(MarkdownController::new(doc, ReviewTheme::default()).mount(|_| {}))
    });
    ui.advance(0.0);
    let short = ui.bounds_of("markdown-row-1").ok_or("no short code line")?;
    let long = ui.bounds_of("markdown-row-2").ok_or("no long code line")?;
    assert!(long.size.height > short.size.height * 4.0, "{short:?} {long:?}");
    assert!(long.max().x <= 720.0);
    Ok(())
}

fn mount(source: &str) -> (TestUi, MarkdownHandle) {
    TestUiBuilder::default().size(920.0, 640.0).fonts(rouge_widgets::bundled_fonts()).build_with(|| {
        let handle =
            MarkdownController::new(Arc::new(MarkdownDocument::parse(source)), ReviewTheme::default()).mount(|_| {});
        (markdown_review_view(handle), handle)
    })
}

fn wrapped_row_heights(ui: &mut TestUi) -> Result<[f32; 2], TestError> {
    let paragraph = ui.rect_of("markdown-row-1")?;
    let heading = ui.rect_of("markdown-row-2")?;
    let item = ui.rect_of("markdown-row-3")?;
    let next_item = ui.rect_of("markdown-row-4")?;
    assert!(heading.origin.y >= paragraph.max().y, "paragraph overlaps heading: {paragraph:?} {heading:?}");
    assert!(next_item.origin.y >= item.max().y, "list items overlap: {item:?} {next_item:?}");
    Ok([paragraph.size.height, item.size.height])
}

fn unit_strays(ui: &TestUi, scene: &str) -> Vec<String> {
    ui.named_bounds()
        .into_iter()
        .filter(|(_, bounds)| bounds.size.height % UNIT != 0.0)
        .map(|(name, bounds)| format!("{scene}: {name} is {} tall", bounds.size.height))
        .collect()
}
