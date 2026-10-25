use clankerdiff_gpui_ui::{
    prelude::{InlineEdit, UiTheme},
    testing::ViewHarness,
};
use clankerdiff_theme::ReviewTheme;
use gpui::{
    AppContext as _, Context, Entity, EventEmitter, IntoElement, Render, TestAppContext, Window,
    WindowOptions,
};
use gpui_base::input::InputState;

#[gpui::test]
fn enter_commits_the_typed_value(cx: &mut TestAppContext) {
    let edit = edit(cx, "old");

    edit.simulate_input(cx, "new");
    edit.simulate_keystrokes(cx, "enter");

    assert_eq!(edit.events(cx), [Edit::Committed("new".to_owned())]);
}

#[gpui::test]
fn escape_cancels(cx: &mut TestAppContext) {
    let edit = edit(cx, "old");

    edit.simulate_keystrokes(cx, "escape");

    assert_eq!(edit.events(cx), [Edit::Cancelled]);
}

fn edit(cx: &mut TestAppContext, value: &str) -> ViewHarness<Editor, Edit> {
    cx.update(gpui_base::init);
    let value = value.to_owned();
    let edit = ViewHarness::open(cx, WindowOptions::default(), |window, cx| {
        let theme = UiTheme::new(&ReviewTheme::default());
        let input = InlineEdit::input(value, theme, window, cx);
        cx.new(|_| Editor { input, theme })
    });
    edit.draw(cx);
    edit
}

#[derive(Debug, Clone, PartialEq)]
enum Edit {
    Committed(String),
    Cancelled,
}

struct Editor {
    input: Entity<InputState>,
    theme: UiTheme,
}

impl EventEmitter<Edit> for Editor {}

impl Render for Editor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (commit, cancel) = (cx.entity().downgrade(), cx.entity().downgrade());
        InlineEdit::new(&self.input, self.theme)
            .on_commit(move |_, cx| {
                let _ = commit.update(cx, |editor, cx| {
                    let value = editor.input.read(cx).value().to_string();
                    cx.emit(Edit::Committed(value));
                });
            })
            .on_cancel(move |_, cx| {
                let _ = cancel.update(cx, |_, cx| cx.emit(Edit::Cancelled));
            })
    }
}
