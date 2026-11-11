use rouge::core::{Chord, Key, Menu, MenuItem, Modifiers, window_handle};

#[must_use]
pub fn build(name: &str, app_items: Vec<MenuItem>, menus: Vec<Menu>) -> Vec<Menu> {
    let mut items = app_items;
    if !items.is_empty() {
        items.push(MenuItem::Separator);
    }
    items.push(
        MenuItem::callback(format!("Quit {name}"), rouge::App::quit)
            .shortcut(Chord::new(Key::character("q"), Modifiers::COMMAND)),
    );
    let mut all = vec![Menu::new(name, items), Menu::edit()];
    all.extend(menus);
    all.push(Menu::new(
        "Window",
        vec![
            MenuItem::callback("Minimize", || window_handle().minimize()),
            MenuItem::callback("Zoom", || window_handle().zoom()),
        ],
    ));
    all
}

#[must_use]
pub fn commands(menus: &[Menu]) -> Vec<(String, MenuItem)> {
    menus
        .iter()
        .flat_map(|menu| &menu.items)
        .flat_map(|item| match item {
            MenuItem::Action { title, .. } | MenuItem::Callback { title, .. } => vec![(title.clone(), item.clone())],
            MenuItem::Submenu(menu) => commands(std::slice::from_ref(menu)),
            MenuItem::Separator => Vec::new(),
        })
        .collect()
}
