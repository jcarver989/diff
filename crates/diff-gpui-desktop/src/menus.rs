use gpui::{Action, App, Menu, MenuItem, OsAction, actions};
use serde::Deserialize;

actions!(
    desktop_menu,
    [About, Hide, HideOthers, Minimize, Quit, ShowAll, Zoom]
);

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
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
                MenuItem::os_action("Undo", NoopEdit::undo(), OsAction::Undo),
                MenuItem::os_action("Redo", NoopEdit::redo(), OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", NoopEdit::cut(), OsAction::Cut),
                MenuItem::os_action("Copy", NoopEdit::copy(), OsAction::Copy),
                MenuItem::os_action("Paste", NoopEdit::paste(), OsAction::Paste),
                MenuItem::os_action("Select All", NoopEdit::select_all(), OsAction::SelectAll),
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
    cx.set_menus(build());
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
enum EditKind {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

#[derive(Clone, PartialEq, Deserialize, schemars::JsonSchema, Action)]
#[allow(clippy::unsafe_derive_deserialize)]
#[action(namespace = desktop_menu)]
struct NoopEdit {
    kind: EditKind,
}

impl NoopEdit {
    fn undo() -> Self {
        Self {
            kind: EditKind::Undo,
        }
    }

    fn redo() -> Self {
        Self {
            kind: EditKind::Redo,
        }
    }

    fn cut() -> Self {
        Self {
            kind: EditKind::Cut,
        }
    }

    fn copy() -> Self {
        Self {
            kind: EditKind::Copy,
        }
    }

    fn paste() -> Self {
        Self {
            kind: EditKind::Paste,
        }
    }

    fn select_all() -> Self {
        Self {
            kind: EditKind::SelectAll,
        }
    }
}
