use clankerdiff_core::DiffScope;
use clankerdiff_gpui::{NextCommit, PreviousCommit};
use gpui::{Action, App, Menu, MenuItem};
use serde::Deserialize;

pub(crate) const APP_NAME: &str = "ClankerDiff";

pub(crate) fn install(cx: &mut App) {
    cx.on_action(|_: &SetScope, _: &mut App| {});
    cx.on_action(|_: &NextCommit, _: &mut App| {});
    cx.on_action(|_: &PreviousCommit, _: &mut App| {});
    clankerdiff_gpui_shell::menus::install(cx, APP_NAME, env!("CARGO_PKG_VERSION"), Vec::new(), extra());
}

pub(crate) fn extra() -> Vec<Menu> {
    vec![Menu {
        name: "View".into(),
        disabled: false,
        items: vec![
            MenuItem::action("Unstaged", SetScope::unstaged()),
            MenuItem::action("Staged", SetScope::staged()),
            MenuItem::action("Both", SetScope::both()),
            MenuItem::separator(),
            MenuItem::action("Previous Commit", PreviousCommit),
            MenuItem::action("Next Commit", NextCommit),
        ],
    }]
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
enum ScopeKind {
    Unstaged,
    Staged,
    Both,
}

#[derive(Clone, PartialEq, Deserialize, schemars::JsonSchema, Action)]
#[allow(clippy::unsafe_derive_deserialize)]
#[action(namespace = desktop_menu)]
pub struct SetScope {
    kind: ScopeKind,
}

impl SetScope {
    fn unstaged() -> Self {
        Self { kind: ScopeKind::Unstaged }
    }

    fn staged() -> Self {
        Self { kind: ScopeKind::Staged }
    }

    fn both() -> Self {
        Self { kind: ScopeKind::Both }
    }

    #[must_use]
    pub const fn scope(&self) -> DiffScope {
        match self.kind {
            ScopeKind::Unstaged => DiffScope::Unstaged,
            ScopeKind::Staged => DiffScope::Staged,
            ScopeKind::Both => DiffScope::Both,
        }
    }
}
