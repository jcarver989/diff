mod common;

use clankerdiff_core::{
    DiffReviewCommand as C, DiffReviewEvent, DiffSide, LookupReply, LookupRequest, ReviewCapabilities, TargetLocation,
    testing::{DocumentBuilder, repo, source, target},
};
use clankerdiff_rouge::ReviewOutput;
use common::ReviewHarness;
use rouge_core::testing::TestResult;

#[test]
fn definition_card_keys_take_precedence_over_navigation_and_cancel() -> TestResult {
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", "", "pub fn build() -> u32 { 1 }\n")
        .changed("src/main.rs", "", "fn main() {\n    build();\n}\n")
        .build();
    let mut review = ReviewHarness::new(document);
    assert!(review.handle.handle_command(C::JumpTo(target("src/main.rs", 2)?)));
    review.press("shift-D");
    review.assert_visible("definition-card");
    review.press("escape");
    review.assert_hidden("definition-card");
    assert!(review.outputs().is_empty());

    review.press("shift-D");
    review.press("enter");
    review.assert_selected_line("src/lib.rs", 1)?;
    review.press("ctrl-o");
    review.assert_selected_line("src/main.rs", 2)?;
    assert!(review.handle.handle_command(C::NavigateForward));
    review.assert_selected_line("src/lib.rs", 1)?;
    Ok(())
}

#[test]
fn stale_reference_reply_does_not_replace_the_current_location() -> TestResult {
    let path = repo("src/other.rs")?;
    let review = ReviewHarness::new(common::document());
    review.handle.set_capabilities(ReviewCapabilities { lookups: true, ..ReviewCapabilities::default() });
    let jump = C::JumpTo(TargetLocation { path: path.clone(), side: DiffSide::New, line_number: 1 });
    assert!(review.handle.handle_command(jump.clone()));
    let request_token = || {
        review.outputs().iter().rev().find_map(|output| match output {
            ReviewOutput::Event(DiffReviewEvent::Lookup(lookup))
                if lookup.request == LookupRequest::Source(path.clone()) =>
            {
                Some(lookup.token)
            }
            _ => None,
        })
    };
    let old = request_token().ok_or("expected source request")?;
    assert!(review.handle.handle_command(C::NavigateBack));
    assert!(review.handle.handle_command(jump));
    let current = request_token().ok_or("expected replacement source request")?;
    assert_ne!(current, old);
    let reply = |token| LookupReply::Source { token, path: path.clone(), result: Ok(source("fn helper() {}\n")) };
    review.handle.lookup_finished(reply(old));
    assert!(review.handle.with(|state| state.session().reference().is_none()));
    review.handle.lookup_finished(reply(current));
    assert!(review.handle.with(|state| state.session().reference().is_some()));
    Ok(())
}
