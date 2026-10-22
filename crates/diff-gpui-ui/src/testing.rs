use crate::{InterfaceSize, tokens::BASE_SIZE};
use gpui::{
    Action, AnyElement, AnyWindowHandle, App, Bounds, Context, Modifiers, MouseButton, Pixels,
    Point, Render, TestAppContext, VisualTestContext, Window, WindowBounds, WindowHandle,
    WindowOptions, div, point, prelude::*, px, size,
};
use std::error::Error;

type RenderFn = Box<dyn Fn(&mut Window, &mut App) -> AnyElement + 'static>;

struct HarnessRoot {
    render: RenderFn,
}

impl Render for HarnessRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        InterfaceSize::apply(window, cx);
        div().size_full().child((self.render)(window, cx))
    }
}

pub struct ComponentHarnessBuilder {
    pub width: f32,
    pub height: f32,
    pub interface_size: f32,
}

impl Default for ComponentHarnessBuilder {
    fn default() -> Self {
        Self {
            width: 800.0,
            height: 600.0,
            interface_size: BASE_SIZE,
        }
    }
}

impl ComponentHarnessBuilder {
    pub fn run(
        self,
        cx: &mut TestAppContext,
        render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
        test: impl FnOnce(&ComponentHarness, &mut TestAppContext) -> Result<(), Box<dyn Error>>,
    ) {
        let harness = self.build(cx, render);
        if let Err(error) = test(&harness, cx) {
            panic!("{error}");
        }
    }

    pub fn build(
        self,
        cx: &mut TestAppContext,
        render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> ComponentHarness {
        cx.update(|cx| InterfaceSize::init(self.interface_size, cx));
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.0), px(0.0)),
                size(px(self.width), px(self.height)),
            ))),
            ..WindowOptions::default()
        };
        let window: WindowHandle<HarnessRoot> = cx.update(|cx| {
            cx.open_window(options, |_, cx| {
                cx.new(|_| HarnessRoot {
                    render: Box::new(render),
                })
            })
            .expect("open GPUI test window")
        });
        let harness = ComponentHarness { window: *window };
        harness.draw(cx);
        harness
    }
}

pub struct ComponentHarness {
    window: AnyWindowHandle,
}

impl ComponentHarness {
    pub fn draw(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.draw(cx).clear(cx))
            .expect("draw GPUI test window");
        cx.run_until_parked();
    }

    pub fn bounds(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
    ) -> Option<Bounds<Pixels>> {
        self.draw(cx);
        VisualTestContext::from_window(self.window, cx).debug_bounds(selector)
    }

    pub fn rem_size(&self, cx: &mut TestAppContext) -> Pixels {
        self.draw(cx);
        cx.update_window(self.window, |_, window, _| window.rem_size())
            .expect("read the test window")
    }

    pub fn click(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
    ) -> Result<(), Box<dyn Error>> {
        self.click_times(cx, selector, 1)
    }

    pub fn double_click(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
    ) -> Result<(), Box<dyn Error>> {
        self.click_times(cx, selector, 2)
    }

    pub fn drag(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
        to: Point<Pixels>,
    ) -> Result<(), Box<dyn Error>> {
        let from = self.center(cx, selector)?;
        let mut visual = VisualTestContext::from_window(self.window, cx);
        visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(
            from + point(px(4.0), px(0.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        visual.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
        self.draw(cx);
        Ok(())
    }

    pub fn simulate_keystrokes(&self, cx: &mut TestAppContext, keystrokes: &str) {
        cx.simulate_keystrokes(self.window, keystrokes);
        self.draw(cx);
    }

    pub fn dispatch_action(
        &self,
        cx: &mut TestAppContext,
        action: &dyn Action,
    ) -> Result<(), Box<dyn Error>> {
        cx.update_window(self.window, |_, window, cx| {
            window.dispatch_action(action.boxed_clone(), cx);
        })?;
        self.draw(cx);
        Ok(())
    }

    fn center(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
    ) -> Result<Point<Pixels>, Box<dyn Error>> {
        Ok(self
            .bounds(cx, selector)
            .ok_or_else(|| format!("missing {selector}"))?
            .center())
    }

    fn click_times(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
        count: usize,
    ) -> Result<(), Box<dyn Error>> {
        let position = self.center(cx, selector)?;
        let mut visual = VisualTestContext::from_window(self.window, cx);
        for click_count in 1..=count {
            visual.simulate_event(gpui::MouseDownEvent {
                position,
                modifiers: Modifiers::default(),
                button: MouseButton::Left,
                click_count,
                first_mouse: false,
            });
            visual.simulate_event(gpui::MouseUpEvent {
                position,
                modifiers: Modifiers::default(),
                button: MouseButton::Left,
                click_count,
            });
        }
        self.draw(cx);
        Ok(())
    }
}
