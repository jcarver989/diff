//! Drives a window from a script and renders it to PNGs, without needing screen-recording or
//! accessibility permissions. Each line of the script is one step:
//!
//! - `wait <milliseconds>`
//! - `type <text>`: types text into the focused view; `\n` is Enter
//! - `keys <keystroke>...`: dispatches keystrokes as if typed, such as `cmd-t` or `ctrl-c`
//! - `action <name>`: dispatches an action by name, such as `workspace::NewTab`
//! - `click <x> <y> [count]`: clicks the left button at a point in the window, in pixels
//! - `drag <x> <y> <x> <y>`: drags the left button between two points
//! - `shot <path>`: renders the window to a PNG
//! - `quit`

use crate::run_loop;
use gpui::{
    AnyWindowHandle, App, AppContext, AsyncApp, Keystroke, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, PlatformInput, Point, point, px,
};
use std::time::Duration;

pub fn run(window: impl Into<AnyWindowHandle>, script: String, cx: &mut App) {
    let window = window.into();
    cx.spawn(async move |cx| {
        for line in script
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
        {
            let (command, argument) = line.split_once(' ').unwrap_or((line, ""));
            match command {
                "wait" => {
                    let millis = argument.parse().unwrap_or(500);
                    cx.background_executor()
                        .timer(Duration::from_millis(millis))
                        .await;
                }
                "type" => {
                    for keystroke in typed(&argument.replace("\\n", "\n")) {
                        let _ = cx.update_window(window, |_, window, cx| {
                            window.dispatch_keystroke(keystroke, cx)
                        });
                    }
                }
                "keys" => {
                    for keystroke in argument.split_whitespace() {
                        match Keystroke::parse(keystroke) {
                            Ok(keystroke) => {
                                let _ = cx.update_window(window, |_, window, cx| {
                                    window.dispatch_keystroke(keystroke, cx)
                                });
                                settle(cx).await;
                                draw(window, cx);
                            }
                            Err(error) => eprintln!("script: bad keystroke `{keystroke}`: {error}"),
                        }
                    }
                }
                "action" => {
                    let _ = cx.update_window(window, |_, window, cx| {
                        match cx.build_action(argument, None) {
                            Ok(action) => window.dispatch_action(action, cx),
                            Err(error) => eprintln!("script: unknown action `{argument}`: {error}"),
                        }
                    });
                    settle(cx).await;
                }
                "click" => {
                    if let [x, y, ..] = numbers(argument).as_slice() {
                        let count = argument
                            .split_whitespace()
                            .nth(2)
                            .and_then(|count| count.parse().ok())
                            .unwrap_or(1);
                        let at = point(px(*x), px(*y));
                        for click in 1..=count {
                            mouse(window, cx, press(at, click));
                            mouse(window, cx, release(at, click));
                        }
                    }
                }
                "drag" => {
                    if let [x1, y1, x2, y2] = numbers(argument).as_slice() {
                        let (from, to) = (point(px(*x1), px(*y1)), point(px(*x2), px(*y2)));
                        mouse(window, cx, press(from, 1));
                        for step in 1..=8u8 {
                            let position = from + (to - from) * (f32::from(step) / 8.0);
                            let event = MouseMoveEvent {
                                position,
                                pressed_button: Some(MouseButton::Left),
                                modifiers: Modifiers::default(),
                            };
                            mouse(window, cx, PlatformInput::MouseMove(event));
                        }
                        mouse(window, cx, release(to, 1));
                    }
                }
                "shot" => {
                    draw(window, cx);
                    match cx.update_window(window, |_, window, _| window.render_to_image()) {
                        Ok(Ok(image)) => match image.save(argument) {
                            Ok(()) => eprintln!("script: saved {argument}"),
                            Err(error) => eprintln!("script: failed to save {argument}: {error}"),
                        },
                        Ok(Err(error)) => eprintln!("script: failed to render: {error}"),
                        Err(error) => eprintln!("script: failed to reach the window: {error}"),
                    }
                }
                "quit" => {
                    cx.update(run_loop::stop);
                    return;
                }
                _ => eprintln!("script: unknown step `{line}`"),
            }
            draw(window, cx);
        }
    })
    .detach();
}

fn typed(text: &str) -> Vec<Keystroke> {
    text.chars()
        .filter_map(|ch| match ch {
            '\n' => Keystroke::parse("enter").ok(),
            _ => Some(Keystroke {
                modifiers: Modifiers::default(),
                key: ch.to_string(),
                key_char: Some(ch.to_string()),
            }),
        })
        .collect()
}

fn numbers(argument: &str) -> Vec<f32> {
    argument
        .split_whitespace()
        .filter_map(|number| number.parse().ok())
        .collect()
}

fn press(position: Point<Pixels>, click_count: usize) -> PlatformInput {
    PlatformInput::MouseDown(MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::default(),
        click_count,
        first_mouse: false,
    })
}

fn release(position: Point<Pixels>, click_count: usize) -> PlatformInput {
    PlatformInput::MouseUp(MouseUpEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::default(),
        click_count,
    })
}

fn mouse(window: AnyWindowHandle, cx: &mut AsyncApp, event: PlatformInput) {
    let _ = cx.update_window(window, |_, window, cx| {
        window.dispatch_event(event, cx);
    });
    draw(window, cx);
}

fn draw(window: AnyWindowHandle, cx: &mut AsyncApp) {
    let _ = cx.update_window(window, |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

async fn settle(cx: &mut AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(50))
        .await;
}
