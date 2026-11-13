use clankerdiff_core::ReviewCommand;
use clankerdiff_markdown::{
    MarkdownDocument, MarkdownFocusPane, MarkdownReviewCommand, MarkdownReviewEvent, MarkdownReviewSession,
    MarkdownTargetId,
};
use clankerdiff_rouge::{MarkdownController, MarkdownHandle, MarkdownOutput, markdown_blocks, markdown_review_view};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{
    ImeEvent,
    testing::{TestUi, TestUiBuilder},
};
use rouge_reactive::{Callback, Runtime, signal};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[test]
fn mounted_markdown_uses_semantic_selection_and_single_review_session() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let output = Rc::new(RefCell::new(Vec::new()));
    let (mut ui, handle) =
        TestUiBuilder::default().size(920.0, 640.0).fonts(rouge_widgets::bundled_fonts()).build_with(|| {
            let doc = Arc::new(MarkdownDocument::new(
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
    handle.set_document(Arc::new(MarkdownDocument::new(
        "\n# Title\n\nA **paragraph** with `code`.\n\n```rust\nfn main() {}\n```",
    )));
    ui.advance(0.0);
    assert_eq!(handle.with(|s| s.review().len()), 1);
    assert!(handle.handle_command(MarkdownReviewCommand::RequestChanges)?);
    assert!(matches!(output.borrow().as_slice(), [MarkdownOutput::Event(MarkdownReviewEvent::Submit(_))]));
    Ok(())
}

#[test]
fn outline_lists_every_heading_that_fits_the_window() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let (mut ui, _handle) = TestUiBuilder::default().size(920.0, 640.0).build_with(|| {
        let doc = Arc::new(MarkdownDocument::new("# One\n\n## Two\n\n## Three\n\n## Four\n\n## Five\n\n## Six\n"));
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
    let headings = handle.with(|s| s.document().outline().iter().map(|h| h.target_id).collect::<Vec<_>>());
    assert!(handle.handle_command(MarkdownReviewCommand::Focus(MarkdownFocusPane::Outline))?);
    assert!(handle.handle_command(MarkdownReviewCommand::MoveSelection(1))?);
    assert_eq!(handle.with(MarkdownReviewSession::selected_target), headings.get(1).copied());
    assert!(handle.handle_command(MarkdownReviewCommand::Last)?);
    assert_eq!(handle.with(MarkdownReviewSession::selected_target), headings.last().copied());
    Ok(())
}

fn mount(source: &str) -> (TestUi, MarkdownHandle) {
    TestUiBuilder::default().size(920.0, 640.0).fonts(rouge_widgets::bundled_fonts()).build_with(|| {
        let handle =
            MarkdownController::new(Arc::new(MarkdownDocument::new(source)), ReviewTheme::default()).mount(|_| {});
        (markdown_review_view(handle), handle)
    })
}

#[test]
fn standalone_markdown_blocks_render_without_a_review_and_copy_code() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let output = Rc::new(RefCell::new(Vec::new()));
    let mut ui = TestUiBuilder::default().size(600.0, 400.0).fonts(rouge_widgets::bundled_fonts()).build(|| {
        let _ = clankerdiff_rouge::theme::ActiveReviewTheme::install(ReviewTheme::default());
        let doc = MarkdownDocument::new("Some **prose**.\n\n```sh\necho hi\n```");
        let sink = output.clone();
        let emit = Callback::from(move |output: MarkdownOutput| sink.borrow_mut().push(output));
        markdown_blocks(doc.blocks(), signal(ReviewTheme::default()).read_only(), true, emit)
    });
    ui.advance(0.0);
    ui.click_on("markdown-copy-code")?;
    assert_eq!(output.borrow().as_slice(), [MarkdownOutput::CopyCode("echo hi\n".into())]);
    Ok(())
}
