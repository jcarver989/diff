#![cfg(feature = "test-support")]

use clankerdiff_core::{
    DiffReviewCommand, DiffReviewEvent, DiffSide, LookupAnswer, LookupRequest, TargetLocation,
    testing::{DocumentBuilder, ResolutionBuilder, external, location, repo, source, target},
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
fn shift_d_again_steps_to_the_next_name_on_the_line(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        harness.select_line(cx, &location("src/lib.rs", 2)?)?;
        harness.simulate_keystrokes(cx, "shift-d");
        harness.draw(cx);
        let first = harness.definition_card_name(cx);
        assert!(first.is_some() && first.as_deref() != Some("offset"));
        harness.simulate_keystrokes(cx, "shift-d");
        harness.draw(cx);
        assert_eq!(harness.definition_card_name(cx).as_deref(), Some("offset"));
        Ok(())
    });
}

#[gpui::test]
fn escape_closes_the_card_without_cancelling_the_review(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        harness.select_line(cx, &location("src/lib.rs", 2)?)?;
        harness.simulate_keystrokes(cx, "shift-d escape");
        harness.draw(cx);
        assert!(harness.bounds(cx, "definition-card").is_none());
        assert!(!harness.events(cx).contains(&DiffReviewEvent::Cancel));
        Ok(())
    });
}

#[gpui::test]
fn enter_jumps_to_the_definition_and_ctrl_o_and_forward_walk_history(cx: &mut TestAppContext) {
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
        harness.click(cx, "navigate-forward")?;
        assert_eq!(harness.selected_line(cx), Some(target));
        Ok(())
    });
}

#[gpui::test]
fn the_toolbar_always_offers_back_and_forward_which_act_once_there_is_history(cx: &mut TestAppContext) {
    builder().run(cx, |harness, cx| {
        let origin = location("src/main.rs", 2)?;
        harness.select_line(cx, &origin)?;
        harness.draw(cx);
        let toolbar = harness.bounds(cx, "diff-toolbar").ok_or("no toolbar")?;
        for selector in ["navigate-back", "navigate-forward"] {
            let button = harness.bounds(cx, selector).ok_or(selector)?;
            assert!(toolbar.contains(&button.center()), "{selector} sits in the toolbar");
        }
        harness.click(cx, "navigate-back")?;
        assert_eq!(harness.selected_line(cx), Some(origin.clone()));

        harness.simulate_keystrokes(cx, "shift-d enter");
        assert_ne!(harness.selected_line(cx), Some(origin.clone()));
        harness.click(cx, "navigate-back")?;
        assert_eq!(harness.selected_line(cx), Some(origin));
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

#[gpui::test]
fn a_definition_outside_the_repository_opens_read_only_and_escape_returns(cx: &mut TestAppContext) {
    builder().with_lookups().run(cx, |harness, cx| {
        let origin = location("src/main.rs", 2)?;
        harness.select_line(cx, &origin)?;
        harness.simulate_keystrokes(cx, "shift-d");
        let definition = harness.last_lookup(cx).ok_or("lookup")?;
        harness.answer_lookup(cx, definition, Ok(ResolutionBuilder::new().external(4, STD, 3).answer()));
        harness.simulate_keystrokes(cx, "enter");
        let std = external(4, STD);
        let lookup = harness.last_lookup(cx).ok_or("the source is requested")?;
        assert_eq!(lookup.request, LookupRequest::Source(std.clone()));
        assert!(harness.bounds(cx, "reference-status").is_some());

        harness.answer_lookup(cx, lookup, Ok(LookupAnswer::Source(source(STD_SOURCE))));
        let target = TargetLocation { path: std, side: DiffSide::New, line_number: 3 };
        assert_eq!(harness.read(cx, |viewer, _| viewer.session().current_location()), Some(target));
        assert!(harness.bounds(cx, "reference-status").is_none());
        harness.simulate_keystrokes(cx, "escape");
        assert_eq!(harness.selected_line(cx), Some(origin));
        assert!(harness.read(cx, |viewer, _| viewer.session().reference().is_none()));
        Ok(())
    });
}

#[gpui::test]
fn a_file_opened_while_the_diff_is_clean_is_shown_read_only(cx: &mut TestAppContext) {
    let clean =
        DiffViewerHarnessBuilder { document: DocumentBuilder::new().build(), ..DiffViewerHarnessBuilder::default() };
    clean.with_lookups().run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "browse-empty").is_some(), "a clean repository is browsed");
        let file = target("src/util.rs", 2)?;
        assert!(harness.dispatch_command(cx, DiffReviewCommand::JumpTo(file.clone()))?);
        let lookup = harness.last_lookup(cx).ok_or("the source is requested")?;
        assert_eq!(lookup.request, LookupRequest::Source(file.path));
        assert!(harness.bounds(cx, "reference-status").is_some());

        harness.answer_lookup(cx, lookup, Ok(LookupAnswer::Source(source(UTIL_SOURCE))));
        assert!(harness.bounds(cx, "browse-empty").is_none());
        assert!(harness.bounds(cx, "diff-pane").is_some());
        assert_eq!(harness.selected_line(cx), Some(location("src/util.rs", 2)?));
        Ok(())
    });
}

#[gpui::test]
fn browse_shows_a_change_as_it_is_now_and_diff_returns_to_it(cx: &mut TestAppContext) {
    builder().with_lookups().run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "diff-files").is_some(), "changes open in the diff");
        let diffed = harness.selected_line(cx);

        harness.click(cx, "files-mode-browse")?;

        assert!(harness.bounds(cx, "source-mode-header").is_some(), "the change shows as it is now");
        harness.list_files(cx, &["src/lib.rs", "src/util.rs"])?;
        assert!(harness.bounds(cx, "browse-file:src/lib.rs").is_some(), "the browsed file is revealed");
        assert!(harness.bounds(cx, "browse-file:src/util.rs").is_some());

        harness.click(cx, "files-mode-diff")?;

        assert!(harness.bounds(cx, "diff-files").is_some());
        assert!(harness.bounds(cx, "source-mode-header").is_none());
        assert_eq!(harness.selected_line(cx), diffed);
        Ok(())
    });
}

#[gpui::test]
fn a_clean_repository_is_browsed_from_a_folded_tree_of_every_file(cx: &mut TestAppContext) {
    let clean =
        DiffViewerHarnessBuilder { document: DocumentBuilder::new().build(), ..DiffViewerHarnessBuilder::default() };
    clean.with_lookups().run(cx, |harness, cx| {
        assert!(harness.bounds(cx, "browse-files-status").is_some(), "files load after the repository is listed");

        harness.list_files(cx, &["src/util.rs", "README.md"])?;
        assert!(harness.bounds(cx, "browse-file:README.md").is_some());
        assert!(harness.bounds(cx, "browse-file:src/util.rs").is_none(), "folders start folded");
        harness.click(cx, "browse-directory:src")?;
        harness.click(cx, "browse-file:src/util.rs")?;

        let lookup = harness.last_lookup(cx).ok_or("the source is requested")?;
        assert_eq!(lookup.request, LookupRequest::Source(repo("src/util.rs")?));
        harness.answer_lookup(cx, lookup, Ok(LookupAnswer::Source(source(UTIL_SOURCE))));
        assert_eq!(harness.selected_line(cx), Some(location("src/util.rs", 1)?));
        assert!(harness.bounds(cx, "source-mode-header").is_some());
        Ok(())
    });
}

const STD: &str = "std · alloc/src/sync.rs";
const UTIL_SOURCE: &str = "pub fn helper() -> u32 {\n    1\n}\n";
const STD_SOURCE: &str = "// std\n// sync\npub struct Arc;\n";

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
