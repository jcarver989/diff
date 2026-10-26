use clankerdiff_core::{DiffDocument, DiffSide, FileDiff, SourceUnavailable};
use std::error::Error;

#[test]
fn serializes_owned_model() -> Result<(), serde_json::Error> {
    let document = DiffDocument::empty();
    let json = serde_json::to_string(&document)?;
    assert_eq!(serde_json::from_str::<DiffDocument>(&json)?, document);
    Ok(())
}

#[test]
fn documents_round_trip_complete_sources_and_accept_patch_only_json() -> Result<(), Box<dyn Error>>
{
    let document = DiffDocument::from_texts([("a.rs", "one\n", "two\n")])?;
    let json = serde_json::to_string(&document)?;
    assert!(json.contains("\"old_source\":{\"Ok\":\"one\\n\"}"));
    let decoded = serde_json::from_str::<DiffDocument>(&json)?;
    assert_eq!(decoded, document);
    assert_eq!(
        decoded.files[0]
            .source_document(DiffSide::New)
            .map(|source| source.text()),
        Some("two\n")
    );

    let patch_only =
        serde_json::from_str::<DiffDocument>(&serde_json::to_string(&DiffDocument {
            files: vec![FileDiff {
                old_source: Err(SourceUnavailable::NotCaptured),
                new_source: Err(SourceUnavailable::NotCaptured),
                ..document.files[0].clone()
            }],
            ..document.clone()
        })?)?;
    assert!(!serde_json::to_string(&patch_only)?.contains("_source"));
    assert_eq!(
        patch_only.files[0].source_unavailable(DiffSide::Old),
        Some(&SourceUnavailable::NotCaptured)
    );
    Ok(())
}

#[test]
fn model_namespaces_reexport_the_same_types() {
    let document: clankerdiff_core::models::DiffDocument = DiffDocument::empty();
    let legacy: clankerdiff_core::model::DiffDocument = document.clone();
    assert_eq!(legacy, document);
}
