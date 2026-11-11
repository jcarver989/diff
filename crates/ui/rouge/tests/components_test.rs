use clankerdiff_rouge_ui::{
    prelude::*,
    testing::{TestError, TestResult, component_test},
};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Key, Styled, div, text};
use rouge_reactive::{Runtime, signal};

#[test]
fn full_width_buttons_rows_and_segments_activate_only_enabled_choices() -> TestResult {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, calls) = component_test().size(440.0, 320.0).build_with(|| {
        let theme = ActiveTheme::install(ReviewTheme::default()).ui();
        let calls = signal(Vec::new());
        let view = Surface::new(theme)
            .w(340.0)
            .padded(true)
            .raised(true)
            .selected(true)
            .child(
                Button::new("primary", "Primary", theme)
                    .variant(ButtonVariant::Primary)
                    .full_width()
                    .keyboard_focusable()
                    .on_click(move || calls.update(|v| v.push(1))),
            )
            .child(
                ListRow::new("row", theme)
                    .depth(2)
                    .selected(true)
                    .on_click(move || calls.update(|v| v.push(2)))
                    .child(text("A row")),
            )
            .child(
                SegmentedControl::new("segments", theme).segments([
                    Segment::new("enabled", "Enabled").selected(true).on_click(move || calls.update(|v| v.push(3))),
                    Segment::icon("disabled", IconName::Check, "Disabled")
                        .detail("Extra")
                        .disabled(true)
                        .on_click(move || calls.update(|v| v.push(4))),
                ]),
            );
        (view, calls)
    });
    let button = ui.bounds_of("primary").ok_or_else(|| TestError::NoTestId("primary".into()))?;
    assert!(button.size.width > 300.0);
    ui.click_on("primary")?;
    ui.click_on("row")?;
    let detail = ui.center_of("disabled-detail")?;
    let disabled = ui.bounds_of("disabled").ok_or_else(|| TestError::NoTestId("disabled".into()))?;
    assert!(disabled.contains(detail));
    ui.click_on("disabled")?;
    ui.click_on("enabled")?;
    ui.press(Key::Space);
    assert_eq!(calls.get(), vec![1, 2, 3, 3]);
    Ok(())
}

#[test]
fn section_action_and_disclosure_do_not_trigger_each_other() -> TestResult {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, counts) = component_test().size(440.0, 320.0).build_with(|| {
        let theme = ActiveTheme::install(ReviewTheme::default()).ui();
        let counts = signal((0, 0));
        let header = SectionHeader::new("section", "Section", true, theme)
            .detail("Details")
            .badge(Badge::new("badge", "3", theme).tone(NoticeTone::Info).icon(IconName::Check))
            .action(
                IconButton::new("action", IconName::Plus, "Add", theme).on_click(move || counts.update(|v| v.0 += 1)),
            )
            .actions_always_visible(true)
            .on_toggle(move || counts.update(|v| v.1 += 1));
        (
            div()
                .child(header)
                .child(EmptyState::new(IconName::File, "No files", "Nothing to review", theme))
                .child(ActionBar::new(theme).child(Notification::error("Not saved", theme))),
            counts,
        )
    });
    ui.click_on("action")?;
    assert_eq!(counts.get(), (1, 0));
    ui.click_on("section-toggle")?;
    assert_eq!(counts.get(), (1, 1));
    assert!(ui.bounds_of("badge").is_some());
    Ok(())
}

#[test]
fn window_drag_exclusion_keeps_nested_controls_out_of_window_gestures() {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    let mut ui =
        component_test().build(|| div().size_full().window_drag_area().child(div().w(20.0).h(20.0).no_window_drag()));
    ui.click_at(10.0, 10.0);
    assert!(ui.take_window_commands().is_empty());
    ui.click_at(60.0, 10.0);
    assert!(matches!(ui.take_window_commands().as_slice(), [rouge_core::WindowCommand::StartDrag]));
}

#[test]
fn checkboxes_keep_their_size_and_toggle_without_activating_their_row() -> TestResult {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, counts) = component_test().size(440.0, 320.0).build_with(|| {
        let theme = ActiveTheme::install(ReviewTheme::default()).ui();
        let counts = signal((0, 0));
        let view = ListRow::new("row", theme)
            .on_click(move || counts.update(|v| v.1 += 1))
            .child(text("file.rs"))
            .child(
                Checkbox::new("check", CheckState::Indeterminate, theme).on_toggle(move || counts.update(|v| v.0 += 1)),
            )
            .child(Checkbox::new("disabled", CheckState::Checked, theme).disabled(true).on_toggle(move || {
                counts.update(|v| v.0 += 10);
            }));
        (view, counts)
    });
    let bounds = ui.bounds_of("check").ok_or_else(|| TestError::NoTestId("check".into()))?;
    assert_eq!((bounds.size.width, bounds.size.height), (12.0, 12.0));
    ui.click_on("check")?;
    assert_eq!(counts.get(), (1, 0));
    ui.press(Key::Space);
    assert_eq!(counts.get(), (2, 0));
    ui.click_on("disabled")?;
    assert_eq!(counts.get(), (2, 1));
    Ok(())
}
