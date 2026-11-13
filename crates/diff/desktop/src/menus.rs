use clankerdiff_core::{DiffReviewCommand, DiffScope};
use clankerdiff_rouge::RunCommand;
use rouge::{Menu, MenuItem};

pub(crate) fn all() -> Vec<Menu> {
    vec![
        Menu::app("ClankerDiff", Vec::new()),
        Menu::new(
            "View",
            vec![
                MenuItem::action("Unstaged", RunCommand(DiffReviewCommand::SetScope(DiffScope::Unstaged))),
                MenuItem::action("Staged", RunCommand(DiffReviewCommand::SetScope(DiffScope::Staged))),
                MenuItem::action("Both", RunCommand(DiffReviewCommand::SetScope(DiffScope::Both))),
                MenuItem::Separator,
                MenuItem::action("Older Commit", RunCommand(DiffReviewCommand::StepTarget(-1))),
                MenuItem::action("Newer Commit", RunCommand(DiffReviewCommand::StepTarget(1))),
            ],
        ),
    ]
}
