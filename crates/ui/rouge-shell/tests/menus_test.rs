use clankerdiff_rouge_shell::{
    menus,
    rouge::core::{Menu, MenuItem},
};

#[test]
fn commands_list_every_action_item_including_submenus() -> Result<(), Box<dyn std::error::Error>> {
    let menus = [Menu::new(
        "App",
        vec![
            MenuItem::action("About", 1_u8),
            MenuItem::Separator,
            MenuItem::Submenu(Menu::new("Window", vec![MenuItem::action("Minimize", 2_u8)])),
            MenuItem::callback("Quit", || {}),
        ],
    )];
    let commands = menus::commands(&menus);
    assert_eq!(commands.iter().map(|(label, _)| label.as_str()).collect::<Vec<_>>(), ["About", "Minimize", "Quit"]);
    let MenuItem::Action { action, .. } = &commands[1].1 else { return Err("missing nested action".into()) };
    assert_eq!(action.downcast_ref::<u8>(), Some(&2));
    Ok(())
}
