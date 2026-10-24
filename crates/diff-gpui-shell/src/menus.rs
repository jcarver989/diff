use crate::run_loop;
use gpui::{
    App, KeyBinding, Menu, MenuItem, OsAction, PromptLevel, SystemMenuType, Window, actions,
};
use gpui_base::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};

actions!(
    app_menu,
    [About, Hide, HideOthers, Minimize, Quit, ShowAll, Zoom]
);

#[cfg(target_os = "macos")]
const QUIT_KEYSTROKE: &str = "cmd-q";
#[cfg(not(target_os = "macos"))]
const QUIT_KEYSTROKE: &str = "ctrl-q";

pub fn install(
    cx: &mut App,
    name: &'static str,
    version: &'static str,
    app_items: Vec<MenuItem>,
    menus: Vec<Menu>,
) {
    cx.on_action(move |_: &About, cx| {
        with_active_window(cx, |window, cx| {
            let version = format!("Version {version}");
            drop(window.prompt(PromptLevel::Info, name, Some(&version), &["OK"], cx));
        });
    });
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &Minimize, cx| with_active_window(cx, |window, _| window.minimize_window()));
    cx.on_action(|_: &Quit, cx| run_loop::stop(cx));
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Zoom, cx| with_active_window(cx, |window, _| window.zoom_window()));
    cx.bind_keys([KeyBinding::new(QUIT_KEYSTROKE, Quit, None)]);
    cx.set_menus(build(name, app_items, menus));
}

#[must_use]
pub fn build(name: &str, app_items: Vec<MenuItem>, menus: Vec<Menu>) -> Vec<Menu> {
    let mut app = vec![MenuItem::action(format!("About {name}"), About)];
    if !app_items.is_empty() {
        app.push(MenuItem::separator());
        app.extend(app_items);
    }
    app.extend([
        MenuItem::separator(),
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action(format!("Hide {name}"), Hide),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::action("Show All", ShowAll),
        MenuItem::separator(),
        MenuItem::action(format!("Quit {name}"), Quit),
    ]);
    let mut all = vec![
        Menu {
            name: name.to_owned().into(),
            disabled: false,
            items: app,
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
    ];
    all.extend(menus);
    all.push(Menu {
        name: "Window".into(),
        disabled: false,
        items: vec![
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", Zoom),
        ],
    });
    all
}

fn with_active_window(cx: &mut App, update: impl FnOnce(&mut Window, &mut App)) {
    if let Some(window) = cx.active_window() {
        let _ = window.update(cx, |_, window, cx| update(window, cx));
    }
}
