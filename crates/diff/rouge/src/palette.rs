use crate::{ReviewHandle, shortcut_label};
use clankerdiff_core::{CommandContext, InteractionPhase, palette_commands};
use clankerdiff_rouge_ui::picker::{PickerItem, SearchPickerEvent, SearchPickerHandle, search_picker};
use rouge_core::{Div, view};

pub(crate) fn help_picker(handle: &ReviewHandle) -> (SearchPickerHandle, Div) {
    let handle = *handle;
    let entries: Vec<_> = handle.with(|state| {
        let context = CommandContext { phase: InteractionPhase::Browse, ..state.command_context() };
        palette_commands(&context, state.session()).collect()
    });
    let items = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let item = PickerItem::new(format!("command-{index}"), entry.title).group(entry.section.title());
            match shortcut_label(&entry.command) {
                Some(label) => item.detail(label),
                None => item,
            }
        })
        .collect();
    let picker = SearchPickerHandle::new(items, move |event| match event {
        SearchPickerEvent::Chose(index) => {
            if let Some(entry) = entries.get(index) {
                handle.choose_palette_command(entry.command.clone());
            }
        }
        SearchPickerEvent::Dismissed => {
            handle.dismiss_picker(InteractionPhase::Help);
        }
        _ => (),
    });
    let palette = view! {
        <search_picker id="command-palette" handle=picker title="Commands" searchable="Search commands…"/>
    };
    (picker, palette)
}
