use clankerdiff_rouge_ui::{fonts::bundled_fonts, prelude::*};
use clankerdiff_ui_core::{ReviewTheme, ThemeChoice};
use rouge::{App, AppError, WindowOptions};
use rouge_core::{Styled, View, div, show, text};
use rouge_reactive::{Signal, signal};

fn main() -> Result<(), AppError> {
    App::new()
        .fonts(bundled_fonts())
        .window(WindowOptions::default().title("Diff shared component gallery").size(920.0, 680.0), gallery)
        .run()
}

fn gallery() -> impl View {
    let active = ActiveTheme::install(ReviewTheme::default());
    let size = InterfaceSize::install(16.0);
    let _indicators = IndicatorLayer::install();
    let theme = active.ui();
    let themes_open = signal(false);
    let dialog_open = signal(false);
    let status = signal(String::from("Choose a control"));
    let edit = InlineEdit::input("Editable text");
    let width = signal(16.0);
    let sidebar = ResizablePanel::new("sidebar", PanelSide::Left, width).min_width(10.0).max_width(28.0).child(
        Surface::new(theme)
            .size_full()
            .child(SectionHeader::new("files", "Files", true, theme).badge(Badge::new("count", "2", theme)))
            .child(TreeRow::new("src", "src", theme).expanded(true).icon(IconName::FolderOpen))
            .child(
                TreeRow::new("lib", "lib.rs", theme)
                    .depth(1)
                    .icon(IconName::File)
                    .selected(true)
                    .on_click(move || status.set("Selected lib.rs".into())),
            ),
    );
    let main = Surface::new(theme)
        .padded(true)
        .raised(true)
        .flex_1()
        .min_w(0.0)
        .gap(rem(10.0))
        .child(SegmentedControl::new("view", theme).segments([
            Segment::new("unified", "Unified").selected(true),
            Segment::icon("split", IconName::Columns2, "Split"),
            Segment::new("disabled-segment", "Disabled").disabled(true),
        ]))
        .child(Notification::new("Review theme tokens and bundled typefaces", NoticeTone::Info, theme))
        .child(InlineEdit::new(edit, theme).on_commit(move |value| status.set(value.to_owned())))
        .child(EmptyState::new(IconName::FileDiff, "No changes", "The working tree is clean", theme))
        .child(
            ActionBar::new(theme)
                .child(text(move || status.get()).typography(Typography::Caption).color(theme.paint(|c| c.text)))
                .child(Waveform::new("activity", theme.paint(|c| c.accent))),
        );
    size.bind(
        div()
            .size_full()
            .flex()
            .col()
            .p(rem(16.0))
            .gap(rem(12.0))
            .bg(theme.paint(|c| c.canvas))
            .child(text("Shared Rouge components").typography(Typography::Heading).color(theme.paint(|c| c.text)))
            .child(controls(theme, size, themes_open, dialog_open, status))
            .child(div().flex().gap(rem(8.0)).flex_1().child(sidebar).child(main))
            .child(show(
                move || themes_open.get(),
                move || {
                    ThemePickerOverlay::new(
                        ThemePicker::new("themes", &active.review(), ThemeChoice::catalog(), theme, move |event| {
                            match event {
                                ThemePickerEvent::Preview(review) => active.set(review),
                                ThemePickerEvent::Commit(review) | ThemePickerEvent::Cancel(review) => {
                                    active.set(review);
                                    themes_open.set(false);
                                }
                            }
                        }),
                        theme,
                    )
                },
            ))
            .child(show(
                move || dialog_open.get(),
                move || {
                    ConfirmDialog::new(
                        "confirm",
                        "Confirm action",
                        "This dialog uses the shared modal and action bar.",
                        "Confirm",
                        theme,
                        move |event| {
                            status.set(format!("{event:?}"));
                            dialog_open.set(false);
                        },
                    )
                    .alternate("Alternate")
                },
            )),
    )
}

fn controls(
    theme: UiTheme,
    size: InterfaceSize,
    themes_open: Signal<bool>,
    dialog_open: Signal<bool>,
    status: Signal<String>,
) -> impl View {
    div()
        .flex()
        .gap(rem(8.0))
        .child(
            Button::new("primary", "Theme picker", theme)
                .variant(ButtonVariant::Primary)
                .keyboard_focusable()
                .on_click(move || themes_open.set(true)),
        )
        .child(
            Button::new("secondary", "Confirm dialog", theme)
                .variant(ButtonVariant::Secondary)
                .keyboard_focusable()
                .on_click(move || dialog_open.set(true)),
        )
        .child(
            Button::new("destructive", "Destructive", theme)
                .variant(ButtonVariant::Destructive)
                .on_click(move || status.set("Destructive selected".into())),
        )
        .child(Button::new("disabled", "Disabled", theme).disabled(true))
        .child(with_menu(
            Button::new("menu", "Interface size", theme),
            [
                MenuItem::new("Larger", move || size.set(size.get() + 1.0)).icon(IconName::Plus),
                MenuItem::new("Smaller", move || size.set(size.get() - 1.0)).icon(IconName::Minus),
                MenuItem::separator(),
                MenuItem::new("Reset", move || size.set(16.0)),
            ],
            theme,
        ))
}
