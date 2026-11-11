use crate::{
    icon::{Icon, IconName},
    indicator_layer::IndicatorMark,
    theme::UiTheme,
    tokens,
    typography::UiStyles,
};
pub use clankerdiff_ui_core::{
    ButtonVariant, ControlSize, ControlState, InteractionState, ModalSize, NoticeTone, SelectionState,
};
use rouge_core::{
    AnyView, Apply, BoxShadow, Color, Cursor, Div, Interactions, Key, Length, PointerEvent, Prop, Rect, Styled, Text,
    View, component, div, overlay, pct, px, styles, text, view,
};
use rouge_reactive::{Callback, Signal, memo, signal};
use rouge_widgets::Hint;

pub(crate) fn pressable(root: Div, keyboard: bool, enabled: bool, action: Callback) -> Div {
    let root = root.enabled(enabled);
    if !enabled || !action.is_set() {
        return root;
    }
    if keyboard {
        root.on_press(move || action.call())
    } else {
        root.cursor(Cursor::Pointer).on_click(move |event| {
            event.stop_propagation();
            action.call();
        })
    }
}

fn hinted(root: Div, hint: Option<String>) -> AnyView {
    match hint {
        Some(hint) => root.tooltip(hint).into_any(),
        None => root.into_any(),
    }
}

#[component]
pub fn button(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    icon: Option<IconName>,
    #[prop(default)] variant: ButtonVariant,
    #[prop(default)] size: ControlSize,
    #[prop(into)] tooltip: Option<String>,
    #[prop(default)] disabled: bool,
    #[prop(default)] selected: bool,
    #[prop(default)] keyboard_focusable: bool,
    #[prop(default)] full_width: bool,
    #[prop(default, into)] on_click: Callback,
) -> AnyView {
    let theme = UiTheme::current();
    let state = signal(Interactions::default());
    let style = memo(move || {
        let state = state.get();
        let interaction = if disabled {
            InteractionState::Disabled
        } else if state.hovered {
            InteractionState::Hovered
        } else {
            InteractionState::Rest
        };
        theme.control_style(variant, ControlState { interaction, selected })
    });
    let small = size == ControlSize::Small;
    let height = if small { 24.0 } else { tokens::ROW_HEIGHT };
    let padding = if small { tokens::CONTROL_PADDING_X_SMALL } else { tokens::CONTROL_PADDING_X_MEDIUM };
    let foreground = move || style.get().foreground;
    let root = view! {
        <div test_id=id flex items_center justify_center flex_none gap=6.0 rounded=tokens::RADIUS_CONTROL
             track_interaction=state border=px(1.0) h=height px=padding when:w_full=full_width
             border_color={move || if state.get().focus_visible() { theme.accent.get() } else { Color::TRANSPARENT }}
             bg={move || style.get().background.unwrap_or(Color::TRANSPARENT)}>
            {icon.map(|icon| Icon::new(icon, foreground))}
            <text label no_wrap weight={move || if style.get().emphasized { 600.0 } else { 500.0 }} color=foreground>
                {label}
            </text>
        </div>
    };
    hinted(pressable(root, keyboard_focusable, !disabled, on_click), tooltip)
}

#[component]
pub fn icon_button(
    #[prop(into)] id: String,
    icon: IconName,
    #[prop(into)] label: String,
    icon_size: Option<f32>,
    #[prop(default)] disabled: bool,
    #[prop(default)] selected: bool,
    #[prop(default)] keyboard_focusable: bool,
    #[prop(default, into)] on_click: Callback,
) -> AnyView {
    let theme = UiTheme::current();
    let enabled = !disabled;
    let state = signal(Interactions::default());
    let foreground = move || {
        if selected || (enabled && state.get().hovered) { theme.text.get() } else { theme.text_muted.get() }
    };
    let root = view! {
        <div test_id=id flex flex_none items_center justify_center size=tokens::ICON_BUTTON_SIZE
             rounded={tokens::ICON_BUTTON_SIZE / 2.0} track_interaction=state when:opacity=(disabled, 0.4)
             bg={move || {
                 let state = state.get();
                 if selected || state.focus_visible() {
                     theme.surface_selected.get()
                 } else if enabled && state.hovered {
                     theme.surface_hover.get()
                 } else {
                     Color::TRANSPARENT
                 }
             }}>
            {Icon::new(icon, foreground).size(icon_size.unwrap_or(tokens::ICON_SIZE))}
        </div>
    };
    hinted(pressable(root, keyboard_focusable, enabled, on_click), enabled.then_some(label))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CheckState {
    #[default]
    Unchecked,
    Checked,
    Indeterminate,
}

#[component]
pub fn checkbox(
    #[prop(into)] id: String,
    state: CheckState,
    #[prop(into)] tooltip: Option<String>,
    #[prop(default)] disabled: bool,
    #[prop(default, into)] on_toggle: Callback,
) -> AnyView {
    let theme = UiTheme::current();
    let enabled = !disabled;
    let interaction = signal(Interactions::default());
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
        painter.stroke_round(&path, size.width / 6.0, theme.text.get());
    })
    .size_full();
    let root = view! {
        <div test_id=id flex_none size=tokens::CHECKBOX_SIZE rounded=2.0 border=px(1.0) bg=theme.canvas
             track_interaction=interaction when:opacity=(disabled, 0.4)
             border_color={move || {
                 let state = interaction.get();
                 if enabled && (state.hovered || state.focus_visible()) {
                     theme.accent.get()
                 } else {
                     theme.text_muted.get()
                 }
             }}>
            {mark}
        </div>
    };
    hinted(pressable(root, true, enabled, on_toggle), tooltip.filter(|_| enabled))
}

#[component]
pub fn segmented_control(#[prop(into)] id: String) -> Div {
    view! {
        <div test_id=id flex items_center gap=2.0 p=2.0 rounded=tokens::RADIUS_CONTROL bg=UiTheme::current().chrome/>
    }
}

#[component]
pub fn segment(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    icon: Option<IconName>,
    #[prop(into)] detail: Option<String>,
    #[prop(default)] selected: bool,
    #[prop(default)] disabled: bool,
    #[prop(default, into)] on_click: Callback,
) -> AnyView {
    let theme = UiTheme::current();
    let hovered = signal(false);
    let foreground = move || if selected || hovered.get() { theme.text.get() } else { theme.text_muted.get() };
    let padding = if icon.is_some() { tokens::CONTROL_PADDING_X_SMALL } else { tokens::CONTROL_PADDING_X_MEDIUM };
    let detail = detail.filter(|_| icon.is_some()).map(|detail| {
        view! {
            <div test_id={format!("{id}-detail")}>
                <text small color=foreground>{detail}</text>
            </div>
        }
    });
    let (caption, hint) = if icon.is_some() { (None, Some(label)) } else { (Some(label), None) };
    let root = view! {
        <div test_id=id h=24.0 flex items_center gap=4.0 px=padding rounded=tokens::RADIUS_INNER
             when:bg=(selected, theme.surface_selected) when:opacity=(disabled, 0.4)
             on_hover={move |inside| {
                 hovered.set_if_changed(inside);
             }}>
            {icon.map(|icon| Icon::new(icon, foreground))}
            {detail}
            {caption.map(|caption| view! { <text small color=foreground>{caption}</text> })}
        </div>
    };
    hinted(pressable(root, true, !disabled, on_click), hint)
}

#[component]
pub fn disclosure(
    #[prop(into)] id: String,
    expanded: bool,
    #[prop(default, into)] on_toggle: Callback,
    children: impl View,
) -> Div {
    let icon = if expanded { IconName::ChevronDown } else { IconName::ChevronRight };
    let root = view! {
        <div test_id=id flex items_center gap=6.0>
            {Icon::new(icon, UiTheme::current().text_muted).size(tokens::ICON_SIZE_SMALL)}
            <div>{children}</div>
        </div>
    };
    pressable(root, true, true, on_toggle)
}

#[component]
pub fn badge(
    #[prop(into)] id: String,
    #[prop(into)] label: String,
    tone: Option<NoticeTone>,
    icon: Option<IconName>,
) -> Div {
    let theme = UiTheme::current();
    let tone = tone.unwrap_or(NoticeTone::Neutral);
    let fg = move || theme.notice_style(tone).foreground;
    view! {
        <div test_id=id flex items_center gap=4.0 px=6.0 rounded=tokens::RADIUS_INNER
             bg={move || theme.notice_style(tone).background.unwrap_or(Color::TRANSPARENT)}>
            {icon.map(|icon| Icon::new(icon, fg).size(tokens::ICON_SIZE_SMALL))}
            <text caption color=fg>{label}</text>
        </div>
    }
}

#[component]
pub fn muted_text(#[prop(into)] children: String) -> Text {
    text(children).small().muted()
}

#[component]
pub fn section_header(
    #[prop(into)] id: String,
    #[prop(into)] title: String,
    expanded: bool,
    #[prop(into)] detail: Option<String>,
    icon: Option<(IconName, Color)>,
    badge: Option<AnyView>,
    action: Option<AnyView>,
    #[prop(default)] actions_always_visible: bool,
    #[prop(default, into)] on_toggle: Callback,
) -> Div {
    let hover = signal(false);
    let action = action.map(|action| {
        view! {
            <div opacity={move || if actions_always_visible || hover.get() { 1.0 } else { 0.0 }}>{action}</div>
        }
    });
    let toggle = format!("{id}-toggle");
    view! {
        <div test_id=id flex items_center justify_between px=8.0 h=tokens::GROUP_HEIGHT
             bg=UiTheme::current().chrome
             on_hover={move |inside| {
                 hover.set(inside);
             }}>
            <disclosure id=toggle expanded on_toggle>
                <div flex items_center gap=6.0>
                    {icon.map(|(icon, color)| Icon::new(icon, color))}
                    <text title primary>{title}</text>
                    {detail.map(|detail| view! { <muted_text>{detail}</muted_text> })}
                    {badge}
                </div>
            </disclosure>
            {action}
        </div>
    }
}

styles! {
    pub trait SurfaceExt {
        surface(target) => {
            let theme = UiTheme::current();
            target.bg(theme.surface).border(px(1.0)).border_color(theme.border_subtle).rounded(tokens::RADIUS_CONTROL)
        };
        floating(target) => {
            let theme = UiTheme::current();
            target
                .bg(theme.elevated)
                .border(px(1.0))
                .border_color(theme.border)
                .rounded(tokens::RADIUS_FLOATING)
                .shadow(BoxShadow::md())
        };
    }
}

#[component]
pub fn surface(#[prop(default)] padded: bool, #[prop(default)] raised: bool, #[prop(default)] selected: bool) -> Div {
    let root = div().flex().col();
    let root = if raised { root.floating() } else { root.surface() };
    root.when(padded, |root| root.p(12.0)).when(selected, |root| root.bg(UiTheme::current().surface_selected))
}

#[must_use]
pub fn hover_group(id: &str) -> String {
    format!("{id}-row")
}

#[component]
pub fn list_row(
    #[prop(into)] id: String,
    #[prop(default)] depth: usize,
    #[prop(default)] state: SelectionState,
    #[prop(reactive)] selection: Option<Prop<SelectionState>>,
    #[prop(default, into)] on_click: Callback,
) -> Div {
    let theme = UiTheme::current();
    let selection =
        memo(
            move || {
                if state == SelectionState::Disabled { state } else { selection.as_ref().map_or(state, Prop::get) }
            },
        );
    let hover = signal(false);
    let root = view! {
        <div flex items_center group=hover_group(&id) test_id=id min_h=tokens::ROW_HEIGHT
             pl={8.0 + crate::tree::indent(depth)} pr=8.0 gap=6.0 rounded=tokens::RADIUS_INNER
             on_hover={move |inside| {
                 hover.set(inside);
             }}
             bg={move || match selection.get() {
                 SelectionState::None if hover.get() => theme.surface_hover.get(),
                 SelectionState::None | SelectionState::Disabled => Color::TRANSPARENT,
                 state => theme.selection_style(state).background.unwrap_or(Color::TRANSPARENT),
             }}
             text_color={move || theme.selection_style(selection.get()).foreground}/>
    };
    pressable(root, true, state != SelectionState::Disabled, on_click)
        .enabled(move || selection.get() != SelectionState::Disabled)
}

#[component]
pub fn action_bar() -> Div {
    view! {
        <div flex items_center justify_end gap=8.0 min_h=tokens::BAR_HEIGHT px=12.0 bg=UiTheme::current().chrome/>
    }
}

#[component]
pub fn backdrop(
    #[prop(into)] id: String,
    #[prop(default)] align_top: bool,
    anchor_below: Option<Signal<Rect>>,
    #[prop(default, into)] on_dismiss: Callback,
    children: impl View,
) -> impl View {
    let theme = UiTheme::current();
    overlay(move || {
        let inner = view! {
            <div flex col min_h=0.0 max_h=pct(100.0) max_w=pct(100.0) on_pointer_down=PointerEvent::stop_propagation>
                {children}
            </div>
        };
        let root = view! {
            <div test_id=id absolute inset=0.0 flex col focus_scope autofocus tab_index=-1
                 on_pointer_down={move |_| on_dismiss.call()}
                 on_key_down={move |event| {
                     if event.key == Key::Tab {
                         return;
                     }
                     if event.key == Key::Escape {
                         on_dismiss.call();
                     }
                     event.stop_propagation();
                 }}/>
        };
        let (root, inner) = if let Some(anchor) = anchor_below {
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
            let root = root.items_center().p(16.0).bg(theme.scrim);
            let root =
                if align_top { root.justify_start().pt(tokens::MODAL_TOP_OFFSET) } else { root.justify_center() };
            (root, inner)
        };
        root.child(IndicatorMark::occluder("modal-occluder").absolute().inset(0.0)).child(inner)
    })
}

#[must_use]
pub fn modal_width(size: ModalSize) -> Length {
    Length::design(match size {
        ModalSize::Compact => tokens::MODAL_WIDTH_COMPACT,
        ModalSize::Medium => tokens::MODAL_WIDTH,
        ModalSize::Wide => tokens::MODAL_WIDTH_WIDE,
    })
}

#[component]
pub fn modal(
    #[prop(into)] id: String,
    #[prop(into)] title: String,
    #[prop(into)] hint: Option<String>,
    #[prop(default)] size: ModalSize,
    width: Option<Length>,
    height: Option<Length>,
    #[prop(default)] align_top: bool,
    #[prop(default, into)] on_dismiss: Callback,
    children: impl View,
) -> impl View {
    view! {
        <backdrop id={format!("{id}-backdrop")} align_top=align_top on_dismiss>
            <div test_id=id flex col gap=8.0 p=16.0 w={width.unwrap_or_else(|| modal_width(size))} max_w=pct(100.0)
                 h?=height overflow_hidden floating>
                <text heading primary>{title}</text>
                {hint.map(|hint| view! { <muted_text>{hint}</muted_text> })}
                <div flex col min_h=0.0>{children}</div>
            </div>
        </backdrop>
    }
}

#[component]
pub fn empty_state(icon: IconName, #[prop(into)] title: String, #[prop(into)] detail: String) -> Div {
    view! {
        <div flex col items_center justify_center p=16.0 gap=6.0>
            {Icon::new(icon, UiTheme::current().text_muted)}
            <text title primary>{title}</text>
            <muted_text>{detail}</muted_text>
        </div>
    }
}

#[component]
pub fn notification(#[prop(into)] message: String, tone: NoticeTone) -> Div {
    let theme = UiTheme::current();
    let fg = move || theme.notice_style(tone).foreground;
    let icon = match tone {
        NoticeTone::Error | NoticeTone::Warning => IconName::TriangleAlert,
        NoticeTone::Positive => IconName::CircleCheck,
        _ => IconName::Info,
    };
    view! {
        <div flex items_center gap=8.0 p=8.0 rounded=tokens::RADIUS_CONTROL
             bg={move || theme.notice_style(tone).background.unwrap_or(Color::TRANSPARENT)}>
            {Icon::new(icon, fg)}
            <text body color=fg>{message}</text>
        </div>
    }
}
