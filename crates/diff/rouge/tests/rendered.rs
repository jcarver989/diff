use clankerdiff_core::{DiffReviewCommand, ViewMode, testing::DocumentBuilder};
use clankerdiff_rouge::{
    ReviewController, diff_view,
    ui::{fonts::bundled_fonts, interface_size::InterfaceSize},
};
use clankerdiff_ui_core::ThemeChoice;
use rouge_core::testing::{TestResult, TestUiBuilder};
use rouge_reactive::Runtime;
use std::io;

#[test]
fn gpu_renders_diff_layout_theme_and_interface_size_changes() -> TestResult {
    let _runtime = Runtime::new().enter();
    let document =
        DocumentBuilder::new().renamed("old.rs", "new.rs", "fn old() {}\n", "fn new() {}").binary("binary.png").build();
    let (mut ui, (handle, size)) =
        TestUiBuilder::default().fonts(bundled_fonts()).size(1024.0, 768.0).build_with(|| {
            let size = InterfaceSize::install(16.0);
            let handle = ReviewController::new(document).mount(|_| {});
            (diff_view(handle), (handle, size))
        });
    assert!(handle.handle_command(DiffReviewCommand::SetViewMode(ViewMode::Unified)));
    ui.advance(0.0);
    let unified = ui.render()?;
    assert!(handle.handle_command(DiffReviewCommand::SetViewMode(ViewMode::Split)));
    ui.advance(0.0);
    let split = ui.render()?;
    assert!(unified.pixels != split.pixels, "split must paint differently from unified");
    let appearance = handle.theme().get().appearance();
    let theme = ThemeChoice::catalog()
        .iter()
        .find(|choice| choice.theme.appearance() != appearance)
        .ok_or_else(|| io::Error::other("no contrasting theme"))?
        .theme
        .clone();
    handle.set_theme(theme);
    ui.advance(0.0);
    let changed = ui.render()?;
    assert_ne!(split.pixel(800, 500), changed.pixel(800, 500));
    let before = ui.center_of("diff-file-header-0")?;
    size.set(22.0);
    ui.advance(0.0);
    let scaled = ui.render()?;
    assert!(changed.pixels != scaled.pixels, "scale change must repaint");
    assert!(ui.center_of("diff-file-header-0")?.y > before.y);
    Ok(())
}
