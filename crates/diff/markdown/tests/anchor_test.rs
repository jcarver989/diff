#![cfg(feature = "review")]

use clankerdiff_markdown::MarkdownDocument;
use std::error::Error;

const SOURCE: &str = "# Title\n\nIntro\n\n- one\n- two\n\n```rust\nfn a() {}\nfn b() {}\n```\n";

#[test]
fn every_target_is_found_exactly_by_its_own_anchor() -> Result<(), Box<dyn Error>> {
    let document = MarkdownDocument::parse(SOURCE);
    assert!(document.targets().len() > 5);
    for target in document.targets() {
        let anchor = document.anchor_for_target(target.id).ok_or("anchor")?;
        assert_eq!(document.exact_target(&anchor), Some(target.id), "{target:?}");
    }
    Ok(())
}

#[test]
fn an_anchor_from_an_edited_document_resolves_only_fuzzily() -> Result<(), Box<dyn Error>> {
    let before = MarkdownDocument::parse(SOURCE);
    let target =
        before.targets().iter().find(|target| before.rendered_target_text(target.id).as_deref() == Some("Intro"));
    let anchor = before.anchor_for_target(target.ok_or("intro")?.id).ok_or("anchor")?;
    let after = MarkdownDocument::parse(SOURCE.replace("# Title", "# Renamed"));
    assert!(after.resolve_anchor(&anchor).is_some());
    assert_eq!(after.exact_target(&anchor), None);
    Ok(())
}
