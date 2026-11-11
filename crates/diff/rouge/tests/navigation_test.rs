mod common;

use clankerdiff_core::{
    DiffReviewCommand as C, DiffReviewEvent, DiffSide, LookupReply, LookupRequest, ReviewCapabilities, TargetLocation,
    testing::{DocumentBuilder, location, repo, source},
};
use clankerdiff_rouge::ReviewOutput;
use rouge_core::{Key, Modifiers, testing::TestResult};

#[test]
fn definition_card_keys_take_precedence_over_navigation_and_cancel() -> TestResult {
    let origin = location("src/main.rs", 2)?;
    let target = location("src/lib.rs", 1)?;
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", "", "pub fn build() -> u32 { 1 }\n")
        .changed("src/main.rs", "", "fn main() {\n    build();\n}\n")
        .build();
    common::run(document, |ui, handle, outputs| {
        assert!(handle.handle_command(C::JumpTo(origin.clone().into())));
        ui.press_with(Key::character("D"), Modifiers::SHIFT);
        assert!(handle.with(|state| state.definition_card().is_some()));
        ui.press(Key::Escape);
        assert!(handle.with(|state| state.definition_card().is_none()));
        assert!(outputs.borrow().is_empty());
        ui.press_with(Key::character("D"), Modifiers::SHIFT);
        ui.press(Key::Enter);
        assert_eq!(handle.with(|state| state.session().selected_source_line()), Some(target.clone()));
        ui.press_with(Key::character("o"), Modifiers::CTRL);
        assert_eq!(handle.with(|state| state.session().selected_source_line()), Some(origin.clone()));
        assert!(handle.handle_command(C::NavigateForward));
        assert_eq!(handle.with(|state| state.session().selected_source_line()), Some(target));
    });
    Ok(())
}

#[test]
fn stale_reference_reply_does_not_replace_the_current_location() -> TestResult {
    let path = repo("src/other.rs")?;
    common::run(common::document(), |_, handle, outputs| {
        handle.set_capabilities(ReviewCapabilities { lookups: true, ..ReviewCapabilities::default() });
        let jump = C::JumpTo(TargetLocation { path: path.clone(), side: DiffSide::New, line_number: 1 });
        assert!(handle.handle_command(jump.clone()));
        let request_token = || {
            outputs.borrow().iter().rev().find_map(|output| match output {
                ReviewOutput::Event(DiffReviewEvent::Lookup(lookup))
                    if lookup.request == LookupRequest::Source(path.clone()) =>
                {
                    Some(lookup.token)
                }
                _ => None,
            })
        };
        let Some(old) = request_token() else {
            panic!("expected source request");
        };
        assert!(handle.handle_command(C::NavigateBack));
        assert!(handle.handle_command(jump));
        let Some(current) = request_token() else {
            panic!("expected replacement source request");
        };
        assert_ne!(current, old);
        let reply = |token| LookupReply::Source { token, path: path.clone(), result: Ok(source("fn helper() {}\n")) };
        handle.lookup_finished(reply(old));
        assert!(handle.with(|state| state.session().reference().is_none()));
        handle.lookup_finished(reply(current));
        assert!(handle.with(|state| state.session().reference().is_some()));
    });
    Ok(())
}
