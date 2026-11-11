use crate::{
    components::{Backdrop, ListRow, MutedText, SelectionState, SurfaceExt},
    fonts::UI_FONT,
    icon::{Icon, IconName},
    theme::UiTheme,
    tokens::{self, rem},
    typography::{Typography, TypographyExt},
};
use clankerdiff_ui_core::{Appearance, PickerOption, PickerRow, PickerState, ReviewTheme, ThemeChoice, ThemeSelection};
use rouge_core::{
    AnyView, BuildCx, Color, ElementId, Key, Length, ScrollState, Scrollable, Styled, View, div, dynamic, each,
    editable, scroll, show, text,
};
use rouge_reactive::{Signal, effect, memo, signal};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[derive(Debug, Clone, PartialEq)]
pub struct PickerDetail {
    text: String,
    color: Option<Color>,
    code: bool,
}

impl PickerDetail {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), color: None, code: false }
    }

    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PickerMarker {
    Dot(Color),
    Pending,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PickerItem {
    id: String,
    label: String,
    marker: Option<PickerMarker>,
    icon: Option<IconName>,
    group: Option<String>,
    prefix: Option<String>,
    subtitle: Option<String>,
    details: Vec<PickerDetail>,
    keywords: Option<String>,
    disabled: bool,
    checked: bool,
}

impl PickerItem {
    #[must_use]
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
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
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    #[must_use]
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<PickerDetail>) -> Self {
        self.details.push(detail.into());
        self
    }

    #[must_use]
    pub fn keywords(mut self, keywords: impl Into<String>) -> Self {
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
            .map(String::as_str)
            .collect::<Vec<&str>>()
            .join(" ");
        let option = PickerOption::new(text).disabled(self.disabled);
        match &self.group {
            Some(group) => option.group(group.clone()),
            None => option,
        }
    }
}

pub struct PickerList {
    id: String,
    theme: UiTheme,
    items: Vec<PickerItem>,
    highlighted: Option<usize>,
    empty: String,
    height: Length,
    state: Option<Signal<ScrollState>>,
    choose: Rc<dyn Fn(usize)>,
    hover: Rc<dyn Fn(usize)>,
}
impl PickerList {
    pub fn new(id: impl Into<String>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            theme,
            items: Vec::new(),
            highlighted: None,
            empty: "No results".into(),
            height: rem(tokens::PICKER_LIST_HEIGHT),
            state: None,
            choose: Rc::new(|_| ()),
            hover: Rc::new(|_| ()),
        }
    }
    #[must_use]
    pub fn items(mut self, items: impl IntoIterator<Item = PickerItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }
    #[must_use]
    pub fn highlighted(mut self, position: Option<usize>) -> Self {
        self.highlighted = position;
        self
    }
    #[must_use]
    pub fn empty(mut self, message: impl Into<String>) -> Self {
        self.empty = message.into();
        self
    }
    #[must_use]
    pub fn max_height(mut self, height: Length) -> Self {
        self.height = height;
        self
    }
    #[must_use]
    pub fn track_scroll(mut self, state: Signal<ScrollState>) -> Self {
        self.state = Some(state);
        self
    }
    #[must_use]
    pub fn on_choose(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.choose = Rc::new(f);
        self
    }
    #[must_use]
    pub fn on_hover(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.hover = Rc::new(f);
        self
    }
}
impl View for PickerList {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let theme = self.theme;
        let state = self.state.unwrap_or_else(|| signal(ScrollState::default()));
        let picker = PickerState::new(self.items.iter().map(PickerItem::option).collect()).unfiltered();
        let columns = Columns::new(&self.items);
        let list = if self.items.is_empty() {
            div().child(MutedText::new(self.empty, theme))
        } else {
            div().flex().col().children(picker.rows().into_iter().map(|row| match row {
                PickerRow::Group(label) => group(label, theme).into_any(),
                PickerRow::Option { position, index } => {
                    let choose = self.choose.clone();
                    let hover = self.hover.clone();
                    let disabled = self.items[index].disabled;
                    item_row(self.items[index].clone(), theme, &columns)
                        .selected(self.highlighted == Some(position))
                        .disabled(self.items[index].disabled)
                        .on_click(move || choose(position))
                        .on_mouse_move(move |_| {
                            if !disabled {
                                hover(position);
                            }
                        })
                        .into_any()
                }
            }))
        };
        scroll().test_id(self.id).max_h(self.height).scroll_state(state).child(list).build(cx)
    }
}
fn group(label: &str, theme: UiTheme) -> impl View {
    div()
        .px(rem(8.0))
        .pt(rem(6.0))
        .child(text(label.to_owned()).typography(Typography::Caption).color(theme.paint(|c| c.text_muted)))
}
#[derive(Clone, PartialEq, Eq)]
struct Columns {
    marker: bool,
    checked: bool,
    prefix: Option<String>,
}

impl Columns {
    fn new(items: &[PickerItem]) -> Self {
        Self {
            marker: items.iter().any(|item| item.marker.is_some()),
            checked: items.iter().any(|item| item.checked),
            prefix: items
                .iter()
                .filter_map(|item| item.prefix.as_ref())
                .max_by_key(|prefix| prefix.chars().count())
                .cloned(),
        }
    }
}

fn item_row(item: PickerItem, theme: UiTheme, columns: &Columns) -> ListRow {
    let slot =
        || div().flex_none().w(rem(tokens::ICON_SIZE)).h(rem(tokens::ICON_SIZE)).flex().items_center().justify_center();
    let mut row = ListRow::new(item.id.clone(), theme);
    if columns.marker {
        let mut mark = slot().test_id(format!("{}-marker", item.id));
        mark = match item.marker {
            Some(PickerMarker::Dot(c)) => mark.child(div().w(rem(7.0)).h(rem(7.0)).rounded(tokens::scaled(4.0)).bg(c)),
            Some(PickerMarker::Pending) => mark.child(
                div()
                    .w(rem(10.0))
                    .h(rem(10.0))
                    .rounded(tokens::scaled(5.0))
                    .border(1.0)
                    .border_color(theme.paint(|c| c.text_muted)),
            ),
            None => mark,
        };
        row = row.child(mark);
    }
    if let Some(icon) = item.icon {
        row = row.child(Icon::new(icon, theme.paint(|c| c.text_secondary)));
    }
    if let Some(widest) = &columns.prefix {
        let code =
            |value: String| text(value).typography(Typography::Code).color(theme.paint(|c| c.text_muted)).no_wrap();
        let mut prefix =
            div().relative().flex_none().child(div().opacity(0.0).pass_through().child(code(widest.clone())));
        if let Some(value) = item.prefix {
            prefix = prefix.child(div().absolute().left(0.0).top(0.0).child(code(value)));
        }
        row = row.child(prefix);
    }
    let mut title = div().flex().col().flex_1().min_w(0.0).overflow_hidden().child(
        div().test_id(format!("{}-label", item.id)).child(text(item.label).typography(Typography::Body).no_wrap()),
    );
    if let Some(subtitle) = item.subtitle {
        title = title.py(rem(4.0)).child(
            div()
                .test_id(format!("{}-subtitle", item.id))
                .child(text(subtitle).typography(Typography::Small).color(theme.paint(|c| c.text_muted)).no_wrap()),
        );
    }
    row = row.child(title);
    if !item.details.is_empty() {
        row = row.child(div().flex().items_center().gap(rem(6.0)).children(item.details.into_iter().map(
            move |detail| {
                text(detail.text)
                    .typography(if detail.code { Typography::Code } else { Typography::Caption })
                    .color(move || detail.color.unwrap_or_else(|| theme.colors().text_muted))
                    .no_wrap()
            },
        )));
    }
    if columns.checked {
        let mut check = slot().test_id(format!("{}-check", item.id));
        if item.checked {
            check = check.child(Icon::new(IconName::Check, theme.paint(|c| c.accent)).size(12.0));
        }
        row = row.child(check);
    }
    row
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchPickerEvent {
    Query(String),
    Highlighted(usize),
    Chose(usize),
    Dismissed,
}

type Preview = Rc<dyn Fn(usize) -> AnyView>;
#[derive(Clone)]
pub struct SearchPickerHandle {
    items: Signal<Vec<PickerItem>>,
    model: Signal<PickerState>,
    query: Signal<String>,
    emit: Rc<dyn Fn(SearchPickerEvent)>,
}
impl SearchPickerHandle {
    pub fn set_items(&self, items: Vec<PickerItem>) {
        rouge_reactive::batch(|| {
            self.model.update(|model| model.set_options(items.iter().map(PickerItem::option).collect()));
            self.items.set(items);
        });
    }
    #[must_use]
    pub fn query(&self) -> Signal<String> {
        self.query
    }
    pub fn move_highlight(&self, by: isize) {
        self.update(|model| {
            model.move_by(by);
        });
    }
    pub fn highlight(&self, index: usize) {
        self.update(|model| {
            model.highlight(index);
        });
    }
    pub fn choose(&self) {
        if let Some(index) = self.model.with_untracked(PickerState::highlighted) {
            (self.emit)(SearchPickerEvent::Chose(index));
        }
    }
    pub fn dismiss(&self) {
        (self.emit)(SearchPickerEvent::Dismissed);
    }
    fn update(&self, f: impl FnOnce(&mut PickerState)) {
        let before = self.model.with_untracked(PickerState::highlighted);
        self.model.update(f);
        if let Some(after) = self.model.with_untracked(PickerState::highlighted).filter(|after| Some(*after) != before)
        {
            (self.emit)(SearchPickerEvent::Highlighted(after));
        }
    }
}

pub struct SearchPicker {
    id: String,
    theme: UiTheme,
    handle: SearchPickerHandle,
    placeholder: Option<String>,
    title: Option<String>,
    footer: Option<String>,
    empty: String,
    width: Length,
    height: Option<Length>,
    list_height: Length,
    preview: Option<Preview>,
    status: Signal<Option<String>>,
}
impl SearchPicker {
    pub fn new(
        id: impl Into<String>,
        items: Vec<PickerItem>,
        theme: UiTheme,
        emit: impl Fn(SearchPickerEvent) + 'static,
    ) -> Self {
        let model = signal(PickerState::new(items.iter().map(PickerItem::option).collect()));
        Self {
            id: id.into(),
            theme,
            handle: SearchPickerHandle {
                items: signal(items),
                model,
                query: signal(String::new()),
                emit: Rc::new(emit),
            },
            placeholder: None,
            title: None,
            footer: None,
            empty: "No matching items".into(),
            width: rem(tokens::PICKER_WIDTH),
            height: None,
            list_height: rem(tokens::PICKER_LIST_HEIGHT),
            preview: None,
            status: signal(None),
        }
    }
    #[must_use]
    pub fn handle(&self) -> SearchPickerHandle {
        self.handle.clone()
    }
    #[must_use]
    pub fn searchable(mut self, hint: impl Into<String>) -> Self {
        self.placeholder = Some(hint.into());
        self
    }
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
    #[must_use]
    pub fn footer(mut self, footer: impl Into<String>) -> Self {
        self.footer = Some(footer.into());
        self
    }
    #[must_use]
    pub fn empty(mut self, empty: impl Into<String>) -> Self {
        self.empty = empty.into();
        self
    }
    #[must_use]
    pub fn unfiltered(self) -> Self {
        self.handle.model.update(|model| *model = model.clone().unfiltered());
        self
    }
    #[must_use]
    pub fn max_matches(self, limit: usize) -> Self {
        self.handle.model.update(|model| *model = model.clone().limit(limit));
        self
    }
    #[must_use]
    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }
    #[must_use]
    pub fn height(mut self, height: Length) -> Self {
        self.height = Some(height);
        self
    }
    #[must_use]
    pub fn max_list_height(mut self, height: Length) -> Self {
        self.list_height = height;
        self
    }
    #[must_use]
    pub fn preview(mut self, render: impl Fn(usize) -> AnyView + 'static) -> Self {
        self.preview = Some(Rc::new(render));
        self
    }
    #[must_use]
    pub fn highlight(self, index: usize) -> Self {
        self.handle.model.update(|model| {
            model.highlight(index);
        });
        self
    }
    #[must_use]
    pub fn status(&self) -> Signal<Option<String>> {
        self.status
    }
}
#[derive(Clone, PartialEq)]
enum Row {
    Group(String),
    Item(usize, PickerItem),
}
impl View for SearchPicker {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, handle) = (self.theme, self.handle);
        let (query, model, items) = (handle.query, handle.model, handle.items);
        let querying = handle.clone();
        effect(move || {
            let next = query.get();
            if !model.with_untracked(|model| model.query() == next) {
                (querying.emit)(SearchPickerEvent::Query(next.clone()));
                querying.update(|model| model.set_query(next));
            }
        });
        let highlight = memo(move || model.with(PickerState::highlighted));
        let entries = memo(move || {
            model.with(|model| {
                items.with(|items| {
                    model
                        .rows()
                        .into_iter()
                        .filter_map(|row| match row {
                            PickerRow::Group(label) => Some(Row::Group(label.to_owned())),
                            PickerRow::Option { index, .. } => {
                                items.get(index).cloned().map(|item| Row::Item(index, item))
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
        });
        let commands = handle.clone();
        let id = self.id.clone();
        let mut root = div()
            .test_id(self.id)
            .flex()
            .col()
            .min_h(0.0)
            .w(self.width)
            .max_w(rouge_core::pct(100.0))
            .gap(rem(6.0))
            .p(rem(8.0))
            .floating(theme)
            .on_key_down(move |event| {
                match event.key {
                    Key::ArrowUp => commands.move_highlight(-1),
                    Key::ArrowDown => commands.move_highlight(1),
                    Key::PageUp => commands.move_highlight(-8),
                    Key::PageDown => commands.move_highlight(8),
                    Key::Enter => commands.choose(),
                    Key::Escape => commands.dismiss(),
                    _ => return,
                }
                event.stop_propagation();
            });
        if let Some(height) = self.height {
            root = root.h(height);
        }
        if let Some(title) = self.title {
            root = root.child(text(title).typography(Typography::Title).color(theme.paint(|c| c.text)));
        }
        root = if let Some(hint) = self.placeholder {
            root.child(query_input(hint, query, handle.clone(), theme))
        } else {
            root.autofocus().tab_index(-1)
        };
        let scroll_list = results(theme, handle, entries, highlight, self.empty, self.list_height);
        if let Some(preview) = self.preview {
            root = root.child(with_preview(&id, scroll_list, highlight, preview, theme));
        } else {
            root = root.child(scroll_list);
        }
        let status = self.status;
        root = root.child(show(
            move || status.with(Option::is_some),
            move || {
                text(move || status.get().unwrap_or_default())
                    .typography(Typography::Caption)
                    .color(theme.paint(|c| c.warning))
            },
        ));
        if let Some(footer) = self.footer {
            root = root.child(MutedText::new(footer, theme));
        }
        root.build(cx)
    }
}

fn with_preview(
    id: &str,
    list: impl View,
    highlight: rouge_reactive::Memo<Option<usize>>,
    preview: Preview,
    theme: UiTheme,
) -> impl View {
    div()
        .flex()
        .row()
        .flex_1()
        .min_h(0.0)
        .child(
            div()
                .flex_none()
                .w(rem(tokens::PICKER_PREVIEW_LIST_WIDTH))
                .max_w(rouge_core::pct(50.0))
                .min_h(0.0)
                .pr(rem(6.0))
                .border_r(1.0)
                .border_color(theme.paint(|c| c.border_subtle))
                .child(list),
        )
        .child(
            div()
                .test_id(format!("{id}-preview"))
                .flex_1()
                .min_w(0.0)
                .min_h(0.0)
                .flex()
                .col()
                .child(dynamic(move || highlight.get().map_or_else(|| div().into_any(), |index| preview(index)))),
        )
}

fn query_input(hint: String, query: Signal<String>, handle: SearchPickerHandle, theme: UiTheme) -> impl View {
    div()
        .relative()
        .p(rem(6.0))
        .surface(theme)
        .child(show(
            move || query.with(String::is_empty),
            move || div().absolute().inset(rem(6.0)).pass_through().child(MutedText::new(hint.clone(), theme)),
        ))
        .child(
            editable(query)
                .test_id("picker-query")
                .autofocus()
                .font(UI_FONT)
                .text_size(rem(13.0))
                .color(theme.paint(|c| c.text))
                .w_full()
                .on_submit(move |_| handle.choose()),
        )
}

fn results(
    theme: UiTheme,
    handle: SearchPickerHandle,
    entries: rouge_reactive::Memo<Vec<Row>>,
    highlight: rouge_reactive::Memo<Option<usize>>,
    empty: String,
    list_height: Length,
) -> impl View {
    let columns = memo(move || handle.items.with(|items| Columns::new(items)));
    let current = signal(None::<(usize, rouge_core::Rect)>);
    let scroll_state = signal(ScrollState::default());
    let bounds = signal(rouge_core::Rect::ZERO);
    let scroll_list = scroll()
        .test_id("picker-results")
        .track_bounds(bounds)
        .scroll_state(scroll_state)
        .max_h(list_height)
        .child(show(move || entries.with(Vec::is_empty), move || MutedText::new(empty.clone(), theme)))
        .child(each(
            move || entries.get().into_iter().enumerate().collect(),
            |(position, row)| match row {
                Row::Group(_) => (*position, None),
                Row::Item(i, item) => (*i, Some(item.id.clone())),
            },
            move |entry| {
                let choosing = handle.clone();
                dynamic(move || match entry.get().1 {
                    Row::Group(label) => group(&label, theme).into_any(),
                    Row::Item(index, item) => {
                        let disabled = item.disabled;
                        let row_bounds = signal(rouge_core::Rect::ZERO);
                        effect(move || {
                            if highlight.get() == Some(index) {
                                current.set_if_changed(Some((index, row_bounds.get())));
                            }
                        });
                        let hover = choosing.clone();
                        let click = choosing.clone();
                        div()
                            .track_bounds(row_bounds)
                            .child(
                                item_row(item, theme, &columns.get())
                                    .selection(move || {
                                        if disabled {
                                            SelectionState::Disabled
                                        } else if highlight.get() == Some(index) {
                                            SelectionState::Selected
                                        } else {
                                            SelectionState::None
                                        }
                                    })
                                    .disabled(disabled)
                                    .on_mouse_move(move |_| {
                                        if !disabled {
                                            hover.highlight(index);
                                        }
                                    })
                                    .on_click(move || {
                                        click.highlight(index);
                                        click.choose();
                                    }),
                            )
                            .into_any()
                    }
                })
            },
        ));
    let pending = signal(true);
    effect(move || {
        highlight.get();
        entries.with(|_| ());
        pending.set(true);
    });
    effect(move || {
        if !pending.get() {
            return;
        }
        if let Some((_, row)) =
            current.get().filter(|(index, row)| Some(*index) == highlight.get() && row.size.height > 0.0)
        {
            let viewport = bounds.get();
            if viewport.is_empty() {
                return;
            }
            pending.set(false);
            let delta = if row.origin.y < viewport.origin.y {
                row.origin.y - viewport.origin.y
            } else if row.size.height <= viewport.size.height && row.max().y > viewport.max().y {
                row.max().y - viewport.max().y
            } else {
                0.0
            };
            if delta != 0.0 {
                scroll_state.update(|state| {
                    state.offset.y += delta;
                });
            }
        }
    });
    scroll_list
}

#[derive(Debug, Clone)]
pub enum ThemePickerEvent {
    Preview(ReviewTheme),
    Commit(ReviewTheme),
    Cancel(ReviewTheme),
}
pub struct ThemePicker(SearchPicker);
impl ThemePicker {
    pub fn new(
        id: impl Into<String>,
        current: &ReviewTheme,
        themes: Arc<[ThemeChoice]>,
        theme: UiTheme,
        emit: impl Fn(ThemePickerEvent) + 'static,
    ) -> Self {
        let selection = ThemeSelection::new(current, themes);
        let items = selection
            .iter()
            .flat_map(ThemeSelection::themes)
            .map(|choice| {
                PickerItem::new(choice.theme.id().to_string(), choice.name.clone())
                    .detail(match choice.theme.appearance() {
                        Appearance::Light => "Light",
                        Appearance::Dark => "Dark",
                    })
                    .checked(choice.theme.id() == current.id())
            })
            .collect();
        let index = selection.as_ref().map_or(0, ThemeSelection::selected);
        let selection = RefCell::new(selection);
        Self(
            SearchPicker::new(id, items, theme, move |event| {
                let event = match event {
                    SearchPickerEvent::Highlighted(index) => {
                        selection.borrow_mut().as_mut().and_then(|s| s.select(index)).map(ThemePickerEvent::Preview)
                    }
                    SearchPickerEvent::Chose(index) => selection.borrow_mut().take().map(|mut s| {
                        s.select(index);
                        ThemePickerEvent::Commit(s.commit())
                    }),
                    SearchPickerEvent::Dismissed => {
                        selection.borrow_mut().take().map(|s| ThemePickerEvent::Cancel(s.cancel()))
                    }
                    SearchPickerEvent::Query(_) => None,
                };
                if let Some(event) = event {
                    emit(event);
                }
            })
            .searchable("Search themes…")
            .empty("No matching themes")
            .footer("↑↓ Preview    ↵ Apply    Esc Cancel")
            .highlight(index),
        )
    }
    #[must_use]
    pub fn handle(&self) -> SearchPickerHandle {
        self.0.handle()
    }
}
impl View for ThemePicker {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        self.0.build(cx)
    }
}
pub struct ThemePickerOverlay {
    picker: ThemePicker,
    theme: UiTheme,
}
impl ThemePickerOverlay {
    #[must_use]
    pub fn new(picker: ThemePicker, theme: UiTheme) -> Self {
        Self { picker, theme }
    }
    #[must_use]
    pub fn picker(&self) -> SearchPickerHandle {
        self.picker.handle()
    }
}
impl View for ThemePickerOverlay {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let picker = self.picker.handle();
        Backdrop::new("theme-picker-backdrop", self.theme)
            .on_dismiss(move || picker.dismiss())
            .child(self.picker)
            .build(cx)
    }
}
