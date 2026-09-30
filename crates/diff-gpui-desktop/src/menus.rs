use crate::run_loop;
use clankerdiff_core::DiffScope;
use gpui::{Action, App, KeyBinding, Menu, MenuItem, OsAction, actions};
use gpui_base::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use serde::Deserialize;

actions!(
    desktop_menu,
    [About, Hide, HideOthers, Minimize, Quit, ShowAll, Zoom]
);

#[cfg(target_os = "macos")]
const QUIT_KEYSTROKE: &str = "cmd-q";
#[cfg(not(target_os = "macos"))]
const QUIT_KEYSTROKE: &str = "ctrl-q";

fn quit(_: &Quit, cx: &mut App) {
    run_loop::stop(cx);
}

fn hide(_: &Hide, cx: &mut App) {
    cx.hide();
}

fn hide_others(_: &HideOthers, cx: &mut App) {
    cx.hide_other_apps();
}

fn show_all(_: &ShowAll, cx: &mut App) {
    cx.unhide_other_apps();
}

pub(crate) fn app_name() -> &'static str {
    "ClankerDiff"
}

pub(crate) fn register_actions(cx: &mut App) {
    cx.on_action(quit);
    cx.on_action(hide);
    cx.on_action(hide_others);
    cx.on_action(show_all);
    cx.on_action(|_: &SetScope, _: &mut App| {});
}

pub(crate) fn build() -> Vec<Menu> {
    vec![
        Menu {
            name: app_name().into(),
            disabled: false,
            items: vec![
                MenuItem::action(format!("About {}", app_name()), About),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(format!("Hide {}", app_name()), Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action(format!("Quit {}", app_name()), Quit),
            ],
        },
        Menu {
            name: "Edit".into(),
            disabled: false,
            items: vec![
                MenuItem::os_action("Undo", Undo, OsAction::Undo),
                MenuItem::os_action("Redo", Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", Cut, OsAction::Cut),
                MenuItem::os_action("Copy", Copy, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
            ],
        },
        Menu {
            name: "View".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Unstaged", SetScope::unstaged()),
                MenuItem::action("Staged", SetScope::staged()),
                MenuItem::action("Both", SetScope::both()),
            ],
        },
        Menu {
            name: "Window".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
            ],
        },
    ]
}

pub(crate) fn install(cx: &mut App) {
    register_actions(cx);
    cx.bind_keys([KeyBinding::new(QUIT_KEYSTROKE, Quit, None)]);
    cx.set_menus(build());
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
        Self {
            kind: ScopeKind::Unstaged,
        }
    }

    fn staged() -> Self {
        Self {
            kind: ScopeKind::Staged,
        }
    }

    fn both() -> Self {
        Self {
            kind: ScopeKind::Both,
        }
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
