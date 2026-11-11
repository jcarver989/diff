mod common;

use clankerdiff_core::{
    DiffReviewCommand,
    testing::{DocumentBuilder, location},
};
use clankerdiff_rouge::ReviewController;
use rouge_core::{Modifiers, Point, testing::TestResult};
use std::io;

#[test]
fn modifier_hover_uses_shaped_utf8_ranges_and_click_jumps() -> TestResult {
    let document = DocumentBuilder::new()
        .changed("src/lib.rs", "", "pub fn build() -> u32 { 1 }\n")
        .changed("src/main.rs", "", "fn main() {\n    let _ = \"漢\"; build();\n}\n")
        .build();
    let origin = location("src/main.rs", 2)?;
    let destination = location("src/lib.rs", 1)?;
    common::run(document, |ui, handle, _| {
        assert!(handle.handle_command(DiffReviewCommand::JumpTo(origin.into())));
        let row = handle.selection().get().row.ok_or_else(|| io::Error::other("no selected row"))?;
        assert!(ui.wait_until(|_| handle.with(ReviewController::symbols_ready)), "symbols never became ready");
        let bounds = ui.bounds_of(&format!("code-{row}-New")).ok_or_else(|| io::Error::other("code not mounted"))?;
        let token = "    let _ = \"漢\"; ".len();
        let point = (0_u16..350)
            .map(|n| Point::new(bounds.origin.x + f32::from(n), bounds.center().y))
            .find(|point| ui.text_index_at(*point).is_some_and(|index| (token + 1..token + 4).contains(&index)))
            .ok_or_else(|| io::Error::other("build token has no shaped hit range"))?;
        ui.pointer_moved(point);
        assert!(handle.with(|s| s.definition_card().is_none()));
        ui.set_modifiers(Modifiers::COMMAND);
        assert!(handle.with(|s| s.definition_card().is_some_and(|card| card.name == "build")));
        assert!(ui.center_of("definition-card-signature").is_ok());
        ui.click(point);
        assert_eq!(handle.with(|s| s.session().selected_source_line()), Some(destination));
        Ok(())
    })
}
