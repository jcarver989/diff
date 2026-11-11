use crate::{
    components::ListRow,
    icon::{Icon, IconName},
    theme::UiTheme,
    tokens::{MENU_WIDTH, rem},
    typography::{Typography, TypographyExt},
};
use rouge_core::{Chord, Key, Styled, View, div, focus_handle, text};
use rouge_reactive::signal;
use std::rc::Rc;

#[derive(Clone)]
pub struct MenuItem(Option<Entry>);
#[derive(Clone)]
struct Entry {
    label: String,
    icon: Option<IconName>,
    shortcut: Option<Chord>,
    destructive: bool,
    action: Rc<dyn Fn()>,
}
impl MenuItem {
    pub fn new(label: impl Into<String>, action: impl Fn() + 'static) -> Self {
        Self(Some(Entry {
            label: label.into(),
            icon: None,
            shortcut: None,
            destructive: false,
            action: Rc::new(action),
        }))
    }
    #[must_use]
    pub const fn separator() -> Self {
        Self(None)
    }
    #[must_use]
    pub fn icon(mut self, icon: IconName) -> Self {
        if let Some(entry) = &mut self.0 {
            entry.icon = Some(icon);
        }
        self
    }
    #[must_use]
    pub fn shortcut(mut self, shortcut: Chord) -> Self {
        if let Some(entry) = &mut self.0 {
            entry.shortcut = Some(shortcut);
        }
        self
    }
    #[must_use]
    pub fn destructive(mut self) -> Self {
        if let Some(entry) = &mut self.0 {
            entry.destructive = true;
        }
        self
    }
}

pub fn with_menu(trigger: impl View, items: impl IntoIterator<Item = MenuItem>, theme: UiTheme) -> impl View {
    let open = signal(false);
    let items = items.into_iter().collect::<Vec<_>>();
    let trigger = div()
        .focusable()
        .child(trigger)
        .on_click(move |event| {
            event.stop_propagation();
            open.set(true);
        })
        .on_key_down(move |event| {
            if matches!(event.key, Key::Enter | Key::Space | Key::ArrowDown) {
                event.stop_propagation();
                open.set(true);
            }
        });
    rouge_widgets::popover(open, trigger, move || {
        let focus: Rc<[_]> = items.iter().filter(|item| item.0.is_some()).map(|_| focus_handle()).collect();
        let mut index = 0;
        div().w(rem(MENU_WIDTH)).max_w(rouge_core::pct(100.0)).flex().col().p(rem(4.0)).children(
            items.iter().cloned().map(|item| {
                let Some(entry) = item.0 else {
                    return div().h(1.0).my(rem(4.0)).bg(theme.paint(|c| c.border));
                };
                let current = index;
                index += 1;
                let handles = focus.clone();
                let action = entry.action.clone();
                let run = move || {
                    open.set(false);
                    action();
                };
                let fg = theme.paint(move |c| if entry.destructive { c.destructive } else { c.text });
                let mut row = ListRow::new(format!("menu-item-{}", entry.label), theme).on_click(run.clone());
                if let Some(icon) = entry.icon {
                    row = row.child(Icon::new(icon, fg));
                }
                row = row.child(text(entry.label).typography(Typography::Label).color(fg));
                if let Some(chord) = entry.shortcut {
                    row = row.child(
                        text(chord.display()).typography(Typography::Caption).color(theme.paint(|c| c.text_muted)),
                    );
                }
                let wrapper = div()
                    .track_focus(focus[current])
                    .focusable()
                    .on_key_down(move |event| {
                        let count = handles.len();
                        match event.key {
                            Key::ArrowDown => handles[(current + 1) % count].focus(),
                            Key::ArrowUp => handles[(current + count - 1) % count].focus(),
                            Key::Home => handles[0].focus(),
                            Key::End => handles[count - 1].focus(),
                            Key::Enter | Key::Space => run(),
                            Key::Escape | Key::Tab => return,
                            _ => {
                                event.stop_propagation();
                                return;
                            }
                        }
                        event.stop_propagation();
                    })
                    .child(row);
                if current == 0 { wrapper.autofocus() } else { wrapper }
            }),
        )
    })
}
