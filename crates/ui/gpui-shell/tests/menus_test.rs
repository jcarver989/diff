use clankerdiff_gpui_shell::menus::{self, About, Minimize, Quit, Zoom};
use gpui::{Menu, MenuItem};

#[test]
fn commands_list_every_action_item_including_submenus() {
    let menus = [Menu {
        name: "App".into(),
        disabled: false,
        items: vec![
            MenuItem::action("About", About),
            MenuItem::separator(),
            MenuItem::submenu(Menu {
                name: "Window".into(),
                disabled: false,
                items: vec![MenuItem::action("Minimize", Minimize), MenuItem::action("Zoom", Zoom)],
            }),
            MenuItem::action("Quit", Quit),
        ],
    }];

    let commands = menus::commands(&menus);

    let labels: Vec<&str> = commands.iter().map(|(label, _)| label.as_ref()).collect();
    assert_eq!(labels, ["About", "Minimize", "Zoom", "Quit"]);
    assert!(commands[1].1.partial_eq(&Minimize));
}
