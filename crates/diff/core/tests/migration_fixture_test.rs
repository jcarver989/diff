use clankerdiff_core::{
    DiffDocument, DiffPresentation, DiffSide, PresentationOptions, Review, ReviewSubmission, ViewMode,
};
use std::{error::Error, fs, path::PathBuf, sync::Arc};

#[test]
fn acceptance_documents_keep_their_semantics_and_same_count_refresh() -> Result<(), Box<dyn Error>> {
    let original = load("small.json")?;
    let replacement = load("same-count-refresh.json")?;
    assert_eq!(original.files.len(), 6);
    assert!(original.files.iter().any(|file| file.binary));
    assert!(original.files.iter().any(|file| file.no_newline_at_end));
    assert!(original.files.iter().any(|file| file.path.as_str() == "src/renamed.rs"));
    assert!(original.files[0].source_document(DiffSide::New).ok_or("missing new source")?.text().contains("日本語"));
    assert_ne!(original, replacement);
    for view_mode in [ViewMode::Unified, ViewMode::Split] {
        let options = PresentationOptions { view_mode, ..Default::default() };
        let before = DiffPresentation::new(original.clone(), options);
        let after = DiffPresentation::new(replacement.clone(), options);
        assert_eq!(before.row_count(), after.row_count());
        assert_ne!(before.rows(0..before.row_count()), after.rows(0..after.row_count()));
    }
    Ok(())
}

#[test]
fn acceptance_review_preserves_structured_and_formatted_feedback() -> Result<(), Box<dyn Error>> {
    let submission: ReviewSubmission = serde_json::from_str(&fs::read_to_string(fixtures().join("review.json"))?)?;
    assert_eq!(submission.comments.len(), 1);
    let comment = &submission.comments[0];
    assert!(comment.body.contains('\n'));
    for line in comment.body.lines() {
        assert!(submission.formatted.contains(&format!("> {line}")));
    }
    let mut review = Review::default();
    review.add_comment(comment.anchor.clone(), comment.body.clone());
    assert_eq!(review.submission(), submission);
    let document = load("small.json")?;
    let presentation = DiffPresentation::new(document, PresentationOptions::default());
    assert!(presentation.row_showing_anchor(&comment.anchor).is_some());
    Ok(())
}

fn load(name: &str) -> Result<Arc<DiffDocument>, Box<dyn Error>> {
    Ok(Arc::new(serde_json::from_str(&fs::read_to_string(fixtures().join(name))?)?))
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/migration")
}
