use diff_gpui::ui::prelude::*;
use diff_theme::DiffTheme;
use gpui::{
    App, AppContext, Bounds, Context, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    size,
};
use gpui_platform::application;

struct ComponentGallery;

impl Render for ComponentGallery {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = UiTheme::new(&DiffTheme::default());
        div()
            .size_full()
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            .bg(theme.colors.canvas)
            .text_color(theme.colors.text)
            .child(div().text_xl().child("Diff GPUI component gallery"))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(Button::new("primary", "Primary", theme).variant(ButtonVariant::Primary))
                    .child(
                        Button::new("secondary", "Secondary", theme)
                            .variant(ButtonVariant::Secondary),
                    )
                    .child(Button::new("ghost", "Ghost", theme))
                    .child(
                        Button::new("destructive", "Destructive", theme)
                            .variant(ButtonVariant::Destructive),
                    )
                    .child(Button::new("disabled", "Disabled", theme).disabled(true)),
            )
            .child(
                Surface::new(theme)
                    .raised(true)
                    .child("Raised surface")
                    .child(ListRow::new("row-default", theme).child("Default row"))
                    .child(
                        ListRow::new("row-selected", theme)
                            .state(SelectionState::Selected)
                            .child("Selected row"),
                    ),
            )
            .child(Notification::new(
                "Example error notification",
                NoticeTone::Error,
                theme,
            ))
            .child(
                ActionBar::new(theme)
                    .child("Action bar")
                    .child(div().flex_1())
                    .child(
                        Button::new("action", "Continue", theme).variant(ButtonVariant::Primary),
                    ),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(760.0), px(560.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|_| ComponentGallery),
        )
        .expect("component gallery window should open");
        cx.activate(true);
    });
}
