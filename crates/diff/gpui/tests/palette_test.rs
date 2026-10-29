#![cfg(feature = "test-support")]

use clankerdiff_core::{DiffReviewEvent, InteractionPhase, testing::DocumentBuilder};
use clankerdiff_gpui::testing::DiffViewerHarnessBuilder;
use gpui::TestAppContext;

#[gpui::test]
fn question_mark_opens_the_commands_and_escape_closes_them(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        harness.simulate_keystrokes(cx, "shift-/");
        assert!(harness.read(cx, |viewer, _| viewer.commands_open()));
        assert_eq!(harness.read(cx, |viewer, _| viewer.command_context().phase), InteractionPhase::Help);
        assert!(harness.bounds(cx, "command-palette-backdrop").is_some());
        harness.simulate_keystrokes(cx, "escape");
        assert!(!harness.read(cx, |viewer, _| viewer.commands_open()));
        assert!(!harness.events(cx).contains(&DiffReviewEvent::Cancel));
        Ok(())
    });
}

#[gpui::test]
fn choosing_a_command_runs_it(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        let split = harness.read(cx, |viewer, _| viewer.layout().is_split());
        harness.simulate_keystrokes(cx, "shift-/");
        harness.simulate_input(cx, "toggle split");
        harness.simulate_keystrokes(cx, "enter");
        assert!(!harness.read(cx, |viewer, _| viewer.commands_open()));
        assert_ne!(harness.read(cx, |viewer, _| viewer.layout().is_split()), split);
        Ok(())
    });
}

#[gpui::test]
fn b_hides_and_shows_the_changed_files(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        assert!(!harness.read(cx, |viewer, _| viewer.sidebar_collapsed()));
        harness.simulate_keystrokes(cx, "b");
        assert!(harness.read(cx, |viewer, _| viewer.sidebar_collapsed()));
        harness.simulate_keystrokes(cx, "b");
        assert!(!harness.read(cx, |viewer, _| viewer.sidebar_collapsed()));
        Ok(())
    });
}

fn builder() -> DiffViewerHarnessBuilder {
    DiffViewerHarnessBuilder {
        document: DocumentBuilder::new()
            .changed("src/lib.rs", "old\n", "new\n")
            .changed("src/main.rs", "fn main() {}\n", "fn main() {\n    run();\n}\n")
            .build(),
        ..DiffViewerHarnessBuilder::default()
    }
}
