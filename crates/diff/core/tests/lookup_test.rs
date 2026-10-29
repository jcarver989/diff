#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DefinitionRequest, DiffDocument, DiffSide, Fingerprint, Lookup, LookupAnswer, LookupFailure, LookupReply,
    LookupRequest, LookupStatus, LookupUnavailable, RepoPath, Resolution, SourceLocation, SourceUnavailable,
    testing::{DocumentBuilder, location},
};
use std::{error::Error, sync::Arc};

#[test]
fn a_new_side_request_names_the_reviewed_content() -> Result<(), Box<dyn Error>> {
    let document = document();
    let request = DefinitionRequest::at(&document, location("src/main.rs", 3)?, 4)?;
    assert_eq!(request.at, location("src/main.rs", 3)?);
    assert_eq!(request.content, new_content(&document)?);
    Ok(())
}

#[test]
fn an_unchanged_old_side_line_asks_about_the_same_line_in_the_current_version() -> Result<(), Box<dyn Error>> {
    let document = document();
    let request = DefinitionRequest::at(&document, old("src/main.rs", 2)?, 4)?;
    assert_eq!((request.at, request.column), (location("src/main.rs", 3)?, 4));
    assert_eq!(request.content, new_content(&document)?);
    Ok(())
}

#[test]
fn removed_lines_deleted_files_and_missing_sources_cannot_be_asked_about() -> Result<(), Box<dyn Error>> {
    let document = document();
    let reason = |at| DefinitionRequest::at(&document, at, 0).err();
    assert_eq!(reason(old("src/main.rs", 3)?), Some(LookupUnavailable::NotInCurrentVersion));
    assert_eq!(reason(old("src/gone.rs", 1)?), Some(LookupUnavailable::NotInCurrentVersion));
    assert_eq!(reason(location("src/elsewhere.rs", 1)?), Some(LookupUnavailable::SourceUnavailable));
    assert_eq!(reason(old("src/huge.rs", 1)?), Some(LookupUnavailable::SourceUnavailable));
    Ok(())
}

#[test]
fn a_failed_definition_lookup_reports_why() -> Result<(), Box<dyn Error>> {
    let request = DefinitionRequest::at(&document(), location("src/main.rs", 3)?, 4)?;
    let lookup = Lookup { token: 1, request: LookupRequest::Definition(request) };
    let LookupReply::Definition { resolution, .. } = LookupReply::new(lookup.clone(), Err("daemon gone".to_owned()))
    else {
        return Err("a definition reply".into());
    };
    assert_eq!(resolution.status, LookupStatus::Failed(LookupFailure::Server("daemon gone".to_owned())));
    let LookupReply::Definition { resolution, .. } =
        LookupReply::new(lookup, Ok(LookupAnswer::Definition(Resolution::default())))
    else {
        return Err("a definition reply".into());
    };
    assert_eq!(resolution.status, LookupStatus::Complete);
    Ok(())
}

#[test]
fn resolutions_from_older_peers_default_to_complete() -> Result<(), Box<dyn Error>> {
    let resolution: Resolution = serde_json::from_str(r#"{"targets":[],"hover":"docs"}"#)?;
    assert_eq!(resolution.status, LookupStatus::Complete);
    assert!(!resolution.current_version);
    assert_eq!(resolution.hover.as_deref(), Some("docs"));
    Ok(())
}

fn document() -> Arc<DiffDocument> {
    DocumentBuilder::new()
        .changed(
            "src/main.rs",
            "fn main() {\n    run();\n    old();\n}\n",
            "fn main() {\n    let one = 1;\n    run();\n}\n",
        )
        .deleted("src/gone.rs", "fn gone() {}\n")
        .changed("src/huge.rs", "fn huge() {}\n", "fn huge() {}\nfn more() {}\n")
        .source("src/huge.rs", DiffSide::New, Err::<&str, _>(SourceUnavailable::TooLarge { bytes: 9_000_000 }))
        .build()
}

fn old(path: &str, line_number: usize) -> Result<SourceLocation, Box<dyn Error>> {
    Ok(SourceLocation { side: DiffSide::Old, ..location(path, line_number)? })
}

fn new_content(document: &DiffDocument) -> Result<Fingerprint, Box<dyn Error>> {
    Ok(document.source(&RepoPath::new("src/main.rs")?, DiffSide::New).ok_or("a new source")?.content_id())
}
