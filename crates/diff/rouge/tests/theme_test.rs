use clankerdiff_rouge::theme::{ActiveReviewTheme, ThemePickerEvent, theme_picker, theme_picker_overlay};
use clankerdiff_ui_core::{ReviewTheme, ThemeChoice};
use rouge_core::testing::TestResult;
use rouge_core::{Color, Styled, Theme, div, view};
use rouge_reactive::{Runtime, signal};
use rouge_widgets::testing::component_test;
use std::sync::Arc;

#[test]
fn theme_picker_previews_commits_and_cancels_against_original_theme() -> TestResult {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    let original = ReviewTheme::default();
    let other = ReviewTheme::builtin("ayu")?;
    for accept in [true, false] {
        let themes: Arc<[_]> =
            [ThemeChoice::new("Original", original.clone()), ThemeChoice::new("Other", other.clone())].into();
        let baseline = original.clone();
        let (mut ui, (active, events)) = component_test().size(720.0, 400.0).build_with(move || {
            let active = ActiveReviewTheme::install(baseline.clone());
            let events = signal(Vec::new());
            let handle = theme_picker(&baseline, themes, move |event| {
                let (name, theme) = match event {
                    ThemePickerEvent::Preview(theme) => ("preview", theme),
                    ThemePickerEvent::Commit(theme) => ("commit", theme),
                    ThemePickerEvent::Cancel(theme) => ("cancel", theme),
                };
                events.update(|events| events.push(name));
                active.set(theme);
            });
            (view! { <theme_picker_overlay id="themes" handle/> }, (active, events))
        });
        ui.type_text("Other");
        assert_eq!(active.review().revision(), other.revision());
        assert_eq!(active.ui().get(), other.ui.map(Color::from));
        ui.press(if accept { "enter" } else { "escape" });
        assert_eq!(events.get(), vec!["preview", if accept { "commit" } else { "cancel" }]);
        assert_eq!(active.review().revision(), if accept { other.revision() } else { original.revision() });
    }
    Ok(())
}

#[test]
fn setting_a_review_theme_updates_the_installed_ui_theme() -> TestResult {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    let next = ReviewTheme::builtin("ayu")?;
    let (mut ui, (active, kit)) = component_test().build_with(|| {
        let active = ActiveReviewTheme::install(ReviewTheme::default());
        (view! { <div size_full/> }, (active, Theme::current()))
    });
    active.set(next.clone());
    ui.advance(0.0);
    assert_eq!(kit.palette(), next.ui.map(Color::from));
    assert_eq!(active.terminal(), next.terminal_palette());
    Ok(())
}
