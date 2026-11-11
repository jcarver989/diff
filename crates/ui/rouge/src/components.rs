use crate::{
    icon::{Icon, IconName},
    indicator_layer::IndicatorMark,
    theme::UiTheme,
    tokens::{self, rem, scaled},
    typography::{Typography, TypographyExt},
};
pub use clankerdiff_ui_core::{
    ButtonVariant, ControlSize, ControlState, InteractionState, ModalSize, NoticeTone, SelectionState,
};
use rouge_core::{
    AnyView, BoxShadow, BuildCx, Color, Cursor, Div, ElementId, Key, Length, PointerEvent, Rect, Styled, View, div,
    overlay, text,
};
use rouge_reactive::{Signal, signal};
use std::rc::Rc;

type Handler = Rc<dyn Fn()>;

pub(crate) fn activate(root: Div, focusable: bool, enabled: bool, action: Option<Handler>) -> Div {
    let root = root.enabled(enabled);
    if !enabled {
        return root;
    }
    let Some(action) = action else {
        return root;
    };
    let pointer = action.clone();
    let root = root.cursor(Cursor::Pointer).on_click(move |event| {
        event.stop_propagation();
        pointer();
    });
    if focusable {
        root.focusable().on_key_down(move |event| {
            if matches!(event.key, Key::Enter | Key::Space) {
                event.stop_propagation();
                action();
            }
        })
    } else {
        root
    }
}

pub struct Button {
    id: String,
    label: String,
    icon: Option<IconName>,
    hint: Option<String>,
    theme: UiTheme,
    variant: ButtonVariant,
    state: ControlState,
    size: ControlSize,
    full: bool,
    focusable: bool,
    action: Option<Handler>,
}
impl Button {
    pub fn new(id: impl Into<String>, label: impl Into<String>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            hint: None,
            theme,
            variant: ButtonVariant::default(),
            state: ControlState::default(),
            size: ControlSize::default(),
            full: false,
            focusable: false,
            action: None,
        }
    }
    #[must_use]
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
    #[must_use]
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
    #[must_use]
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
    #[must_use]
    pub fn tooltip(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.interaction = if disabled { InteractionState::Disabled } else { InteractionState::Rest };
        self
    }
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.state.selected = selected;
        self
    }
    #[must_use]
    pub fn keyboard_focusable(mut self) -> Self {
        self.focusable = true;
        self
    }
    #[must_use]
    pub fn full_width(mut self) -> Self {
        self.full = true;
        self
    }
    #[must_use]
    pub fn on_click(mut self, action: impl Fn() + 'static) -> Self {
        self.action = Some(Rc::new(action));
        self
    }
}
impl View for Button {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, variant, state) = (self.theme, self.variant, self.state);
        let enabled = state.interaction != InteractionState::Disabled;
        let hovered = signal(false);
        let focused = signal(false);
        let style = rouge_reactive::memo(move || {
            theme.control_style(
                variant,
                ControlState {
                    interaction: if enabled && hovered.get() { InteractionState::Hovered } else { state.interaction },
                    ..state
                },
            )
        });
        let root = div()
            .test_id(self.id)
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .gap(rem(6.0))
            .rounded(scaled(tokens::RADIUS_CONTROL))
            .on_hover(move |inside| {
                hovered.set_if_changed(inside);
            })
            .on_focus(move |event| {
                focused.set_if_changed(event.visible);
            })
            .on_blur(move |_| {
                focused.set(false);
            })
            .border(1.0)
            .border_color(move || if focused.get() { theme.colors().accent } else { Color::TRANSPARENT })
            .bg(move || style.get().background.unwrap_or(Color::TRANSPARENT));
        let mut root = root.h(rem(if self.size == ControlSize::Small { 24.0 } else { tokens::ROW_HEIGHT })).px(rem(
            if self.size == ControlSize::Small {
                tokens::CONTROL_PADDING_X_SMALL
            } else {
                tokens::CONTROL_PADDING_X_MEDIUM
            },
        ));
        if self.full {
            root = root.w_full();
        }
        if let Some(icon) = self.icon {
            root = root.child(Icon::new(icon, move || style.get().foreground));
        }
        root = root.child(
            text(self.label)
                .typography(Typography::Label)
                .weight(move || if style.get().emphasized { 600.0 } else { 500.0 })
                .color(move || style.get().foreground)
                .no_wrap(),
        );
        let root = activate(root, self.focusable, enabled, self.action);
        if let Some(hint) = self.hint { Tooltip::new(root, hint).build(cx) } else { root.build(cx) }
    }
}

pub struct IconButton {
    id: String,
    icon: IconName,
    label: String,
    theme: UiTheme,
    icon_size: f32,
    selected: bool,
    disabled: bool,
    focusable: bool,
    action: Option<Handler>,
}
impl IconButton {
    pub fn new(id: impl Into<String>, icon: IconName, label: impl Into<String>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            theme,
            icon_size: tokens::ICON_SIZE,
            selected: false,
            disabled: false,
            focusable: false,
            action: None,
        }
    }
    #[must_use]
    pub fn on_click(mut self, action: impl Fn() + 'static) -> Self {
        self.action = Some(Rc::new(action));
        self
    }
    #[must_use]
    pub const fn icon_size(mut self, size: f32) -> Self {
        self.icon_size = size;
        self
    }
    #[must_use]
    pub const fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    #[must_use]
    pub const fn keyboard_focusable(mut self) -> Self {
        self.focusable = true;
        self
    }
}
impl View for IconButton {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, selected, enabled) = (self.theme, self.selected, !self.disabled);
        let hovered = signal(false);
        let focused = signal(false);
        let root = div()
            .test_id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(rem(tokens::ICON_BUTTON_SIZE))
            .h(rem(tokens::ICON_BUTTON_SIZE))
            .rounded(scaled(tokens::ICON_BUTTON_SIZE / 2.0))
            .bg(move || {
                let colors = theme.colors();
                if selected || focused.get() {
                    colors.surface_selected
                } else if enabled && hovered.get() {
                    colors.surface_hover
                } else {
                    Color::TRANSPARENT
                }
            })
            .on_hover(move |inside| {
                hovered.set_if_changed(inside);
            })
            .on_focus(move |event| {
                focused.set_if_changed(event.visible);
            })
            .on_blur(move |_| {
                focused.set(false);
            })
            .child(
                Icon::new(self.icon, move || {
                    let colors = theme.colors();
                    if selected || (enabled && hovered.get()) { colors.text } else { colors.text_muted }
                })
                .size(self.icon_size),
            );
        let root = if enabled { root } else { root.opacity(0.4) };
        let root = activate(root, self.focusable, enabled, self.action);
        if enabled { Tooltip::new(root, self.label).build(cx) } else { root.build(cx) }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CheckState {
    #[default]
    Unchecked,
    Checked,
    Indeterminate,
}

pub struct Checkbox {
    id: String,
    state: CheckState,
    hint: Option<String>,
    theme: UiTheme,
    disabled: bool,
    action: Option<Handler>,
}
impl Checkbox {
    pub fn new(id: impl Into<String>, state: CheckState, theme: UiTheme) -> Self {
        Self { id: id.into(), state, hint: None, theme, disabled: false, action: None }
    }
    #[must_use]
    pub fn tooltip(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    #[must_use]
    pub fn on_toggle(mut self, action: impl Fn() + 'static) -> Self {
        self.action = Some(Rc::new(action));
        self
    }
}
impl View for Checkbox {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, state, enabled) = (self.theme, self.state, !self.disabled);
        let hovered = signal(false);
        let focused = signal(false);
        let mark = rouge_core::canvas(move |painter| {
            let size = painter.size();
            let at = |x: f32, y: f32| (size.width * x, size.height * y);
            let path = match state {
                CheckState::Unchecked => return,
                CheckState::Checked => {
                    let ((ax, ay), (bx, by), (cx, cy)) = (at(0.22, 0.5), at(0.43, 0.72), at(0.78, 0.28));
                    rouge_core::Path::builder().move_to(ax, ay).line_to(bx, by).line_to(cx, cy).build()
                }
                CheckState::Indeterminate => {
                    let ((ax, ay), (bx, by)) = (at(0.25, 0.5), at(0.75, 0.5));
                    rouge_core::Path::builder().move_to(ax, ay).line_to(bx, by).build()
                }
            };
            painter.stroke_round(&path, size.width / 6.0, theme.colors().text);
        })
        .size_full();
        let root = div()
            .test_id(self.id)
            .flex_none()
            .w(rem(tokens::CHECKBOX_SIZE))
            .h(rem(tokens::CHECKBOX_SIZE))
            .rounded(scaled(2.0))
            .border(1.0)
            .border_color(move || {
                let colors = theme.colors();
                if enabled && (hovered.get() || focused.get()) { colors.accent } else { colors.text_muted }
            })
            .bg(theme.paint(|c| c.canvas))
            .on_hover(move |inside| {
                hovered.set_if_changed(inside);
            })
            .on_focus(move |event| {
                focused.set_if_changed(event.visible);
            })
            .on_blur(move |_| {
                focused.set(false);
            })
            .child(mark);
        let root = if enabled { root } else { root.opacity(0.4) };
        let root = activate(root, true, enabled, self.action);
        match self.hint.filter(|_| enabled) {
            Some(hint) => Tooltip::new(root, hint).build(cx),
            None => root.build(cx),
        }
    }
}

pub struct Segment {
    id: String,
    label: String,
    icon: Option<IconName>,
    detail: Option<String>,
    selected: bool,
    disabled: bool,
    action: Option<Handler>,
}
impl Segment {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            detail: None,
            selected: false,
            disabled: false,
            action: None,
        }
    }
    pub fn icon(id: impl Into<String>, icon: IconName, label: impl Into<String>) -> Self {
        let mut s = Self::new(id, label);
        s.icon = Some(icon);
        s
    }
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    #[must_use]
    pub fn on_click(mut self, action: impl Fn() + 'static) -> Self {
        self.action = Some(Rc::new(action));
        self
    }
}
pub struct SegmentedControl {
    id: String,
    theme: UiTheme,
    segments: Vec<Segment>,
}
impl SegmentedControl {
    pub fn new(id: impl Into<String>, theme: UiTheme) -> Self {
        Self { id: id.into(), theme, segments: Vec::new() }
    }
    #[must_use]
    pub fn segments(mut self, segments: impl IntoIterator<Item = Segment>) -> Self {
        self.segments = segments.into_iter().collect();
        self
    }
}
impl View for SegmentedControl {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let theme = self.theme;
        div()
            .test_id(self.id)
            .flex()
            .items_center()
            .gap(rem(2.0))
            .p(rem(2.0))
            .rounded(scaled(tokens::RADIUS_CONTROL))
            .bg(theme.paint(|c| c.chrome))
            .children(self.segments.into_iter().map(move |s| {
                let hovered = signal(false);
                let foreground = move || {
                    let colors = theme.colors();
                    if s.selected || hovered.get() { colors.text } else { colors.text_muted }
                };
                let detail_id = format!("{}-detail", s.id);
                let mut segment = div()
                    .test_id(s.id)
                    .h(rem(24.0))
                    .flex()
                    .items_center()
                    .gap(rem(4.0))
                    .px(rem(if s.icon.is_some() {
                        tokens::CONTROL_PADDING_X_SMALL
                    } else {
                        tokens::CONTROL_PADDING_X_MEDIUM
                    }))
                    .rounded(scaled(tokens::RADIUS_INNER))
                    .on_hover(move |inside| {
                        hovered.set_if_changed(inside);
                    });
                if s.selected {
                    segment = segment.bg(theme.paint(|c| c.surface_selected));
                }
                if s.disabled {
                    segment = segment.opacity(0.4);
                }
                let hint = if let Some(icon) = s.icon {
                    segment = segment.child(Icon::new(icon, foreground));
                    if let Some(detail) = s.detail {
                        segment = segment.child(
                            div()
                                .test_id(detail_id)
                                .child(text(detail).typography(Typography::Small).color(foreground)),
                        );
                    }
                    Some(s.label)
                } else {
                    segment = segment.child(text(s.label).typography(Typography::Small).color(foreground));
                    None
                };
                let segment = activate(segment, true, !s.disabled, s.action);
                if let Some(hint) = hint { Tooltip::new(segment, hint).into_any() } else { segment.into_any() }
            }))
            .build(cx)
    }
}

pub struct Tooltip(rouge_widgets::Tooltip);
impl Tooltip {
    pub fn new(target: impl View, label: impl Into<String>) -> Self {
        Self(rouge_widgets::tooltip(target, label))
    }
    #[must_use]
    pub fn delay(self, delay: std::time::Duration) -> Self {
        Self(self.0.delay(delay))
    }
}
impl View for Tooltip {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        self.0.build(cx)
    }
}

pub struct Disclosure {
    root: Div,
    id: String,
    expanded: bool,
    theme: UiTheme,
    action: Option<Handler>,
}
impl Disclosure {
    pub fn new(id: impl Into<String>, expanded: bool, theme: UiTheme) -> Self {
        Self { root: div(), id: id.into(), expanded, theme, action: None }
    }
    #[must_use]
    pub fn child(mut self, child: impl View) -> Self {
        self.root = self.root.child(child);
        self
    }
    #[must_use]
    pub fn on_toggle(mut self, action: impl Fn() + 'static) -> Self {
        self.action = Some(Rc::new(action));
        self
    }
}
rouge_core::styled!(Disclosure => root);
impl View for Disclosure {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let icon = if self.expanded { IconName::ChevronDown } else { IconName::ChevronRight };
        let root = div()
            .test_id(self.id)
            .flex()
            .items_center()
            .gap(rem(6.0))
            .child(Icon::new(icon, self.theme.paint(|c| c.text_muted)).size(tokens::ICON_SIZE_SMALL))
            .child(self.root);
        activate(root, true, true, self.action).build(cx)
    }
}

pub struct Badge {
    id: String,
    label: String,
    theme: UiTheme,
    tone: NoticeTone,
    icon: Option<IconName>,
}
impl Badge {
    pub fn new(id: impl Into<String>, label: impl Into<String>, theme: UiTheme) -> Self {
        Self { id: id.into(), label: label.into(), theme, tone: NoticeTone::Neutral, icon: None }
    }
    #[must_use]
    pub fn tone(mut self, tone: NoticeTone) -> Self {
        self.tone = tone;
        self
    }
    #[must_use]
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
}
impl View for Badge {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, tone) = (self.theme, self.tone);
        let fg = move || theme.notice_style(tone).foreground;
        let mut v = div()
            .test_id(self.id)
            .flex()
            .items_center()
            .gap(rem(4.0))
            .px(rem(6.0))
            .rounded(scaled(tokens::RADIUS_INNER))
            .bg(move || theme.notice_style(tone).background.unwrap_or(Color::TRANSPARENT));
        if let Some(icon) = self.icon {
            v = v.child(Icon::new(icon, fg).size(tokens::ICON_SIZE_SMALL));
        }
        v.child(text(self.label).typography(Typography::Caption).color(fg)).build(cx)
    }
}

pub struct SectionHeader {
    id: String,
    title: String,
    theme: UiTheme,
    expanded: bool,
    detail: Option<String>,
    badge: Option<Badge>,
    icon: Option<(IconName, Color)>,
    action: Option<AnyView>,
    always: bool,
    toggle: Option<Handler>,
}
impl SectionHeader {
    pub fn new(id: impl Into<String>, title: impl Into<String>, expanded: bool, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            theme,
            expanded,
            detail: None,
            badge: None,
            icon: None,
            action: None,
            always: false,
            toggle: None,
        }
    }
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
    #[must_use]
    pub fn icon(mut self, icon: IconName, color: Color) -> Self {
        self.icon = Some((icon, color));
        self
    }
    #[must_use]
    pub fn badge(mut self, badge: Badge) -> Self {
        self.badge = Some(badge);
        self
    }
    #[must_use]
    pub fn action(mut self, action: impl View) -> Self {
        self.action = Some(action.into_any());
        self
    }
    #[must_use]
    pub fn actions_always_visible(mut self, always: bool) -> Self {
        self.always = always;
        self
    }
    #[must_use]
    pub fn on_toggle(mut self, action: impl Fn() + 'static) -> Self {
        self.toggle = Some(Rc::new(action));
        self
    }
}
impl View for SectionHeader {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let hover = signal(false);
        let mut row = div().flex().items_center().gap(rem(6.0));
        if let Some((icon, color)) = self.icon {
            row = row.child(Icon::new(icon, color));
        }
        row = row.child(text(self.title).typography(Typography::Title).color(self.theme.paint(|c| c.text)));
        if let Some(detail) = self.detail {
            row = row.child(MutedText::new(detail, self.theme));
        }
        if let Some(badge) = self.badge {
            row = row.child(badge);
        }
        let mut disclosure = Disclosure::new(format!("{}-toggle", self.id), self.expanded, self.theme).child(row);
        disclosure.action = self.toggle;
        let mut header = div()
            .test_id(self.id)
            .flex()
            .items_center()
            .justify_between()
            .px(rem(8.0))
            .h(rem(tokens::GROUP_HEIGHT))
            .bg(self.theme.paint(|c| c.chrome))
            .on_hover(move |v| {
                hover.set(v);
            })
            .child(disclosure);
        if let Some(action) = self.action {
            header =
                header.child(div().child(action).opacity(move || if self.always || hover.get() { 1.0 } else { 0.0 }));
        }
        header.build(cx)
    }
}

pub trait SurfaceExt: Styled {
    #[must_use]
    fn surface(self, theme: UiTheme) -> Self {
        self.bg(theme.paint(|c| c.surface))
            .border(1.0)
            .border_color(theme.paint(|c| c.border_subtle))
            .rounded(scaled(tokens::RADIUS_CONTROL))
    }
    #[must_use]
    fn floating(self, theme: UiTheme) -> Self {
        self.bg(theme.paint(|c| c.elevated))
            .border(1.0)
            .border_color(theme.paint(|c| c.border))
            .rounded(scaled(tokens::RADIUS_FLOATING))
            .shadow(BoxShadow::md())
    }
}
impl<T: Styled> SurfaceExt for T {}

pub struct Surface {
    root: Div,
    theme: UiTheme,
    padded: bool,
    raised: bool,
    selected: bool,
}
impl Surface {
    #[must_use]
    pub fn new(theme: UiTheme) -> Self {
        Self { root: div().flex().col(), theme, padded: false, raised: false, selected: false }
    }
    #[must_use]
    pub fn padded(mut self, padded: bool) -> Self {
        self.padded = padded;
        self
    }
    #[must_use]
    pub fn raised(mut self, raised: bool) -> Self {
        self.raised = raised;
        self
    }
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    #[must_use]
    pub fn child(mut self, child: impl View) -> Self {
        self.root = self.root.child(child);
        self
    }
}
rouge_core::styled!(Surface => root);
impl View for Surface {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let mut root = if self.raised { self.root.floating(self.theme) } else { self.root.surface(self.theme) };
        if self.padded {
            root = root.p(rem(12.0));
        }
        if self.selected {
            root = root.bg(self.theme.paint(|c| c.surface_selected));
        }
        root.build(cx)
    }
}

pub struct ListRow {
    root: Div,
    id: String,
    theme: UiTheme,
    state: SelectionState,
    selection: Option<Rc<dyn Fn() -> SelectionState>>,
    depth: usize,
    action: Option<Handler>,
}
impl ListRow {
    pub fn new(id: impl Into<String>, theme: UiTheme) -> Self {
        Self {
            root: div().flex().items_center(),
            id: id.into(),
            theme,
            state: SelectionState::None,
            selection: None,
            depth: 0,
            action: None,
        }
    }
    #[must_use]
    pub fn hover_group(id: &str) -> String {
        format!("{id}-row")
    }
    #[must_use]
    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }
    #[must_use]
    pub fn state(mut self, state: SelectionState) -> Self {
        self.state = state;
        self
    }
    #[must_use]
    pub fn selected(self, selected: bool) -> Self {
        self.state(if selected { SelectionState::Selected } else { SelectionState::None })
    }
    #[must_use]
    pub fn selection(mut self, selection: impl Fn() -> SelectionState + 'static) -> Self {
        self.selection = Some(Rc::new(selection));
        self
    }
    #[must_use]
    pub fn disabled(self, disabled: bool) -> Self {
        if disabled { self.state(SelectionState::Disabled) } else { self }
    }
    #[must_use]
    pub fn on_click(mut self, action: impl Fn() + 'static) -> Self {
        self.action = Some(Rc::new(action));
        self
    }
    #[must_use]
    pub fn on_mouse_move(mut self, action: impl Fn(&PointerEvent) + 'static) -> Self {
        self.root = self.root.on_pointer_move(action);
        self
    }
    #[must_use]
    pub fn on_hover(mut self, action: impl Fn(bool) + 'static) -> Self {
        self.root = self.root.on_hover(action);
        self
    }
    #[must_use]
    pub fn child(mut self, child: impl View) -> Self {
        self.root = self.root.child(child);
        self
    }
}
rouge_core::styled!(ListRow => root);
impl View for ListRow {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, state) = (self.theme, self.state);
        let selection = rouge_reactive::memo(move || {
            if state == SelectionState::Disabled { state } else { self.selection.as_ref().map_or(state, |f| f()) }
        });
        let hover = signal(false);
        let root = self
            .root
            .group(Self::hover_group(&self.id))
            .test_id(self.id)
            .min_h(rem(tokens::ROW_HEIGHT))
            .pl(rem(8.0 + crate::tree::indent(self.depth)))
            .pr(rem(8.0))
            .gap(rem(6.0))
            .rounded(scaled(tokens::RADIUS_INNER))
            .on_hover(move |inside| {
                hover.set(inside);
            })
            .bg(move || match selection.get() {
                SelectionState::None if hover.get() => theme.colors().surface_hover,
                SelectionState::None | SelectionState::Disabled => Color::TRANSPARENT,
                state => theme.selection_style(state).background.unwrap_or(Color::TRANSPARENT),
            })
            .text_color(move || theme.selection_style(selection.get()).foreground);
        activate(root, true, state != SelectionState::Disabled, self.action)
            .enabled(move || selection.get() != SelectionState::Disabled)
            .build(cx)
    }
}

pub struct ActionBar(Div);
impl ActionBar {
    #[must_use]
    pub fn new(theme: UiTheme) -> Self {
        Self(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(rem(8.0))
                .min_h(rem(tokens::BAR_HEIGHT))
                .px(rem(12.0))
                .bg(theme.paint(|c| c.chrome)),
        )
    }
    #[must_use]
    pub fn child(mut self, child: impl View) -> Self {
        self.0 = self.0.child(child);
        self
    }
}
impl View for ActionBar {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        self.0.build(cx)
    }
}

pub struct Backdrop {
    id: String,
    theme: UiTheme,
    top: bool,
    anchor: Option<Signal<Rect>>,
    root: Div,
    dismiss: Option<Handler>,
}
impl Backdrop {
    pub fn new(id: impl Into<String>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            theme,
            top: false,
            anchor: None,
            root: div().flex().col().min_h(0.0).max_h(rouge_core::pct(100.0)).max_w(rouge_core::pct(100.0)),
            dismiss: None,
        }
    }
    #[must_use]
    pub fn align_top(mut self) -> Self {
        self.top = true;
        self
    }
    #[must_use]
    pub const fn anchor_below(mut self, anchor: Signal<Rect>) -> Self {
        self.anchor = Some(anchor);
        self
    }
    #[must_use]
    pub fn on_dismiss(mut self, action: impl Fn() + 'static) -> Self {
        self.dismiss = Some(Rc::new(action));
        self
    }
    #[must_use]
    pub fn child(mut self, child: impl View) -> Self {
        self.root = self.root.child(child);
        self
    }
}
impl View for Backdrop {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        overlay(move || {
            let dismiss = self.dismiss;
            let outside = dismiss.clone();
            let inner = self.root.on_pointer_down(PointerEvent::stop_propagation);
            let root = div()
                .test_id(self.id)
                .absolute()
                .inset(0.0)
                .flex()
                .col()
                .focus_scope()
                .autofocus()
                .tab_index(-1)
                .on_pointer_down(move |_| {
                    if let Some(f) = &outside {
                        f();
                    }
                })
                .on_key_down(move |event| {
                    if event.key == Key::Tab {
                        return;
                    }
                    if event.key == Key::Escape
                        && let Some(f) = &dismiss
                    {
                        f();
                    }
                    event.stop_propagation();
                });
            let (root, inner) = if let Some(anchor) = self.anchor {
                let window = rouge_core::window_size();
                let measured = signal(Rect::ZERO);
                let inner = inner
                    .absolute()
                    .left(0.0)
                    .top(0.0)
                    .track_bounds(measured)
                    .translate_x(move || {
                        let space = (window.get().width - measured.get().size.width).max(0.0);
                        anchor.get().origin.x.clamp(0.0, space)
                    })
                    .translate_y(move || anchor.get().max().y + 4.0);
                (root, inner)
            } else {
                let root = root.items_center().p(rem(16.0)).bg(self.theme.paint(|c| c.scrim));
                let root = if self.top {
                    root.justify_start().pt(rem(tokens::MODAL_TOP_OFFSET))
                } else {
                    root.justify_center()
                };
                (root, inner)
            };
            root.child(IndicatorMark::occluder("modal-occluder").absolute().inset(0.0)).child(inner)
        })
        .build(cx)
    }
}

pub struct Modal {
    id: String,
    title: String,
    theme: UiTheme,
    root: Div,
    hint: Option<String>,
    width: Length,
    height: Option<Length>,
    top: bool,
    dismiss: Option<Handler>,
}
impl Modal {
    pub fn new(id: impl Into<String>, title: impl Into<String>, theme: UiTheme) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            theme,
            root: div().flex().col().min_h(0.0),
            hint: None,
            width: rem(tokens::MODAL_WIDTH),
            height: None,
            top: false,
            dismiss: None,
        }
    }
    #[must_use]
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
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
    pub fn size(self, size: ModalSize) -> Self {
        self.width(rem(match size {
            ModalSize::Compact => tokens::MODAL_WIDTH_COMPACT,
            ModalSize::Medium => tokens::MODAL_WIDTH,
            ModalSize::Wide => tokens::MODAL_WIDTH_WIDE,
        }))
    }
    #[must_use]
    pub fn align_top(mut self) -> Self {
        self.top = true;
        self
    }
    #[must_use]
    pub fn on_dismiss(mut self, action: impl Fn() + 'static) -> Self {
        self.dismiss = Some(Rc::new(action));
        self
    }
    #[must_use]
    pub fn child(mut self, child: impl View) -> Self {
        self.root = self.root.child(child);
        self
    }
}
impl View for Modal {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let mut body = div()
            .test_id(self.id.clone())
            .flex()
            .col()
            .gap(rem(8.0))
            .p(rem(16.0))
            .w(self.width)
            .max_w(rouge_core::pct(100.0))
            .overflow_hidden()
            .floating(self.theme)
            .child(text(self.title).typography(Typography::Heading).color(self.theme.paint(|c| c.text)));
        if let Some(hint) = self.hint {
            body = body.child(MutedText::new(hint, self.theme));
        }
        if let Some(height) = self.height {
            body = body.h(height);
        }
        let mut backdrop = Backdrop::new(format!("{}-backdrop", self.id), self.theme).child(body.child(self.root));
        backdrop.top = self.top;
        backdrop.dismiss = self.dismiss;
        backdrop.build(cx)
    }
}

pub struct MutedText(rouge_core::Text);
impl MutedText {
    pub fn new(label: impl Into<String>, theme: UiTheme) -> Self {
        Self(text(label.into()).typography(Typography::Small).color(theme.paint(|c| c.text_muted)))
    }
}
impl View for MutedText {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        self.0.build(cx)
    }
}

pub struct EmptyState {
    icon: IconName,
    title: String,
    detail: String,
    theme: UiTheme,
}
impl EmptyState {
    pub fn new(icon: IconName, title: impl Into<String>, detail: impl Into<String>, theme: UiTheme) -> Self {
        Self { icon, title: title.into(), detail: detail.into(), theme }
    }
}
impl View for EmptyState {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        div()
            .flex()
            .col()
            .items_center()
            .justify_center()
            .p(rem(16.0))
            .gap(rem(6.0))
            .child(Icon::new(self.icon, self.theme.paint(|c| c.text_muted)))
            .child(text(self.title).typography(Typography::Title).color(self.theme.paint(|c| c.text)))
            .child(MutedText::new(self.detail, self.theme))
            .build(cx)
    }
}

pub struct Notification {
    message: String,
    tone: NoticeTone,
    theme: UiTheme,
}
impl Notification {
    pub fn new(message: impl Into<String>, tone: NoticeTone, theme: UiTheme) -> Self {
        Self { message: message.into(), tone, theme }
    }
    pub fn error(message: impl Into<String>, theme: UiTheme) -> Self {
        Self::new(message, NoticeTone::Error, theme)
    }
}
impl View for Notification {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let (theme, tone) = (self.theme, self.tone);
        let fg = move || theme.notice_style(tone).foreground;
        let icon = match tone {
            NoticeTone::Error | NoticeTone::Warning => IconName::TriangleAlert,
            NoticeTone::Positive => IconName::CircleCheck,
            _ => IconName::Info,
        };
        div()
            .flex()
            .items_center()
            .gap(rem(8.0))
            .p(rem(8.0))
            .rounded(scaled(tokens::RADIUS_CONTROL))
            .bg(move || theme.notice_style(tone).background.unwrap_or(Color::TRANSPARENT))
            .child(Icon::new(icon, fg))
            .child(text(self.message).typography(Typography::Body).color(fg))
            .build(cx)
    }
}
