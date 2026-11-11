use clankerdiff_client::{ClientState, ConnectionState};
use clankerdiff_core::{DiffReviewCommand, DiffReviewEvent, RepoPath, ReviewCapabilities, testing::DocumentBuilder};
use clankerdiff_rouge::{DiffClientView, DiffViewerOptions, ReviewOutput};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Size, testing::TestUiBuilder};
use rouge_reactive::Runtime;
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[test]
fn unchanged_snapshot_does_not_complete_repository_work_or_drop_view() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let outputs = Rc::new(RefCell::new(Vec::new()));
    let (mut ui, client) = TestUiBuilder::default().build_with(|| {
        let sink = outputs.clone();
        let client = DiffClientView::mount(ReviewTheme::default(), DiffViewerOptions::default(), move |output| {
            sink.borrow_mut().push(output);
        });
        (client.view(), client)
    });
    assert!(!client.handle_command(DiffReviewCommand::Refresh));
    let snapshot = DocumentBuilder::new().changed("src/lib.rs", "old\n", "new\n").build();
    let mut state = ClientState {
        snapshot: Some(snapshot.clone()),
        files: None,
        connection: ConnectionState::Connected,
        capabilities: ReviewCapabilities::default(),
        error: None,
    };
    client.set_state(&state);
    ui.advance(0.0);
    let presentation = client.review().presentation_revision().get();
    assert!(client.handle_command(DiffReviewCommand::Refresh));
    let files: Arc<[RepoPath]> = vec![RepoPath::new("README.md")?].into();
    state.files = Some(files);
    client.set_state(&state);
    assert_eq!(client.review().presentation_revision().get(), presentation);
    assert!(!client.handle_command(DiffReviewCommand::Refresh));
    client.command_finished(Ok(()));
    assert!(client.handle_command(DiffReviewCommand::Refresh));
    assert_eq!(
        outputs.borrow().as_slice(),
        [ReviewOutput::Event(DiffReviewEvent::Refresh), ReviewOutput::Event(DiffReviewEvent::Refresh)]
    );
    state.snapshot = Some(DocumentBuilder::new().build());
    client.set_state(&state);
    ui.advance(0.0);
    client.set_state(&state);
    ui.advance(0.0);
    drop(ui);
    client.set_state(&state);
    assert!(!client.handle_command(DiffReviewCommand::Refresh));
    Ok(())
}

#[test]
fn loaded_view_fills_the_window() -> Result<(), Box<dyn std::error::Error>> {
    let _runtime = Runtime::new().enter();
    let (mut ui, client) = TestUiBuilder::default().size(1024.0, 768.0).build_with(|| {
        let client = DiffClientView::mount(ReviewTheme::default(), DiffViewerOptions::default(), |_| {});
        (client.view(), client)
    });
    client.set_state(&ClientState {
        snapshot: Some(DocumentBuilder::new().changed("src/lib.rs", "old\n", "new\n").build()),
        files: None,
        connection: ConnectionState::Connected,
        capabilities: ReviewCapabilities::default(),
        error: None,
    });
    ui.advance(0.0);
    let viewer = ui.bounds_of("diff-viewer").ok_or("diff viewer is not mounted")?;
    assert_eq!(viewer.size, Size::new(1024.0, 768.0));
    ui.center_of("diff-row-1")?;
    Ok(())
}
