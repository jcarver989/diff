#![cfg(feature = "test-support")]

use clankerdiff_core::{
    ControlKind, DiffScope, ReviewSession, Toolbar, ToolbarControl, default_key_label,
    testing::{DocumentBuilder, commit_id, location},
};
use std::error::Error;

#[test]
fn a_diff_offers_file_history_layout_controls_and_totals_with_their_keys() {
    let session = ReviewSession::new(DocumentBuilder::new().changed("a.rs", "old\n", "new\n").build());
    let toolbar = Toolbar::new(&session, Some(false), false, default_key_label);
    assert_eq!(
        kinds(&toolbar.files),
        [ControlKind::Sidebar { hidden: false }, ControlKind::AllFiles { collapsed: false }]
    );
    assert_eq!(toolbar.files[0].tooltip, "Hide changed files (b)");
    assert_eq!(tooltips(&toolbar.history), ["Back (Ctrl-O / Esc)", "Forward"]);
    let selected: Vec<_> =
        toolbar.layouts.iter().filter(|control| control.selected).map(|control| control.kind).collect();
    assert_eq!(selected, [ControlKind::Layout(session.view_mode())]);
    assert_eq!(toolbar.stat, Some(session.presentation().document().line_stat()));
    assert!(toolbar.stepper.is_empty() && toolbar.page.is_none());
}

#[test]
fn without_navigation_or_in_source_view_the_toolbar_drops_what_does_not_apply() {
    let mut session = ReviewSession::new(DocumentBuilder::new().changed("a.rs", "old\n", "new\n").build());
    let toolbar = Toolbar::new(&session, None, false, |_| None);
    assert_eq!(kinds(&toolbar.files), [ControlKind::AllFiles { collapsed: false }]);
    assert_eq!(toolbar.files[0].tooltip, "Collapse all files");
    assert!(session.toggle_source_view());
    let toolbar = Toolbar::new(&session, Some(true), false, |_| None);
    assert_eq!(kinds(&toolbar.files), [ControlKind::Sidebar { hidden: true }]);
    assert_eq!(kinds(&toolbar.layouts), NONE);
    let empty = Toolbar::new(&ReviewSession::new(DocumentBuilder::new().build()), Some(false), false, |_| None);
    assert_eq!(kinds(&empty.history), NONE);
}

#[test]
fn commit_pages_offer_the_stepper_and_history_names_its_target() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new()
        .commit("Add parser")
        .commit("Wire the parser")
        .target(commit_id(2))
        .changed("src/lib.rs", "old\n", "new\nmore\n")
        .build();
    let mut session = ReviewSession::new(document);
    assert!(session.jump_to(location("src/lib.rs", 2)?));
    let toolbar = Toolbar::new(&session, Some(false), false, default_key_label);
    let stepper = [ControlKind::PreviousCommit, ControlKind::NextCommit, ControlKind::ChooseCommit];
    assert_eq!(kinds(&toolbar.stepper), stepper);
    assert_eq!(tooltips(&toolbar.stepper), ["Previous commit ([)", "Next commit (])", "Choose commit (p)"]);
    assert!(toolbar.page.is_some() && toolbar.scopes.is_empty());
    assert!(toolbar.history[0].tooltip.starts_with("Back to lib.rs:"), "{}", toolbar.history[0].tooltip);
    Ok(())
}

#[test]
fn a_working_tree_offers_every_scope_and_raises_its_own() {
    let session =
        ReviewSession::new(DocumentBuilder::new().target(DiffScope::Staged).changed("a.rs", "old\n", "new\n").build());
    let toolbar = Toolbar::new(&session, None, false, |_| None);
    let scopes: Vec<_> = toolbar.scopes.iter().map(|control| (control.kind, control.selected)).collect();
    let expected = DiffScope::ALL.map(|scope| (ControlKind::Scope(scope), scope == DiffScope::Staged));
    assert_eq!(scopes, expected);
}

const NONE: [ControlKind; 0] = [];

fn kinds(controls: &[ToolbarControl]) -> Vec<ControlKind> {
    controls.iter().map(|control| control.kind).collect()
}

fn tooltips(controls: &[ToolbarControl]) -> Vec<&str> {
    controls.iter().map(|control| control.tooltip.as_str()).collect()
}
