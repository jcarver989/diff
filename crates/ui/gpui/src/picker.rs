use super::{
    components::{Backdrop, ListRow, SelectionState, SurfaceExt},
    fonts::CODE_FONT,
    icon::{Icon, IconName},
    theme::{UiColors, UiTheme},
    tokens::{self, rem},
    typography::{Typography, TypographyExt},
};
use clankerdiff_ui_core::{Appearance, PickerOption, PickerState, ReviewTheme, ThemeChoice, ThemeSelection};
use gpui::{
    AnyElement, App, Context, Div, Entity, EventEmitter, FocusHandle, Hsla, IntoElement, KeyBinding, NoAction,
    ParentElement, Rems, RenderOnce, ScrollHandle, SharedString, Subscription, Window, div, prelude::*, relative,
};
use gpui_base::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};
use std::{rc::Rc, sync::Arc};

type PositionHandler = Rc<dyn Fn(&usize, &mut Window, &mut App) + 'static>;
type PreviewRenderer = Rc<dyn Fn(usize, &mut Window, &mut App) -> AnyElement + 'static>;

const MARKER_SIZE: f32 = 7.0;
const PENDING_MARKER_SIZE: f32 = 8.0;

pub const CONTEXT: &str = "SearchPicker";

#[derive(Debug, Clone)]
pub struct PickerDetail {
    text: SharedString,
    color: Option<Hsla>,
    code: bool,
}

impl PickerDetail {
    #[must_use]
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into(), color: None, code: false }
    }

    #[must_use]
    pub const fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    #[must_use]
    pub const fn code(mut self) -> Self {
        self.code = true;
        self
    }
}

impl From<&'static str> for PickerDetail {
    fn from(text: &'static str) -> Self {
        Self::new(text)
    }
}

impl From<String> for PickerDetail {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<SharedString> for PickerDetail {
    fn from(text: SharedString) -> Self {
        Self::new(text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PickerMarker {
    Dot(Hsla),
    Pending,
}

#[derive(Debug, Clone)]
pub struct PickerItem {
    id: SharedString,
    label: SharedString,
    marker: Option<PickerMarker>,
    icon: Option<IconName>,
    group: Option<SharedString>,
    prefix: Option<SharedString>,
    subtitle: Option<SharedString>,
    details: Vec<PickerDetail>,
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
            marker: None,
            icon: None,
            group: None,
            prefix: None,
            subtitle: None,
            details: Vec::new(),
            keywords: None,
            disabled: false,
            checked: false,
        }
    }

    #[must_use]
    pub const fn marker(mut self, marker: PickerMarker) -> Self {
        self.marker = Some(marker);
        self
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
    pub fn prefix(mut self, prefix: impl Into<SharedString>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<PickerDetail>) -> Self {
        self.details.push(detail.into());
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

    fn option(&self) -> PickerOption {
        let text = [self.prefix.as_ref(), Some(&self.label), self.subtitle.as_ref()]
            .into_iter()
            .flatten()
            .chain(self.details.iter().map(|detail| &detail.text))
            .chain(self.keywords.as_ref())
            .map(SharedString::as_ref)
            .collect::<Vec<&str>>()
            .join(" ");
        let option = PickerOption::new(text).disabled(self.disabled);
        match &self.group {
            Some(group) => option.group(group.to_string()),
            None => option,
        }
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
    on_choose: Option<PositionHandler>,
    on_hover: Option<PositionHandler>,
    checkable: bool,
    widest_prefix: Option<SharedString>,
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
            on_hover: None,
            checkable: false,
            widest_prefix: None,
        }
    }

    #[must_use]
    pub fn items(mut self, items: impl IntoIterator<Item = PickerItem>) -> Self {
        let items: Vec<PickerItem> = items.into_iter().collect();
        self = self.align_with(&items);
        self.items.extend(items);
        self
    }

    #[must_use]
    pub fn align_with<'a>(mut self, items: impl IntoIterator<Item = &'a PickerItem>) -> Self {
        for item in items {
            self.checkable |= item.checked;
            if let Some(prefix) = &item.prefix
                && self.widest_prefix.as_ref().is_none_or(|widest| widest.chars().count() < prefix.chars().count())
            {
                self.widest_prefix = Some(prefix.clone());
            }
        }
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

    #[must_use]
    pub fn on_hover(mut self, handler: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_hover = Some(Rc::new(handler));
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
        let checkable = self.checkable;
        let widest_prefix = self.widest_prefix;
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
            let on_hover = self.on_hover.clone().filter(|_| !item.disabled);
            let label_selector = format!("{id}-{}-label", item.id);
            let subtitle_selector = format!("{id}-{}-subtitle", item.id);
            rows.push(
                ListRow::new(format!("{id}-{}", item.id), theme)
                    .state(state)
                    .when_some(on_choose, |row, choose| {
                        row.on_click(move |_, window, cx| choose(&position, window, cx))
                    })
                    .when_some(on_hover, |row, hover| {
                        row.on_mouse_move(move |_, window, cx| hover(&position, window, cx))
                    })
                    .children(
                        item.marker.map(|marker| marker_slot(marker, format!("{id}-{}-marker", item.id), &colors)),
                    )
                    .children(item.icon.map(|icon| Icon::new(icon, colors.text_muted)))
                    .children(widest_prefix.clone().map(|widest| prefix_slot(widest, item.prefix, &colors)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .when(item.subtitle.is_some(), gpui::Styled::py_1)
                            .flex()
                            .flex_col()
                            .child(div().debug_selector(move || label_selector).truncate().child(item.label))
                            .children(item.subtitle.map(|subtitle| {
                                div()
                                    .debug_selector(move || subtitle_selector)
                                    .truncate()
                                    .typography(Typography::Small)
                                    .text_color(colors.text_muted)
                                    .child(subtitle)
                            })),
                    )
                    .when(!item.details.is_empty(), |row| row.child(details(item.details, &colors)))
                    .when(checkable, |row| {
                        row.child(check_slot(item.checked, format!("{id}-{}-check", item.id), &colors))
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
            .p(rem(tokens::FLOATING_INSET))
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element()
    }
}

fn check_slot(checked: bool, selector: String, colors: &UiColors) -> Div {
    div()
        .debug_selector(move || selector)
        .flex_none()
        .size(rem(tokens::ICON_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .when(checked, |slot| slot.child(Icon::new(IconName::Check, colors.accent).size(tokens::ICON_SIZE_SMALL)))
}

impl PickerMarker {
    #[must_use]
    pub fn dot(self, colors: &UiColors) -> Div {
        match self {
            Self::Dot(color) => div().flex_none().size(rem(MARKER_SIZE)).rounded_full().bg(color),
            Self::Pending => div()
                .flex_none()
                .size(rem(PENDING_MARKER_SIZE))
                .rounded_full()
                .border_1()
                .border_dashed()
                .border_color(colors.text_muted),
        }
    }
}

fn marker_slot(marker: PickerMarker, selector: String, colors: &UiColors) -> Div {
    div()
        .debug_selector(move || selector)
        .flex_none()
        .size(rem(tokens::ICON_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .child(marker.dot(colors))
}

fn prefix_slot(widest: SharedString, prefix: Option<SharedString>, colors: &UiColors) -> Div {
    div()
        .flex_none()
        .relative()
        .typography(Typography::Code)
        .text_color(colors.text_muted)
        .child(div().invisible().child(widest))
        .children(prefix.map(|prefix| div().absolute().top_0().left_0().child(prefix)))
}

fn details(details: Vec<PickerDetail>, colors: &UiColors) -> Div {
    div()
        .flex_none()
        .max_w(relative(0.5))
        .overflow_hidden()
        .whitespace_nowrap()
        .flex()
        .items_center()
        .gap_2()
        .typography(Typography::Small)
        .text_color(colors.text_muted)
        .children(details.into_iter().map(|detail| {
            div()
                .flex_none()
                .when(detail.code, |detail| detail.font_family(CODE_FONT))
                .when_some(detail.color, gpui::Styled::text_color)
                .child(detail.text)
        }))
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
    state: PickerState,
    width: Option<Rems>,
    height: Option<Rems>,
    max_list_height: Option<Rems>,
    preview: Option<PreviewRenderer>,
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
            KeyBinding::new("enter", Enter { secondary: false, shift: false }, Some(CONTEXT)),
            KeyBinding::new("escape", Escape, Some(CONTEXT)),
        ]);
    }

    #[must_use]
    pub fn new(id: impl Into<SharedString>, items: Vec<PickerItem>, theme: UiTheme, cx: &mut Context<Self>) -> Self {
        Self {
            id: id.into(),
            theme,
            title: None,
            footer: None,
            empty: "No matches".into(),
            status: None,
            search: None,
            state: PickerState::new(items.iter().map(PickerItem::option).collect()),
            items,
            width: None,
            height: None,
            max_list_height: None,
            preview: None,
            focus_handle: cx.focus_handle(),
            needs_focus: true,
            scroll: ScrollHandle::new(),
        }
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
                if !this.state.is_filtered() {
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
        self.state = self.state.unfiltered();
        self
    }

    #[must_use]
    pub fn max_matches(mut self, limit: usize) -> Self {
        self.state = self.state.limit(limit);
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
    pub fn preview(mut self, render: impl Fn(usize, &mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.preview = Some(Rc::new(render));
        self
    }

    #[must_use]
    pub fn highlight(mut self, index: usize) -> Self {
        if self.state.highlight(index) {
            self.scroll.scroll_to_item(self.state.highlighted_position());
        }
        self
    }

    pub fn set_items(&mut self, items: Vec<PickerItem>, cx: &mut Context<Self>) {
        let before = self.state.highlighted();
        self.state.set_options(items.iter().map(PickerItem::option).collect());
        self.items = items;
        self.highlight_changed(before, cx);
    }

    pub fn move_highlight(&mut self, by: isize, cx: &mut Context<Self>) {
        let before = self.state.highlighted();
        if self.state.move_by(by) {
            self.scroll.scroll_to_item(self.state.highlighted_position());
            self.highlight_changed(before, cx);
        }
    }

    pub fn highlight_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let before = self.state.highlighted();
        if self.state.highlight(index) {
            self.scroll.scroll_to_item(self.state.highlighted_position());
            self.highlight_changed(before, cx);
        }
    }

    pub fn choose_highlighted(&mut self, cx: &mut Context<Self>) {
        self.choose(self.state.highlighted_position(), cx);
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
        self.search.as_ref().map(|(input, _)| input.read(cx).value().to_string()).unwrap_or_default()
    }

    fn apply_query(&mut self, query: &str, cx: &mut Context<Self>) {
        let before = self.state.highlighted();
        self.state.set_query(query);
        self.scroll.scroll_to_item(self.state.highlighted_position());
        self.highlight_changed(before, cx);
    }

    fn hover(&mut self, position: usize, cx: &mut Context<Self>) {
        let before = self.state.highlighted();
        if position != self.state.highlighted_position() && self.state.highlight_position(position) {
            self.highlight_changed(before, cx);
        }
    }

    fn highlight_changed(&self, before: Option<usize>, cx: &mut Context<Self>) {
        if let Some(index) = self.state.highlighted().filter(|&index| Some(index) != before) {
            cx.emit(SearchPickerEvent::Highlighted(index));
        }
        cx.notify();
    }

    fn choose(&mut self, position: usize, cx: &mut Context<Self>) {
        if let Some(&index) = self.state.matches().get(position).filter(|&&index| !self.items[index].disabled) {
            cx.emit(SearchPickerEvent::Chose(index));
        }
    }
}

impl SearchPicker {
    fn render_content(&self, list: PickerList, preview: Option<AnyElement>) -> Div {
        let colors = self.theme.colors;
        let has_preview = preview.is_some();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .child(
                div()
                    .when(has_preview, |column| {
                        column
                            .flex_none()
                            .w(rem(tokens::PICKER_PREVIEW_LIST_WIDTH))
                            .border_r_1()
                            .border_color(colors.border_subtle)
                    })
                    .when(!has_preview, |column| column.flex_1().min_w_0())
                    .min_h_0()
                    .when_some(self.max_list_height, gpui::Styled::max_h)
                    .flex()
                    .flex_col()
                    .child(list),
            )
            .children(preview.map(|preview| {
                let selector = format!("{}-preview", self.id);
                div().debug_selector(move || selector).flex_1().min_w_0().min_h_0().flex().flex_col().child(preview)
            }))
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
                .border_color(colors.border_subtle)
                .child(Icon::new(IconName::Search, colors.text_muted))
                .child(div().flex_1().min_w_0().child(Input::new(input)))
        });
        let preview = self.preview.clone().map(|render| {
            self.state.highlighted().map_or_else(|| div().into_any_element(), |index| render(index, window, cx))
        });
        let body = if let Some(status) = self.status.clone() {
            PickerList::new(id.clone(), self.theme).empty(status)
        } else {
            PickerList::new(id.clone(), self.theme)
                .align_with(&self.items)
                .items(self.state.matches().iter().map(|&index| self.items[index].clone()))
                .highlighted(Some(self.state.highlighted_position()))
                .empty(self.empty.clone())
                .track_scroll(&self.scroll)
                .on_choose(cx.listener(|this, position: &usize, _, cx| this.choose(*position, cx)))
                .on_hover(cx.listener(|this, position: &usize, _, cx| this.hover(*position, cx)))
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
            .floating(self.theme)
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
            .child(self.render_content(body, preview))
            .children(self.footer.clone().map(|footer| {
                div()
                    .flex_none()
                    .h(rem(tokens::GROUP_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(colors.border_subtle)
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
        Self { picker, selection, _subscription: subscription }
    }

    pub fn move_highlight(&mut self, by: isize, cx: &mut Context<Self>) {
        self.picker.update(cx, |picker, cx| picker.move_highlight(by, cx));
    }

    pub fn highlight(&mut self, index: usize, cx: &mut Context<Self>) {
        self.picker.update(cx, |picker, cx| picker.highlight_item(index, cx));
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
                if let Some(theme) = self.selection.as_mut().and_then(|selection| selection.select(*index)) {
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
        let picker = cx.new(|cx| ThemePicker::new("theme-picker", current, themes, theme, window, cx));
        let subscription = cx.subscribe_in(&picker, window, move |this, _, event, window, cx| {
            on_event(this, event, window, cx);
        });
        Self { picker, _subscription: subscription }
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
