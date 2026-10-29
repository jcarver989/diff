use crate::ui::prelude::{CODE_FONT, Modal, ModalSize, Typography, TypographyExt, UiTheme, rem};
use gpui::{App, Window, div, prelude::*};

pub(crate) type Shortcuts = &'static [(&'static str, &'static str)];

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
