use clankerdiff_core::{DiffReviewCommand, DiffScope};
use clankerdiff_gpui::RunCommand;
use gpui::{App, Menu, MenuItem};

pub(crate) const APP_NAME: &str = "ClankerDiff";

pub(crate) fn install(cx: &mut App) {
    cx.on_action(|_: &RunCommand, _: &mut App| {});
    clankerdiff_gpui_shell::menus::install(cx, APP_NAME, env!("CARGO_PKG_VERSION"), Vec::new(), extra());
}

pub(crate) fn extra() -> Vec<Menu> {
    vec![Menu {
        name: "View".into(),
        disabled: false,
        items: vec![
            MenuItem::action("Unstaged", RunCommand(DiffReviewCommand::SetScope(DiffScope::Unstaged))),
            MenuItem::action("Staged", RunCommand(DiffReviewCommand::SetScope(DiffScope::Staged))),
            MenuItem::action("Both", RunCommand(DiffReviewCommand::SetScope(DiffScope::Both))),
            MenuItem::separator(),
            MenuItem::action("Previous Commit", RunCommand(DiffReviewCommand::StepTarget(-1))),
            MenuItem::action("Next Commit", RunCommand(DiffReviewCommand::StepTarget(1))),
        ],
    }]
}
