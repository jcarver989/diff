#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DiffReviewEvent,
    testing::{DocumentBuilder, location},
};
use clankerdiff_gpui::testing::DiffViewerHarnessBuilder;
use gpui::TestAppContext;

#[gpui::test]
fn shift_d_shows_the_only_name_on_a_line_and_escape_closes_it_without_cancelling(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        harness.select_line(cx, &location("src/main.rs", 2)?)?;
        harness.simulate_keystrokes(cx, "shift-d");
        harness.draw(cx);
        assert!(harness.bounds(cx, "definition-card").is_some());
        harness.simulate_keystrokes(cx, "escape");
        harness.draw(cx);
        assert!(harness.bounds(cx, "definition-card").is_none());
        assert_eq!(harness.events(cx).len(), 0);
        Ok(())
    });
}

#[gpui::test]
fn shift_d_on_a_line_with_several_names_offers_lettered_choices(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        harness.select_line(cx, &location("src/lib.rs", 2)?)?;
        harness.simulate_keystrokes(cx, "shift-d");
        harness.draw(cx);
        assert!(harness.bounds(cx, "name-hints").is_some());
        assert!(harness.bounds(cx, "definition-card").is_none());
        harness.simulate_keystrokes(cx, "s");
        harness.draw(cx);
        assert!(harness.bounds(cx, "name-hints").is_none());
        assert_eq!(harness.definition_card_name(cx).as_deref(), Some("offset"));
        Ok(())
    });
}

#[gpui::test]
fn escape_dismisses_lettered_choices(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        harness.select_line(cx, &location("src/lib.rs", 2)?)?;
        harness.simulate_keystrokes(cx, "shift-d escape");
        harness.draw(cx);
        assert!(harness.bounds(cx, "name-hints").is_none());
        assert!(!harness.events(cx).contains(&DiffReviewEvent::Cancel));
        Ok(())
    });
}

#[gpui::test]
fn enter_jumps_to_the_definition_and_ctrl_o_and_ctrl_i_walk_history(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        let origin = location("src/main.rs", 2)?;
        let target = location("src/lib.rs", 1)?;
        harness.select_line(cx, &location("src/main.rs", 2)?)?;
        harness.simulate_keystrokes(cx, "shift-d enter");
        harness.draw(cx);
        assert_eq!(harness.selected_line(cx), Some(target.clone()));
        assert!(harness.bounds(cx, "definition-card").is_none());
        assert!(harness.bounds(cx, "navigate-back").is_some());
        harness.simulate_keystrokes(cx, "ctrl-o");
        assert_eq!(harness.selected_line(cx), Some(origin));
        harness.simulate_keystrokes(cx, "ctrl-i");
        assert_eq!(harness.selected_line(cx), Some(target));
        Ok(())
    });
}

#[gpui::test]
fn the_sticky_header_names_the_enclosing_symbol_and_lists_its_siblings(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        harness.select_line(cx, &location("src/lib.rs", 2)?)?;
        harness.draw(cx);
        assert!(harness.bounds(cx, "symbol-path").is_some());
        harness.click(cx, "symbol-path-0")?;
        harness.draw(cx);
        assert!(harness.bounds(cx, "symbol-path-0-menu").is_some());
        harness.click(cx, "symbol-path-item-1")?;
        harness.draw(cx);
        assert_eq!(harness.selected_line(cx), Some(location("src/lib.rs", 5)?));
        assert!(harness.bounds(cx, "navigate-back").is_some());
        Ok(())
    });
}

fn builder() -> DiffViewerHarnessBuilder {
    DiffViewerHarnessBuilder {
        document: DocumentBuilder::new()
            .changed(
                "src/lib.rs",
                "",
                "pub fn build() -> u32 {\n    helper() + offset()\n}\n\nfn helper() -> u32 {\n    1\n}\n",
            )
            .changed("src/main.rs", "", "fn main() {\n    build();\n}\n")
            .build(),
        ..DiffViewerHarnessBuilder::default()
    }
}
