pub mod menus;
pub mod persist;
pub mod runtime;
#[cfg(feature = "script")]
pub mod script;

pub use rouge;

pub fn launch(app: rouge::App) -> Result<(), rouge::AppError> {
    app.fonts(clankerdiff_rouge_ui::fonts::bundled_fonts()).run()
}
