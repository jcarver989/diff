use gpui::{
    AnyElement, App, BoxShadow, CursorStyle, Decorations, Hsla, MouseButton, Pixels, Point,
    ResizeEdge, Size, Tiling, Window, WindowButton, WindowButtonLayout, WindowControlArea,
    WindowControls, div, point, prelude::*, px, rgb, transparent_black,
};
const CLIENT_INSET: Pixels = px(8.0);
const BORDER_SIZE: Pixels = px(1.0);
const TITLE_BAR_HEIGHT: Pixels = px(36.0);
const BUTTON_WIDTH: Pixels = px(42.0);

pub(crate) fn decorate(content: AnyElement, window: &mut Window, cx: &mut App) -> AnyElement {
    let decorations = window.window_decorations();
    let Decorations::Client { tiling } = decorations else {
        window.set_client_inset(px(0.0));
        return content;
    };

    window.set_client_inset(CLIENT_INSET);
    let window_size = window.window_bounds().get_bounds().size;
    let is_resizable = window.is_resizable();
    let controls = window.window_controls();
    let layout = cx.button_layout().unwrap_or(WindowButtonLayout {
        left: [None, None, None],
        right: [
            Some(WindowButton::Minimize),
            Some(WindowButton::Maximize),
            Some(WindowButton::Close),
        ],
    });
    let left_controls =
        supported_buttons(layout.left, controls, is_resizable, window.is_minimizable());
    let right_controls = supported_buttons(
        layout.right,
        controls,
        is_resizable,
        window.is_minimizable(),
    );

    let framed_content = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .overflow_hidden()
        .border_color(rgb(0x003b_424a))
        .when(!tiling.top, |element| element.border_t(BORDER_SIZE))
        .when(!tiling.bottom, |element| element.border_b(BORDER_SIZE))
        .when(!tiling.left, |element| element.border_l(BORDER_SIZE))
        .when(!tiling.right, |element| element.border_r(BORDER_SIZE))
        .when(!tiling.is_tiled(), |element| {
            element.shadow(vec![BoxShadow {
                color: Hsla {
                    h: 0.0,
                    s: 0.0,
                    l: 0.0,
                    a: 0.35,
                },
                blur_radius: px(6.0),
                spread_radius: px(-1.0),
                offset: point(px(0.0), px(1.0)),
                inset: false,
            }])
        })
        .child(title_bar(
            left_controls,
            right_controls,
            controls.maximize && is_resizable,
            window.is_maximized(),
        ))
        .child(div().flex_1().min_h_0().min_w_0().child(content));

    div()
        .id("desktop-window-backdrop")
        .size_full()
        .flex()
        .flex_col()
        .bg(transparent_black())
        .when(!tiling.top, |element| element.pt(CLIENT_INSET))
        .when(!tiling.bottom, |element| element.pb(CLIENT_INSET))
        .when(!tiling.left, |element| element.pl(CLIENT_INSET))
        .when(!tiling.right, |element| element.pr(CLIENT_INSET))
        .when(is_resizable, |element| {
            element.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                if let Some(edge) = resize_edge(event.position, window_size, CLIENT_INSET, tiling) {
                    cx.stop_propagation();
                    window.start_window_resize(edge);
                }
            })
        })
        .child(framed_content)
        .children(resize_hit_zones(
            window_size,
            CLIENT_INSET,
            tiling,
            is_resizable,
        ))
        .into_any_element()
}

fn title_bar(
    left_controls: Vec<WindowButton>,
    right_controls: Vec<WindowButton>,
    can_maximize: bool,
    is_maximized: bool,
) -> impl IntoElement {
    div()
        .id("desktop-title-bar")
        .window_control_area(WindowControlArea::Drag)
        .h(TITLE_BAR_HEIGHT)
        .w_full()
        .flex()
        .items_center()
        .bg(rgb(0x0018_1d23))
        .text_color(rgb(0x00d6_d9dc))
        .on_mouse_down(MouseButton::Left, move |event, window, _| {
            if can_maximize && event.click_count == 2 {
                window.zoom_window();
            } else {
                window.start_window_move();
            }
        })
        .child(window_button_group(left_controls, is_maximized))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_center()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child("Diff Review"),
        )
        .child(window_button_group(right_controls, is_maximized))
}

fn window_button_group(buttons: Vec<WindowButton>, is_maximized: bool) -> impl IntoElement {
    div()
        .h_full()
        .flex()
        .children(buttons.into_iter().map(|button| {
            let (glyph, area) = match button {
                WindowButton::Minimize => ("—", WindowControlArea::Min),
                WindowButton::Maximize if is_maximized => ("❐", WindowControlArea::Max),
                WindowButton::Maximize => ("□", WindowControlArea::Max),
                WindowButton::Close => ("×", WindowControlArea::Close),
            };
            div()
                .id(button.id())
                .window_control_area(area)
                .h_full()
                .w(BUTTON_WIDTH)
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .cursor(CursorStyle::PointingHand)
                .hover(move |element| {
                    if button == WindowButton::Close {
                        element.bg(rgb(0x00c4_2b3b))
                    } else {
                        element.bg(rgb(0x0029_3038))
                    }
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    match button {
                        WindowButton::Minimize => window.minimize_window(),
                        WindowButton::Maximize => window.zoom_window(),
                        WindowButton::Close => window.remove_window(),
                    }
                })
                .child(glyph)
        }))
}

fn supported_buttons(
    buttons: [Option<WindowButton>; 3],
    controls: WindowControls,
    is_resizable: bool,
    is_minimizable: bool,
) -> Vec<WindowButton> {
    buttons
        .into_iter()
        .flatten()
        .filter(|button| match button {
            WindowButton::Minimize => controls.minimize && is_minimizable,
            WindowButton::Maximize => controls.maximize && is_resizable,
            WindowButton::Close => true,
        })
        .collect()
}

fn resize_hit_zones(
    size: Size<Pixels>,
    inset: Pixels,
    tiling: Tiling,
    is_resizable: bool,
) -> Vec<AnyElement> {
    if !is_resizable {
        return Vec::new();
    }

    let mut zones = Vec::new();
    let mut push = |edge: ResizeEdge, left, top, width, height| {
        zones.push(
            div()
                .absolute()
                .left(left)
                .top(top)
                .w(width)
                .h(height)
                .cursor(cursor_for_edge(edge))
                .into_any_element(),
        );
    };

    if !tiling.top {
        push(ResizeEdge::Top, px(0.0), px(0.0), size.width, inset);
    }
    if !tiling.bottom {
        push(
            ResizeEdge::Bottom,
            px(0.0),
            size.height - inset,
            size.width,
            inset,
        );
    }
    if !tiling.left {
        push(ResizeEdge::Left, px(0.0), px(0.0), inset, size.height);
    }
    if !tiling.right {
        push(
            ResizeEdge::Right,
            size.width - inset,
            px(0.0),
            inset,
            size.height,
        );
    }
    if !tiling.top && !tiling.left {
        push(ResizeEdge::TopLeft, px(0.0), px(0.0), inset, inset);
    }
    if !tiling.top && !tiling.right {
        push(
            ResizeEdge::TopRight,
            size.width - inset,
            px(0.0),
            inset,
            inset,
        );
    }
    if !tiling.bottom && !tiling.left {
        push(
            ResizeEdge::BottomLeft,
            px(0.0),
            size.height - inset,
            inset,
            inset,
        );
    }
    if !tiling.bottom && !tiling.right {
        push(
            ResizeEdge::BottomRight,
            size.width - inset,
            size.height - inset,
            inset,
            inset,
        );
    }
    zones
}

fn resize_edge(
    position: Point<Pixels>,
    size: Size<Pixels>,
    inset: Pixels,
    tiling: Tiling,
) -> Option<ResizeEdge> {
    let on_top = !tiling.top && position.y < inset;
    let on_bottom = !tiling.bottom && position.y >= size.height - inset;
    let on_left = !tiling.left && position.x < inset;
    let on_right = !tiling.right && position.x >= size.width - inset;

    match (on_top, on_bottom, on_left, on_right) {
        (true, _, true, _) => Some(ResizeEdge::TopLeft),
        (true, _, _, true) => Some(ResizeEdge::TopRight),
        (_, true, true, _) => Some(ResizeEdge::BottomLeft),
        (_, true, _, true) => Some(ResizeEdge::BottomRight),
        (true, _, _, _) => Some(ResizeEdge::Top),
        (_, true, _, _) => Some(ResizeEdge::Bottom),
        (_, _, true, _) => Some(ResizeEdge::Left),
        (_, _, _, true) => Some(ResizeEdge::Right),
        _ => None,
    }
}

fn cursor_for_edge(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, size};

    #[test]
    fn detects_resize_edges_and_corners() {
        let size = size(px(100.0), px(80.0));
        let tiling = Tiling::default();
        let cases = [
            (point(px(1.0), px(1.0)), Some(ResizeEdge::TopLeft)),
            (point(px(50.0), px(1.0)), Some(ResizeEdge::Top)),
            (point(px(99.0), px(1.0)), Some(ResizeEdge::TopRight)),
            (point(px(99.0), px(40.0)), Some(ResizeEdge::Right)),
            (point(px(99.0), px(79.0)), Some(ResizeEdge::BottomRight)),
            (point(px(50.0), px(79.0)), Some(ResizeEdge::Bottom)),
            (point(px(1.0), px(79.0)), Some(ResizeEdge::BottomLeft)),
            (point(px(1.0), px(40.0)), Some(ResizeEdge::Left)),
            (point(px(50.0), px(40.0)), None),
        ];

        for (position, expected) in cases {
            assert_eq!(resize_edge(position, size, CLIENT_INSET, tiling), expected);
        }
    }

    #[test]
    fn tiled_edges_are_not_resizable() {
        let size = size(px(100.0), px(80.0));
        let tiling = Tiling {
            top: true,
            left: true,
            right: false,
            bottom: false,
        };

        assert_eq!(
            resize_edge(point(px(1.0), px(1.0)), size, CLIENT_INSET, tiling),
            None
        );
        assert_eq!(
            resize_edge(point(px(99.0), px(1.0)), size, CLIENT_INSET, tiling),
            Some(ResizeEdge::Right)
        );
        assert_eq!(
            resize_edge(point(px(1.0), px(79.0)), size, CLIENT_INSET, tiling),
            Some(ResizeEdge::Bottom)
        );
    }

    #[test]
    fn preserves_layout_order_and_filters_unsupported_controls() {
        let controls = WindowControls {
            fullscreen: true,
            maximize: false,
            minimize: true,
            window_menu: true,
        };
        let left = [
            Some(WindowButton::Close),
            Some(WindowButton::Minimize),
            None,
        ];
        let right = [
            Some(WindowButton::Maximize),
            Some(WindowButton::Close),
            None,
        ];

        assert_eq!(
            supported_buttons(left, controls, true, true),
            vec![WindowButton::Close, WindowButton::Minimize]
        );
        assert_eq!(
            supported_buttons(right, controls, true, true),
            vec![WindowButton::Close]
        );
        assert_eq!(
            supported_buttons(left, controls, true, false),
            vec![WindowButton::Close]
        );
    }
}
