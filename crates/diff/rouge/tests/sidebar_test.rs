mod common;

use clankerdiff_core::{
    DiffReviewCommand as C, DiffReviewEvent, RepoPath, RepositoryAction, ReviewCapabilities, StageState,
    testing::DocumentBuilder,
};
use clankerdiff_rouge::{ReviewOutput, ViewerPane};
use common::ReviewHarness;
use rouge_core::testing::TestResult;
use rouge_widgets::tokens::CHECKBOX_SIZE;
use std::io;

#[test]
fn file_tree_navigation_reveals_the_last_file_without_mounting_the_whole_tree() -> TestResult {
    const FILES: usize = 300;
    let doc = (0..FILES)
        .fold(DocumentBuilder::new(), |builder, file| builder.generated(&format!("src/file_{file:03}.rs"), 2))
        .build();
    let mut review = ReviewHarness::new(doc);
    review.click("diff-file-0")?;
    assert_eq!(review.handle.selection().get().pane, ViewerPane::Files);
    review.assert_hidden(&format!("diff-file-{}", FILES - 1));
    let mounted = (0..FILES).filter(|file| review.ui.bounds_of(&format!("diff-file-{file}")).is_some()).count();
    assert!(mounted < FILES / 4, "mounted {mounted} of {FILES} rows");
    assert!(review.handle.handle_command(C::Last));
    review.ui.advance(0.0);
    let row =
        review.ui.bounds_of(&format!("diff-file-{}", FILES - 1)).ok_or_else(|| io::Error::other("no last file"))?;
    let panel = review.ui.bounds_of("files-panel").ok_or_else(|| io::Error::other("no files panel"))?;
    assert_eq!(review.handle.selection().get().file, Some(FILES - 1));
    assert!(row.origin.y >= panel.origin.y && row.origin.y + row.size.height <= panel.origin.y + panel.size.height);
    review.assert_hidden("diff-file-0");
    review.press("up");
    assert_eq!(review.handle.selection().get().file, Some(FILES - 2));
    Ok(())
}

#[test]
fn staging_checkboxes_keep_their_geometry_and_do_not_activate_rows() -> TestResult {
    for stage in [StageState::Unstaged, StageState::Staged, StageState::PartiallyStaged] {
        let doc = DocumentBuilder::new()
            .changed_staged("src/lib.rs", "old", "new", stage)
            .changed("z.rs", "old", "new")
            .build();
        let mut review = ReviewHarness::new(doc);
        review.handle.set_capabilities(ReviewCapabilities { repository: true, ..ReviewCapabilities::NONE });
        assert!(review.handle.handle_command(C::SelectFile(1)));
        review.ui.advance(0.0);
        let file = review.ui.bounds_of("diff-file-checkbox:0").ok_or_else(|| io::Error::other("no file checkbox"))?;
        let directory = review
            .ui
            .bounds_of("diff-directory-checkbox:src")
            .ok_or_else(|| io::Error::other("no directory checkbox"))?;
        assert_eq!((file.size.width, file.size.height), (CHECKBOX_SIZE, CHECKBOX_SIZE));
        assert_eq!(file.size, directory.size);
        review.click("diff-file-checkbox:0")?;
        assert_eq!(review.handle.selection().get().file, Some(1));
        let path = vec![RepoPath::new("src/lib.rs")?];
        let expected = if stage == StageState::Staged {
            RepositoryAction::UnstagePaths(path)
        } else {
            RepositoryAction::StagePaths(path)
        };
        assert!(matches!(
            &*review.outputs(),
            [ReviewOutput::Event(DiffReviewEvent::RepositoryAction(action))] if *action == expected
        ));
    }
    Ok(())
}
