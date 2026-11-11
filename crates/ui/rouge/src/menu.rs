use crate::{
    components::list_row,
    icon::{Icon, IconName},
    theme::UiTheme,
    tokens::MENU_WIDTH,
    typography::UiStyles,
};
use rouge_core::{Chord, Key, Styled, View, component, div, pct, px, text, view};
use rouge_reactive::{Callback, signal};
use rouge_widgets::RovingFocus;

#[derive(Clone)]
pub struct MenuItem(Option<Entry>);
#[derive(Clone)]
struct Entry {
    label: String,
    icon: Option<IconName>,
    shortcut: Option<Chord>,
    destructive: bool,
    action: Callback,
}
impl MenuItem {
    pub fn new(label: impl Into<String>, action: impl Fn() + 'static) -> Self {
        Self(Some(Entry { label: label.into(), icon: None, shortcut: None, destructive: false, action: action.into() }))
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

#[component]
pub fn with_menu(children: impl View, items: impl IntoIterator<Item = MenuItem>) -> impl View {
    let theme = UiTheme::current();
    let open = signal(false);
    let items = items.into_iter().collect::<Vec<_>>();
    let trigger = view! {
        <div focusable
             on_click={move |event| {
                 event.stop_propagation();
                 open.set(true);
             }}
             on_key_down={move |event| {
                 if matches!(event.key, Key::Enter | Key::Space | Key::ArrowDown) {
                     event.stop_propagation();
                     open.set(true);
                 }
             }}>
            {children}
        </div>
    };
    rouge_widgets::popover(open, trigger, move || {
        let focus = RovingFocus::new(items.iter().filter(|item| item.0.is_some()).count());
        let mut index = 0;
        let entries = items.iter().cloned().map(|item| {
            let Some(entry) = item.0 else {
                return view! { <div h=px(1.0) my=4.0 bg=theme.border/> };
            };
            let current = index;
            index += 1;
            let run = move || {
                open.set(false);
                entry.action.call();
            };
            let fg = if entry.destructive { theme.destructive } else { theme.text };
            let wrapper = view! {
                <div focusable
                     on_key_down={move |event| {
                         match event.key {
                             Key::Enter | Key::Space => run(),
                             Key::Escape | Key::Tab | Key::ArrowDown | Key::ArrowUp | Key::Home | Key::End => return,
                             _ => {}
                         }
                         event.stop_propagation();
                     }}>
                    <list_row id={format!("menu-item-{}", entry.label)} on_click=run>
                        {entry.icon.map(|icon| Icon::new(icon, fg))}
                        <text label color=fg>{entry.label}</text>
                        {entry.shortcut.map(|chord| view! { <text caption muted>{chord.display()}</text> })}
                    </list_row>
                </div>
            };
            focus.item(current, wrapper)
        });
        view! {
            <div w=MENU_WIDTH max_w=pct(100.0) flex col p=4.0>{..entries}</div>
        }
    })
}
