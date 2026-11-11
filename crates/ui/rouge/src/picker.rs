use crate::{
    components::{SelectionState, SurfaceExt, backdrop, list_row, muted_text},
    fonts::UI_FONT,
    icon::{Icon, IconName},
    theme::UiTheme,
    tokens,
    typography::{Typography, UiStyles},
};
use clankerdiff_ui_core::{Appearance, PickerOption, PickerRow, PickerState, ReviewTheme, ThemeChoice, ThemeSelection};
use rouge_core::{
    AnyView, Color, Div, Key, Length, Rect, ScrollState, Scrollable, Styled, View, component, div, dynamic, each,
    editable, pct, px, scroll, show, text, view,
};
use rouge_reactive::{Callback, Memo, Signal, batch, effect, memo, signal};
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

#[component]
#[allow(clippy::needless_pass_by_value)]
pub fn picker_list(
    #[prop(into)] id: String,
    items: Vec<PickerItem>,
    highlighted: Option<usize>,
    #[prop(into)] empty: Option<String>,
    max_height: Option<Length>,
    track_scroll: Option<Signal<ScrollState>>,
    #[prop(default, into)] on_choose: Callback<(usize,)>,
    #[prop(default, into)] on_hover: Callback<(usize,)>,
) -> impl View {
    let state = track_scroll.unwrap_or_else(|| signal(ScrollState::default()));
    let picker = PickerState::new(items.iter().map(PickerItem::option).collect()).unfiltered();
    let columns = Columns::new(&items);
    let list = if items.is_empty() {
        view! { <div><muted_text>{empty.unwrap_or_else(|| "No results".into())}</muted_text></div> }
    } else {
        let rows = picker.rows().into_iter().map(|row| match row {
            PickerRow::Group(label) => group(label).into_any(),
            PickerRow::Option { position, index } => {
                let item = items[index].clone();
                let disabled = item.disabled;
                let state = match (disabled, highlighted == Some(position)) {
                    (true, _) => SelectionState::Disabled,
                    (false, true) => SelectionState::Selected,
                    (false, false) => SelectionState::None,
                };
                item_row(item, &columns, state, None, Callback::from(move || on_choose.call(position)))
                    .on_pointer_move(move |_| {
                        if !disabled {
                            on_hover.call(position);
                        }
                    })
                    .into_any()
            }
        });
        view! { <div flex col>{..rows}</div> }
    };
    view! {
        <scroll test_id=id max_h={max_height.unwrap_or_else(|| Length::design(tokens::PICKER_LIST_HEIGHT))}
                scroll_state=state>
            {list}
        </scroll>
    }
}

fn group(label: &str) -> impl View {
    view! {
        <div px=8.0 pt=6.0><text caption muted>{label.to_owned()}</text></div>
    }
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

fn item_row(
    item: PickerItem,
    columns: &Columns,
    state: SelectionState,
    selection: Option<Memo<SelectionState>>,
    on_click: Callback,
) -> Div {
    let theme = UiTheme::current();
    let slot = || view! { <div flex_none size=tokens::ICON_SIZE flex items_center justify_center/> };
    let marker = columns.marker.then(|| {
        let mark = match item.marker {
            Some(PickerMarker::Dot(color)) => Some(view! { <div size=7.0 rounded=4.0 bg=color/> }),
            Some(PickerMarker::Pending) => {
                Some(view! { <div size=10.0 rounded=5.0 border=px(1.0) border_color=theme.text_muted/> })
            }
            None => None,
        };
        slot().test_id(format!("{}-marker", item.id)).child(mark)
    });
    let prefix = columns.prefix.clone().map(|widest| {
        let code = |value: String| view! { <text code muted no_wrap>{value}</text> };
        view! {
            <div relative flex_none>
                <div opacity=0.0 pass_through>{code(widest)}</div>
                {item.prefix.map(|value| view! { <div absolute left=0.0 top=0.0>{code(value)}</div> })}
            </div>
        }
    });
    let subtitle = item.subtitle.map(|subtitle| {
        view! {
            <div test_id={format!("{}-subtitle", item.id)}><text small muted no_wrap>{subtitle}</text></div>
        }
    });
    let details = (!item.details.is_empty()).then(|| {
        let details = item.details.into_iter().map(move |detail| {
            view! {
                <text typography={if detail.code { Typography::Code } else { Typography::Caption }} no_wrap
                      color={move || detail.color.unwrap_or_else(|| theme.text_muted.get())}>
                    {detail.text}
                </text>
            }
        });
        view! { <div flex items_center gap=6.0>{..details}</div> }
    });
    let check = columns.checked.then(|| {
        let icon = item.checked.then(|| Icon::new(IconName::Check, theme.accent).size(12.0));
        slot().test_id(format!("{}-check", item.id)).child(icon)
    });
    view! {
        <list_row id={item.id.clone()} state selection?=selection on_click>
            {marker}
            {item.icon.map(|icon| Icon::new(icon, theme.text_secondary))}
            {prefix}
            <div flex col flex_1 min_w=0.0 overflow_hidden when:py=(subtitle.is_some(), 4.0)>
                <div test_id={format!("{}-label", item.id)}><text body no_wrap>{item.label}</text></div>
                {subtitle}
            </div>
            {details}
            {check}
        </list_row>
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchPickerEvent {
    Query(String),
    Highlighted(usize),
    Chose(usize),
    Dismissed,
}

#[derive(Clone)]
pub struct Preview(Rc<dyn Fn(usize) -> AnyView>);

impl<T: Fn(usize) -> AnyView + 'static> From<T> for Preview {
    fn from(render: T) -> Self {
        Self(Rc::new(render))
    }
}

#[derive(Clone, Copy)]
pub struct SearchPickerHandle {
    items: Signal<Vec<PickerItem>>,
    model: Signal<PickerState>,
    query: Signal<String>,
    emit: Callback<(SearchPickerEvent,)>,
}
impl SearchPickerHandle {
    pub fn new(items: Vec<PickerItem>, emit: impl Fn(SearchPickerEvent) + 'static) -> Self {
        Self {
            model: signal(PickerState::new(items.iter().map(PickerItem::option).collect())),
            items: signal(items),
            query: signal(String::new()),
            emit: emit.into(),
        }
    }
    #[must_use]
    pub fn unfiltered(self) -> Self {
        self.model.update(|model| *model = model.clone().unfiltered());
        self
    }
    #[must_use]
    pub fn max_matches(self, limit: usize) -> Self {
        self.model.update(|model| *model = model.clone().limit(limit));
        self
    }
    #[must_use]
    pub fn highlighted(self, index: usize) -> Self {
        self.model.update(|model| {
            model.highlight(index);
        });
        self
    }
    pub fn set_items(&self, items: Vec<PickerItem>) {
        batch(|| {
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
            self.emit.call(SearchPickerEvent::Chose(index));
        }
    }
    pub fn dismiss(&self) {
        self.emit.call(SearchPickerEvent::Dismissed);
    }
    fn update(&self, f: impl FnOnce(&mut PickerState)) {
        let before = self.model.with_untracked(PickerState::highlighted);
        self.model.update(f);
        if let Some(after) = self.model.with_untracked(PickerState::highlighted).filter(|after| Some(*after) != before)
        {
            self.emit.call(SearchPickerEvent::Highlighted(after));
        }
    }
}

#[derive(Clone, PartialEq)]
enum Row {
    Group(String),
    Item(usize, PickerItem),
}

#[component]
pub fn search_picker(
    #[prop(into)] id: String,
    handle: SearchPickerHandle,
    #[prop(into)] searchable: Option<String>,
    #[prop(into)] title: Option<String>,
    #[prop(into)] footer: Option<String>,
    #[prop(into)] empty: Option<String>,
    width: Option<Length>,
    height: Option<Length>,
    max_list_height: Option<Length>,
    #[prop(into)] preview: Option<Preview>,
    status: Option<Signal<Option<String>>>,
) -> Div {
    let (query, model, items) = (handle.query, handle.model, handle.items);
    effect(move || {
        let next = query.get();
        if !model.with_untracked(|model| model.query() == next) {
            handle.emit.call(SearchPickerEvent::Query(next.clone()));
            handle.update(|model| model.set_query(next));
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
                        PickerRow::Option { index, .. } => items.get(index).cloned().map(|item| Row::Item(index, item)),
                    })
                    .collect::<Vec<_>>()
            })
        })
    });
    let focus_root = searchable.is_none();
    let empty = empty.unwrap_or_else(|| "No matching items".into());
    let list_height = max_list_height.unwrap_or_else(|| Length::design(tokens::PICKER_LIST_HEIGHT));
    let list = results(handle, entries, highlight, empty, list_height);
    let status = status.unwrap_or_else(|| signal(None));
    let list = match preview {
        Some(preview) => view! { <with_preview id={&id} highlight preview>{list}</with_preview> }.into_any(),
        None => list.into_any(),
    };
    view! {
        <div test_id=id flex col min_h=0.0 w={width.unwrap_or_else(|| Length::design(tokens::PICKER_WIDTH))}
             max_w=pct(100.0) gap=6.0 p=8.0 floating h?=height when:autofocus=focus_root
             when:tab_index=(focus_root, -1)
             on_key_down={move |event| {
                 match event.key {
                     Key::ArrowUp => handle.move_highlight(-1),
                     Key::ArrowDown => handle.move_highlight(1),
                     Key::PageUp => handle.move_highlight(-8),
                     Key::PageDown => handle.move_highlight(8),
                     Key::Enter => handle.choose(),
                     Key::Escape => handle.dismiss(),
                     _ => return,
                 }
                 event.stop_propagation();
             }}>
            {title.map(|title| view! { <text title primary>{title}</text> })}
            {searchable.map(|hint| view! { <query_input hint handle/> })}
            {list}
            <show when={move || status.with(Option::is_some)}>
                <text caption color=UiTheme::current().warning>{move || status.get().unwrap_or_default()}</text>
            </show>
            {footer.map(|footer| view! { <muted_text>{footer}</muted_text> })}
        </div>
    }
}

#[component]
fn with_preview(id: &str, children: impl View, highlight: Memo<Option<usize>>, preview: Preview) -> impl View {
    view! {
        <div flex row flex_1 min_h=0.0>
            <div flex_none w=tokens::PICKER_PREVIEW_LIST_WIDTH max_w=pct(50.0) min_h=0.0 pr=6.0 border_r=px(1.0)
                 border_color=UiTheme::current().border_subtle>
                {children}
            </div>
            <div test_id={format!("{id}-preview")} flex_1 min_w=0.0 min_h=0.0 flex col>
                <dynamic>{highlight.get().map(|index| (preview.0)(index))}</dynamic>
            </div>
        </div>
    }
}

#[component]
fn query_input(hint: String, handle: SearchPickerHandle) -> impl View {
    let query = handle.query;
    view! {
        <div relative p=6.0 surface>
            <show when={move || query.with(String::is_empty)}>
                <div absolute inset=6.0 pass_through><muted_text>{hint.clone()}</muted_text></div>
            </show>
            <editable value=query test_id="picker-query" autofocus font=UI_FONT text_size=13.0
                      color=UiTheme::current().text w_full on_submit={move |_| handle.choose()}/>
        </div>
    }
}

fn results(
    handle: SearchPickerHandle,
    entries: Memo<Vec<Row>>,
    highlight: Memo<Option<usize>>,
    empty: String,
    list_height: Length,
) -> impl View {
    let columns = memo(move || handle.items.with(|items| Columns::new(items)));
    let current = signal(None::<(usize, Rect)>);
    let scroll_state = signal(ScrollState::default());
    let bounds = signal(Rect::ZERO);
    let scroll_list = view! {
        <scroll test_id="picker-results" track_bounds=bounds scroll_state=scroll_state max_h=list_height>
            <show when={move || entries.with(Vec::is_empty)}><muted_text>{empty.clone()}</muted_text></show>
            <each items={move || entries.get().into_iter().enumerate().collect()}
                  key={|(position, row)| match row {
                      Row::Group(_) => (*position, None),
                      Row::Item(i, item) => (*i, Some(item.id.clone())),
                  }}
                  let:entry>
                <dynamic>{match entry.get().1 {
                    Row::Group(label) => group(&label).into_any(),
                    Row::Item(index, item) => {
                        let disabled = item.disabled;
                        let row_bounds = signal(Rect::ZERO);
                        effect(move || {
                            if highlight.get() == Some(index) {
                                current.set_if_changed(Some((index, row_bounds.get())));
                            }
                        });
                        let state = if disabled { SelectionState::Disabled } else { SelectionState::None };
                        let selection = memo(move || {
                            if highlight.get() == Some(index) { SelectionState::Selected } else { SelectionState::None }
                        });
                        let choose = Callback::from(move || {
                            handle.highlight(index);
                            handle.choose();
                        });
                        let row = item_row(item, &columns.get(), state, Some(selection), choose)
                            .on_pointer_move(move |_| {
                                if !disabled {
                                    handle.highlight(index);
                                }
                            });
                        view! { <div track_bounds=row_bounds>{row}</div> }.into_any()
                    }
                }}</dynamic>
            </each>
        </scroll>
    };
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

pub fn theme_picker(
    current: &ReviewTheme,
    themes: Arc<[ThemeChoice]>,
    emit: impl Fn(ThemePickerEvent) + 'static,
) -> SearchPickerHandle {
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
    SearchPickerHandle::new(items, move |event| {
        let event = match event {
            SearchPickerEvent::Highlighted(index) => {
                selection.borrow_mut().as_mut().and_then(|s| s.select(index)).map(ThemePickerEvent::Preview)
            }
            SearchPickerEvent::Chose(index) => selection.borrow_mut().take().map(|mut s| {
                s.select(index);
                ThemePickerEvent::Commit(s.commit())
            }),
            SearchPickerEvent::Dismissed => selection.borrow_mut().take().map(|s| ThemePickerEvent::Cancel(s.cancel())),
            SearchPickerEvent::Query(_) => None,
        };
        if let Some(event) = event {
            emit(event);
        }
    })
    .highlighted(index)
}

#[component]
pub fn theme_picker_view(#[prop(into)] id: String, handle: SearchPickerHandle) -> Div {
    view! {
        <search_picker id handle searchable="Search themes…" empty="No matching themes"
                       footer="↑↓ Preview    ↵ Apply    Esc Cancel"/>
    }
}

#[component]
pub fn theme_picker_overlay(#[prop(into)] id: String, handle: SearchPickerHandle) -> impl View {
    view! {
        <backdrop id="theme-picker-backdrop" on_dismiss={move || handle.dismiss()}>
            <theme_picker_view id handle/>
        </backdrop>
    }
}
