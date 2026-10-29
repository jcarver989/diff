use crate::{
    DiffViewer,
    comment_editor::init_input,
    ui::prelude::{PickerItem, SearchPicker},
    viewer::{Overlay, PickerOverlay},
};
use clankerdiff_core::{CommandContext, InteractionPhase, palette_commands};
use gpui::{Context, Window, prelude::*};

impl DiffViewer {
    pub(crate) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        init_input(cx);
        let context = CommandContext { phase: InteractionPhase::Browse, ..self.command_context() };
        let entries: Vec<_> = palette_commands(&context, self.session()).collect();
        let items = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let item = PickerItem::new(format!("command-{index}"), entry.title).group(entry.section.title());
                match entry.key_label() {
                    Some(keys) => item.detail(keys),
                    None => item,
                }
            })
            .collect();
        let theme = self.ui_theme();
        let picker = cx.new(|cx| {
            SearchPicker::new("command-palette", items, theme, cx).title("Commands").searchable(
                "Search commands…",
                window,
                cx,
            )
        });
        let overlay =
            PickerOverlay::new(picker, window, cx, move |index| entries.get(index).map(|entry| entry.command.clone()));
        self.overlay = Some(Overlay::Commands(overlay));
    }
}
