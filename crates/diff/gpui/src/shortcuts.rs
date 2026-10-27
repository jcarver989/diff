use crate::{
    DiffViewer,
    ui::prelude::{CODE_FONT, Modal, ModalSize, Typography, TypographyExt, UiTheme, rem},
};
use gpui::{App, Window, div, prelude::*};

const NAVIGATION: Shortcuts = &[
    ("j / k, ↑ / ↓", "move selection"),
    ("h / l, ← / →", "change pane, fold, or select side"),
    ("Tab", "change pane"),
    ("b", "hide / show changed files"),
    ("g / G, Home / End", "first / last item"),
    ("PgUp / PgDn", "move a page"),
    ("[ / ]", "previous / next commit"),
    ("{ / }", "first commit / uncommitted changes"),
    ("p", "pick commit (↑ / ↓, Enter, Esc)"),
    ("o", "toggle source / diff"),
    ("Enter", "expand context / open file"),
    ("O", "expand all context"),
    ("f", "toggle all diff context"),
    ("z / Z", "collapse / expand file or all files"),
    ("⌘/Ctrl-hover, Shift-D", "show definition (Enter go, Tab next, Esc close)"),
    ("⌘/Ctrl-click", "go to definition"),
    ("Ctrl-O / Ctrl-I", "back / forward after a jump"),
];
const GIT: Shortcuts = &[
    ("Space", "stage / unstage file or directory"),
    ("a / A", "stage / unstage all"),
    ("C", "commit staged changes"),
    ("d", "discard selected file"),
    ("S", "cycle scope (unstaged/staged/both)"),
    ("⌘/Ctrl-R", "refresh repository"),
];
const REVIEW: Shortcuts = &[
    ("c / e / x", "add / edit / delete comment"),
    ("u", "undo last comment"),
    ("s / y", "submit / copy review"),
    ("v", "toggle split / unified"),
    ("V", "auto layout"),
    ("t", "choose theme (↑ / ↓, Enter, Esc)"),
    ("Esc / Ctrl-G", "cancel review"),
];
const DRAFT: Shortcuts = &[("Enter", "save comment"), ("Shift-Enter", "insert newline"), ("Esc", "cancel comment")];

pub(crate) type Shortcuts = &'static [(&'static str, &'static str)];

impl DiffViewer {
    pub(crate) fn render_shortcuts(&self) -> ShortcutHelp {
        ShortcutHelp::new(&self.ui_theme())
            .section("Navigation", NAVIGATION)
            .section("Review", REVIEW)
            .section("Git", GIT)
            .section("Comment editor", DRAFT)
    }
}

#[derive(IntoElement)]
pub(crate) struct ShortcutHelp {
    theme: UiTheme,
    sections: Vec<(&'static str, Shortcuts)>,
}

impl ShortcutHelp {
    pub(crate) const fn new(theme: &UiTheme) -> Self {
        Self { theme: *theme, sections: Vec::new() }
    }

    pub(crate) fn section(mut self, title: &'static str, shortcuts: Shortcuts) -> Self {
        self.sections.push((title, shortcuts));
        self
    }
}

impl RenderOnce for ShortcutHelp {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        let section = |title: &'static str, rows: Shortcuts| {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().mb_1().typography(Typography::Label).text_color(colors.accent).child(title))
                .children(rows.iter().map(|(keys, description)| {
                    div()
                        .flex()
                        .gap_3()
                        .child(
                            div()
                                .w(rem(120.0))
                                .flex_shrink_0()
                                .font_family(CODE_FONT)
                                .text_color(colors.text)
                                .child(*keys),
                        )
                        .child(div().text_color(colors.text_muted).child(*description))
                }))
        };
        Modal::new("shortcut-help", "Review shortcuts", self.theme)
            .hint("? / Esc to close")
            .size(ModalSize::Wide)
            .children(self.sections.chunks(2).enumerate().map(|(index, pair)| {
                div()
                    .when(index > 0, Styled::mt_4)
                    .flex()
                    .gap_5()
                    .children(pair.iter().map(|(title, rows)| section(title, rows)))
            }))
    }
}
