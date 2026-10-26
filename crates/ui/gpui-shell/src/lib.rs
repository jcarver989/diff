pub mod menus;
pub mod persist;
pub mod run_loop;
pub mod runtime;
#[cfg(feature = "script")]
pub mod script;

use clankerdiff_gpui_ui::fonts::load_fonts;
use gpui::App;

pub fn launch(init: impl FnOnce(&mut App) + 'static) {
    gpui_platform::application().run(move |cx| {
        load_fonts(cx).expect("failed to load the bundled fonts");
        run_loop::install(cx);
        init(cx);
        cx.activate(true);
    });
}
