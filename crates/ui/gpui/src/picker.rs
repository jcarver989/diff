use super::{
    components::{Backdrop, ListRow, SelectionState, SurfaceExt},
    icon::{Icon, IconName},
    theme::UiTheme,
    tokens::{self, rem},
    typography::{Typography, TypographyExt},
};
use clankerdiff_theme::{Appearance, ReviewTheme, ThemeChoice, ThemeSelection};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, IntoElement, KeyBinding, NoAction,
    ParentElement, Rems, RenderOnce, ScrollHandle, SharedString, Subscription, Window, div,
    prelude::*, relative,
};
use gpui_base::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use std::{cmp::Reverse, rc::Rc, sync::Arc};

type ChooseHandler = Rc<dyn Fn(&usize, &mut Window, &mut App) + 'static>;

pub const CONTEXT: &str = "SearchPicker";

#[must_use]
pub fn fuzzy_filter<T: AsRef<str>>(query: &str, keys: impl IntoIterator<Item = T>) -> Vec<usize> {
    let keys = keys.into_iter();
    if query.trim().is_empty() {
        return (0..keys.count()).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, usize)> = keys
        .enumerate()
        .filter_map(|(index, key)| {
            pattern
                .score(Utf32Str::new(key.as_ref(), &mut buf), &mut matcher)
                .map(|score| (score, index))
        })
        .collect();
    scored.sort_by_key(|&(score, _)| Reverse(score));
    scored.into_iter().map(|(_, index)| index).collect()
}

#[derive(Debug, Clone)]
pub struct PickerItem {
    id: SharedString,
    label: SharedString,
    icon: Option<IconName>,
    group: Option<SharedString>,
    detail: Option<SharedString>,
    keywords: Option<SharedString>,
    disabled: bool,
    checked: bool,
}

impl PickerItem {
    #[must_use]
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            group: None,
            detail: None,
            keywords: None,
            disabled: false,
            checked: false,
        }
    }

    #[must_use]
    pub const fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn keywords(mut self, keywords: impl Into<SharedString>) -> Self {
        self.keywords = Some(keywords.into());
        self
    }

    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[must_use]
    pub const fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    fn match_text(&self) -> String {
        [
            Some(&self.label),
            self.detail.as_ref(),
            self.keywords.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(SharedString::as_ref)
        .collect::<Vec<&str>>()
        .join(" ")
    }
}

#[derive(IntoElement)]
pub struct PickerList {
    id: SharedString,
    theme: UiTheme,
    items: Vec<PickerItem>,
    highlighted: Option<usize>,
    empty: SharedString,
    max_height: Option<Rems>,
    scroll: Option<ScrollHandle>,
    on_choose: Option<ChooseHandler>,
}

impl PickerList {
    #[must_use]
    pub fn new(id: impl Into<SharedString>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            theme,
            items: Vec::new(),
            highlighted: None,
            empty: "No matches".into(),
            max_height: None,
            scroll: None,
            on_choose: None,
        }
    }

    #[must_use]
    pub fn items(mut self, items: impl IntoIterator<Item = PickerItem>) -> Self {
        self.items.extend(items);
        self
    }

    #[must_use]
    pub const fn highlighted(mut self, position: Option<usize>) -> Self {
        self.highlighted = position;
        self
    }

    #[must_use]
    pub fn empty(mut self, message: impl Into<SharedString>) -> Self {
        self.empty = message.into();
        self
    }

    #[must_use]
    pub const fn max_height(mut self, height: Rems) -> Self {
        self.max_height = Some(height);
        self
    }

    #[must_use]
    pub fn track_scroll(mut self, scroll: &ScrollHandle) -> Self {
        self.scroll = Some(scroll.clone());
        self
    }

    #[must_use]
    pub fn on_choose(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_choose = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for PickerList {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.theme.colors;
        if self.items.is_empty() {
            return div()
                .px_3()
                .py_2()
                .typography(Typography::Small)
                .text_color(colors.text_muted)
                .child(self.empty)
                .into_any_element();
        }
        let id = self.id;
        let theme = self.theme;
        let mut previous_group = None;
        let mut rows = Vec::new();
        for (position, item) in self.items.into_iter().enumerate() {
            if item.group.is_some() && item.group != previous_group {
                previous_group.clone_from(&item.group);
                let group = item.group.clone().unwrap_or_default();
                let selector = format!("{id}-group-{group}");
                rows.push(
                    div()
                        .debug_selector(move || selector)
                        .px_2()
                        .pt_2()
                        .pb_1()
                        .typography(Typography::Caption)
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(colors.text_muted)
                        .child(group)
                        .into_any_element(),
                );
            }
            let state = if item.disabled {
                SelectionState::Disabled
            } else if self.highlighted == Some(position) {
                SelectionState::Selected
            } else {
                SelectionState::None
            };
            let on_choose = self.on_choose.clone().filter(|_| !item.disabled);
            rows.push(
                ListRow::new(format!("{id}-{}", item.id), theme)
                    .state(state)
                    .when_some(on_choose, |row, choose| {
                        row.on_click(move |_, window, cx| choose(&position, window, cx))
                    })
                    .children(item.icon.map(|icon| Icon::new(icon, colors.text_muted)))
                    .child(div().flex_1().min_w_0().truncate().child(item.label))
                    .children(item.detail.map(|detail| {
                        div()
                            .flex_none()
                            .max_w(relative(0.5))
                            .truncate()
                            .typography(Typography::Small)
                            .text_color(colors.text_muted)
                            .child(detail)
                    }))
                    .when(item.checked, |row| {
                        row.child(Icon::new(IconName::Check, colors.text_muted))
                    })
                    .into_any_element(),
            );
        }
        let list = format!("{id}-list");
        div()
            .id(SharedString::from(list.clone()))
            .debug_selector(move || list)
            .min_h_0()
            .when_some(self.max_height, gpui::Styled::max_h)
            .overflow_y_scroll()
            .when_some(self.scroll, |list, scroll| list.track_scroll(&scroll))
            .p_1()
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchPickerEvent {
    Query(String),
    Highlighted(usize),
    Chose(usize),
    Dismissed,
}

pub struct SearchPicker {
    id: SharedString,
    theme: UiTheme,
    title: Option<SharedString>,
    footer: Option<SharedString>,
    empty: SharedString,
    status: Option<SharedString>,
    search: Option<(Entity<InputState>, Subscription)>,
    items: Vec<PickerItem>,
    matches: Vec<usize>,
    highlighted: usize,
    filtered: bool,
    width: Option<Rems>,
    height: Option<Rems>,
    max_list_height: Option<Rems>,
    focus_handle: FocusHandle,
    needs_focus: bool,
    scroll: ScrollHandle,
}

impl SearchPicker {
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("space", NoAction, Some(CONTEXT)),
            KeyBinding::new("up", MoveUp, Some(CONTEXT)),
            KeyBinding::new("down", MoveDown, Some(CONTEXT)),
            KeyBinding::new(
                "enter",
                Enter {
                    secondary: false,
                    shift: false,
                },
                Some(CONTEXT),
            ),
            KeyBinding::new("escape", Escape, Some(CONTEXT)),
        ]);
    }

    #[must_use]
    pub fn new(
        id: impl Into<SharedString>,
        items: Vec<PickerItem>,
        theme: UiTheme,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut picker = Self {
            id: id.into(),
            theme,
            title: None,
            footer: None,
            empty: "No matches".into(),
            status: None,
            search: None,
            items,
            matches: Vec::new(),
            highlighted: 0,
            filtered: true,
            width: None,
            height: None,
            max_list_height: None,
            focus_handle: cx.focus_handle(),
            needs_focus: true,
            scroll: ScrollHandle::new(),
        };
        picker.refilter("");
        picker
    }

    #[must_use]
    pub fn searchable(
        mut self,
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let style = self.theme.editor_style();
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(placeholder.into());
            input.set_editor_style(style);
            input
        });
        let subscription = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().to_string();
                this.apply_query(&query, cx);
                if !this.filtered {
                    cx.emit(SearchPickerEvent::Query(query));
                }
            }
        });
        self.search = Some((input, subscription));
        self
    }

    #[must_use]
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[must_use]
    pub fn footer(mut self, footer: impl Into<SharedString>) -> Self {
        self.footer = Some(footer.into());
        self
    }

    #[must_use]
    pub fn empty(mut self, message: impl Into<SharedString>) -> Self {
        self.empty = message.into();
        self
    }

    #[must_use]
    pub fn unfiltered(mut self) -> Self {
        self.filtered = false;
        self
    }

    #[must_use]
    pub const fn width(mut self, width: Rems) -> Self {
        self.width = Some(width);
        self
    }

    #[must_use]
    pub const fn height(mut self, height: Rems) -> Self {
        self.height = Some(height);
        self
    }

    #[must_use]
    pub const fn max_list_height(mut self, height: Rems) -> Self {
        self.max_list_height = Some(height);
        self
    }

    #[must_use]
    pub fn highlight(mut self, index: usize) -> Self {
        if let Some(position) = self.matches.iter().position(|&matched| matched == index) {
            self.highlighted = position;
            self.scroll.scroll_to_item(position);
        }
        self
    }

    pub fn set_items(&mut self, items: Vec<PickerItem>, cx: &mut Context<Self>) {
        self.items = items;
        let query = self.query(cx);
        self.apply_query(&query, cx);
    }

    pub fn move_highlight(&mut self, by: isize, cx: &mut Context<Self>) {
        if let Some(next) = self
            .highlighted
            .checked_add_signed(by)
            .and_then(|start| self.next_enabled(start, by))
        {
            self.set_highlighted_position(next, cx);
        }
    }

    pub fn highlight_item(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(position) = self
            .matches
            .iter()
            .position(|&matched| matched == index)
            .filter(|_| !self.items[index].disabled)
        {
            self.set_highlighted_position(position, cx);
        }
    }

    pub fn choose_highlighted(&mut self, cx: &mut Context<Self>) {
        self.choose(self.highlighted, cx);
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(SearchPickerEvent::Dismissed);
    }

    pub fn set_status(&mut self, status: Option<SharedString>, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    #[must_use]
    pub fn query(&self, cx: &App) -> String {
        self.search
            .as_ref()
            .map(|(input, _)| input.read(cx).value().to_string())
            .unwrap_or_default()
    }

    fn refilter(&mut self, query: &str) {
        self.matches = if self.filtered {
            fuzzy_filter(query, self.items.iter().map(PickerItem::match_text))
        } else {
            (0..self.items.len()).collect()
        };
        let items = &self.items;
        self.matches.sort_by_key(|&index| {
            items[index].group.as_ref().map(|group| {
                items
                    .iter()
                    .position(|item| item.group.as_ref() == Some(group))
            })
        });
        self.highlighted = self.next_enabled(0, 1).unwrap_or(0);
        self.scroll.scroll_to_item(self.highlighted);
    }

    fn apply_query(&mut self, query: &str, cx: &mut Context<Self>) {
        let before = self.highlighted_item();
        self.refilter(query);
        if let Some(index) = self
            .highlighted_item()
            .filter(|&index| Some(index) != before)
        {
            cx.emit(SearchPickerEvent::Highlighted(index));
        }
        cx.notify();
    }

    fn set_highlighted_position(&mut self, position: usize, cx: &mut Context<Self>) {
        let before = self.highlighted_item();
        self.highlighted = position;
        self.scroll.scroll_to_item(position);
        if let Some(index) = self
            .highlighted_item()
            .filter(|&index| Some(index) != before)
        {
            cx.emit(SearchPickerEvent::Highlighted(index));
        }
        cx.notify();
    }

    fn highlighted_item(&self) -> Option<usize> {
        self.matches
            .get(self.highlighted)
            .copied()
            .filter(|&index| !self.items[index].disabled)
    }

    fn next_enabled(&self, start: usize, by: isize) -> Option<usize> {
        let mut position = start;
        loop {
            if !self.items[*self.matches.get(position)?].disabled {
                return Some(position);
            }
            position = position.checked_add_signed(by)?;
        }
    }

    fn choose(&mut self, position: usize, cx: &mut Context<Self>) {
        if let Some(&index) = self
            .matches
            .get(position)
            .filter(|&&index| !self.items[index].disabled)
        {
            cx.emit(SearchPickerEvent::Chose(index));
        }
    }
}

impl EventEmitter<SearchPickerEvent> for SearchPicker {}

impl Render for SearchPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.needs_focus) {
            let focus = self.focus_handle.clone();
            let input = self.search.as_ref().map(|(input, _)| input.clone());
            window.defer(cx, move |window, cx| match input {
                Some(input) => input.update(cx, |input, cx| input.focus(window, cx)),
                None => focus.focus(window, cx),
            });
        }
        let id = self.id.clone();
        let colors = self.theme.colors;
        let search = self.search.as_ref().map(|(input, _)| {
            let selector = format!("{id}-search");
            div()
                .debug_selector(move || selector)
                .flex_none()
                .h(rem(tokens::BAR_HEIGHT))
                .px_3()
                .flex()
                .items_center()
                .gap_2()
                .border_b_1()
                .border_color(colors.border)
                .child(Icon::new(IconName::Search, colors.text_muted))
                .child(div().flex_1().min_w_0().child(Input::new(input)))
        });
        let body = if let Some(status) = self.status.clone() {
            PickerList::new(id.clone(), self.theme).empty(status)
        } else {
            PickerList::new(id.clone(), self.theme)
                .items(self.matches.iter().map(|&index| self.items[index].clone()))
                .highlighted(Some(self.highlighted))
                .empty(self.empty.clone())
                .track_scroll(&self.scroll)
                .on_choose(cx.listener(|this, position: &usize, _, cx| this.choose(*position, cx)))
        };
        div()
            .id(id.clone())
            .debug_selector(move || id.to_string())
            .track_focus(&self.focus_handle)
            .key_context(CONTEXT)
            .when_some(self.width, gpui::Styled::w)
            .max_w_full()
            .when_some(self.height, gpui::Styled::h)
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .surface(self.theme)
            .shadow_lg()
            .typography(Typography::Body)
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                this.move_highlight(-1, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                this.move_highlight(1, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &Enter, _, cx| this.choose_highlighted(cx)))
            .on_action(cx.listener(|this, _: &Escape, _, cx| this.dismiss(cx)))
            .children(self.title.clone().map(|title| {
                div()
                    .flex_none()
                    .px_3()
                    .pt_2()
                    .pb_1()
                    .typography(Typography::Label)
                    .text_color(colors.text_muted)
                    .child(title)
            }))
            .children(search)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .when_some(self.max_list_height, gpui::Styled::max_h)
                    .flex()
                    .flex_col()
                    .child(body),
            )
            .children(self.footer.clone().map(|footer| {
                div()
                    .flex_none()
                    .h(rem(tokens::GROUP_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(colors.border)
                    .typography(Typography::Caption)
                    .text_color(colors.text_muted)
                    .child(footer)
            }))
    }
}

#[derive(Debug, Clone)]
pub enum ThemePickerEvent {
    Preview(ReviewTheme),
    Commit(ReviewTheme),
    Cancel(ReviewTheme),
}

pub struct ThemePicker {
    picker: Entity<SearchPicker>,
    selection: Option<ThemeSelection>,
    _subscription: Subscription,
}

impl ThemePicker {
    #[must_use]
    pub fn new(
        id: impl Into<SharedString>,
        current: &ReviewTheme,
        themes: Arc<[ThemeChoice]>,
        theme: UiTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let selection = ThemeSelection::new(current, themes);
        let items = selection
            .iter()
            .flat_map(ThemeSelection::themes)
            .map(|choice| {
                let detail = match choice.theme.appearance() {
                    Appearance::Light => "Light",
                    Appearance::Dark => "Dark",
                };
                PickerItem::new(choice.theme.id().to_string(), choice.name.clone())
                    .detail(detail)
                    .checked(choice.theme.id() == current.id())
            })
            .collect();
        let selected = selection.as_ref().map_or(0, ThemeSelection::selected);
        let id = id.into();
        let picker = cx.new(|cx| {
            SearchPicker::new(id, items, theme, cx)
                .searchable("Search themes…", window, cx)
                .empty("No matching themes")
                .footer("↑↓ Preview    ↵ Apply    Esc Cancel")
                .width(rem(tokens::PICKER_WIDTH))
                .max_list_height(rem(tokens::PICKER_LIST_HEIGHT))
                .highlight(selected)
        });
        let subscription = cx.subscribe(&picker, |this, _, event: &SearchPickerEvent, cx| {
            this.on_picker_event(event, cx);
        });
        Self {
            picker,
            selection,
            _subscription: subscription,
        }
    }

    pub fn move_highlight(&mut self, by: isize, cx: &mut Context<Self>) {
        self.picker
            .update(cx, |picker, cx| picker.move_highlight(by, cx));
    }

    pub fn highlight(&mut self, index: usize, cx: &mut Context<Self>) {
        self.picker
            .update(cx, |picker, cx| picker.highlight_item(index, cx));
    }

    pub fn commit(&mut self, cx: &mut Context<Self>) {
        self.picker.update(cx, SearchPicker::choose_highlighted);
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.picker.update(cx, SearchPicker::dismiss);
    }

    fn on_picker_event(&mut self, event: &SearchPickerEvent, cx: &mut Context<Self>) {
        match event {
            SearchPickerEvent::Highlighted(index) => {
                if let Some(theme) = self
                    .selection
                    .as_mut()
                    .and_then(|selection| selection.select(*index))
                {
                    cx.emit(ThemePickerEvent::Preview(theme));
                }
            }
            SearchPickerEvent::Chose(index) => {
                if let Some(mut selection) = self.selection.take() {
                    selection.select(*index);
                    cx.emit(ThemePickerEvent::Commit(selection.commit()));
                }
            }
            SearchPickerEvent::Dismissed => {
                if let Some(selection) = self.selection.take() {
                    cx.emit(ThemePickerEvent::Cancel(selection.cancel()));
                }
            }
            SearchPickerEvent::Query(_) => {}
        }
    }
}

impl EventEmitter<ThemePickerEvent> for ThemePicker {}

pub struct ThemePickerOverlay {
    picker: Entity<ThemePicker>,
    _subscription: Subscription,
}

impl ThemePickerOverlay {
    pub fn open<T: 'static>(
        current: &ReviewTheme,
        themes: Arc<[ThemeChoice]>,
        theme: UiTheme,
        window: &mut Window,
        cx: &mut Context<T>,
        on_event: impl Fn(&mut T, &ThemePickerEvent, &mut Window, &mut Context<T>) + 'static,
    ) -> Self {
        let picker =
            cx.new(|cx| ThemePicker::new("theme-picker", current, themes, theme, window, cx));
        let subscription = cx.subscribe_in(&picker, window, move |this, _, event, window, cx| {
            on_event(this, event, window, cx);
        });
        Self {
            picker,
            _subscription: subscription,
        }
    }

    #[must_use]
    pub const fn picker(&self) -> &Entity<ThemePicker> {
        &self.picker
    }

    #[must_use]
    pub fn render(&self, theme: UiTheme) -> Backdrop {
        let picker = self.picker.clone();
        Backdrop::new("theme-picker-backdrop", theme)
            .on_dismiss(move |_, cx| picker.update(cx, ThemePicker::cancel))
            .child(self.picker.clone())
    }
}

impl Render for ThemePicker {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.picker.clone()
    }
}
