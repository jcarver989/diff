#![cfg(feature = "test-support")]

use clankerdiff_core::testing::{DocumentBuilder, location};
use clankerdiff_gpui::testing::DiffViewerHarnessBuilder;
use gpui::{Modifiers, MouseMoveEvent, TestAppContext, point, px};

#[gpui::test]
fn hovering_a_name_with_the_secondary_modifier_shows_its_signature_and_docs(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        harness.hover_name(cx, "code:src/main.rs:new:2", "build")?;
        harness.settle(cx);
        for selector in ["definition-card", "definition-card-signature", "definition-card-docs"] {
            assert!(harness.bounds(cx, selector).is_some(), "{selector}");
        }
        Ok(())
    });
}

#[gpui::test]
fn moving_off_the_name_hides_a_hovered_card_but_the_card_itself_keeps_it_open(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        let name = harness.hover_name(cx, "code:src/main.rs:new:2", "build")?;
        harness.hold_secondary(cx, false);
        let card = harness.center(cx, "definition-card")?;
        harness.simulate_event(
            cx,
            MouseMoveEvent { position: card, pressed_button: None, modifiers: Modifiers::default() },
        );
        harness.settle(cx);
        assert!(harness.bounds(cx, "definition-card").is_some(), "the pointer is resting on the card");
        harness.simulate_event(
            cx,
            MouseMoveEvent { position: point(name.x, px(4.0)), pressed_button: None, modifiers: Modifiers::default() },
        );
        harness.settle(cx);
        assert!(harness.bounds(cx, "definition-card").is_none());
        Ok(())
    });
}

#[gpui::test]
fn tab_cycles_definitions_and_enter_jumps_to_the_chosen_one(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        harness.select_line(cx, &location("src/lib.rs", 3)?)?;
        harness.simulate_keystrokes(cx, "shift-d a");
        assert_eq!(harness.definition_card_name(cx).as_deref(), Some("helper"));
        harness.simulate_keystrokes(cx, "tab enter");
        assert_eq!(harness.selected_line(cx), Some(location("src/util.rs", 1)?));
        Ok(())
    });
}

#[gpui::test]
fn a_secondary_click_jumps_to_the_definition_in_the_diff(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        let position = harness.hover_name(cx, "code:src/main.rs:new:2", "build")?;
        harness.secondary_click_at(cx, position);
        assert_eq!(harness.selected_line(cx), Some(location("src/lib.rs", 2)?));
        assert!(harness.bounds(cx, "definition-card").is_none());
        Ok(())
    });
}

#[gpui::test]
fn a_name_defined_outside_the_diff_keeps_its_card_open_instead_of_jumping(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        let origin = location("src/lib.rs", 3)?;
        harness.select_line(cx, &origin)?;
        let position = harness.hover_name(cx, "code:src/lib.rs:new:3", "offset")?;
        harness.secondary_click_at(cx, position);
        harness.settle(cx);
        assert_eq!(harness.selected_line(cx), Some(origin));
        assert!(harness.bounds(cx, "definition-card").is_some());
        assert!(harness.bounds(cx, "definition-card-signature").is_none());
        Ok(())
    });
}

fn builder() -> DiffViewerHarnessBuilder {
    DiffViewerHarnessBuilder {
        document: DocumentBuilder::new()
            .changed(
                "src/lib.rs",
                "",
                "/// Adds the parts.\npub fn build() -> u32 {\n    helper() + offset()\n}\n\nfn helper() -> u32 {\n    1\n}\n",
            )
            .changed("src/main.rs", "", "fn main() {\n    build();\n}\n")
            .changed("src/util.rs", "", "pub fn helper() -> u32 {\n    2\n}\n")
            .build(),
        ..DiffViewerHarnessBuilder::default()
    }
}
