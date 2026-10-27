use crate::{InterfaceSize, tokens::BASE_SIZE};
use gpui::{
    Action, AnyElement, AnyWindowHandle, App, Bounds, Context, Entity, EventEmitter, InputEvent, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, Render, ScrollDelta,
    ScrollWheelEvent, TestAppContext, VisualTestContext, Window, WindowBounds, WindowHandle, WindowOptions, div, point,
    prelude::*, px, size,
};
use std::{error::Error, ops::Deref};

type RenderFn = Box<dyn Fn(&mut Window, &mut App) -> AnyElement + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error("no painted element is tagged `{0}`")]
    MissingElement(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct WindowDriver {
    window: AnyWindowHandle,
}

impl WindowDriver {
    #[must_use]
    pub fn new(window: impl Into<AnyWindowHandle>) -> Self {
        Self { window: window.into() }
    }

    #[must_use]
    pub const fn window(&self) -> AnyWindowHandle {
        self.window
    }

    pub fn draw(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| {
            window.draw(cx).clear(cx);
            if window.simulate_next_frame(cx) > 0 {
                window.draw(cx).clear(cx);
            }
        })
        .expect("draw the GPUI test window");
        cx.run_until_parked();
    }

    pub fn bounds(&self, cx: &mut TestAppContext, selector: &'static str) -> Option<Bounds<Pixels>> {
        self.draw(cx);
        VisualTestContext::from_window(self.window, cx).debug_bounds(selector)
    }

    pub fn center(&self, cx: &mut TestAppContext, selector: &'static str) -> Result<Point<Pixels>, HarnessError> {
        self.bounds(cx, selector).map(|bounds| bounds.center()).ok_or(HarnessError::MissingElement(selector))
    }

    pub fn rem_size(&self, cx: &mut TestAppContext) -> Pixels {
        self.draw(cx);
        cx.update_window(self.window, |_, window, _| window.rem_size()).expect("read the GPUI test window")
    }

    pub fn click(&self, cx: &mut TestAppContext, selector: &'static str) -> Result<(), HarnessError> {
        self.click_times(cx, selector, MouseButton::Left, 1)
    }

    pub fn double_click(&self, cx: &mut TestAppContext, selector: &'static str) -> Result<(), HarnessError> {
        self.click_times(cx, selector, MouseButton::Left, 2)
    }

    pub fn right_click(&self, cx: &mut TestAppContext, selector: &'static str) -> Result<(), HarnessError> {
        self.click_times(cx, selector, MouseButton::Right, 1)?;
        self.draw(cx);
        Ok(())
    }

    pub fn hover(&self, cx: &mut TestAppContext, selector: &'static str) -> Result<(), HarnessError> {
        let position = self.center(cx, selector)?;
        VisualTestContext::from_window(self.window, cx).simulate_mouse_move(position, None, Modifiers::default());
        self.draw(cx);
        Ok(())
    }

    pub fn drag(&self, cx: &mut TestAppContext, selector: &'static str, to: Point<Pixels>) -> Result<(), HarnessError> {
        let from = self.center(cx, selector)?;
        let mut visual = VisualTestContext::from_window(self.window, cx);
        visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(from + point(px(4.0), px(0.0)), MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
        visual.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
        self.draw(cx);
        Ok(())
    }

    pub fn scroll(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
        delta: ScrollDelta,
    ) -> Result<(), HarnessError> {
        let position = self.center(cx, selector)?;
        self.simulate_event(cx, ScrollWheelEvent { position, delta, ..ScrollWheelEvent::default() });
        Ok(())
    }

    pub fn focus_next(&self, cx: &mut TestAppContext) {
        cx.update_window(self.window, |_, window, cx| window.focus_next(cx)).expect("focus in the GPUI test window");
        self.draw(cx);
    }

    pub fn simulate_keystrokes(&self, cx: &mut TestAppContext, keystrokes: &str) {
        cx.simulate_keystrokes(self.window, keystrokes);
        self.draw(cx);
    }

    pub fn press_key(&self, cx: &mut TestAppContext, key: &str) {
        let keystroke = Keystroke::parse(key).expect("a valid test keystroke");
        self.simulate_event(
            cx,
            KeyDownEvent { keystroke: keystroke.clone(), is_held: false, prefer_character_input: false },
        );
        self.simulate_event(cx, KeyUpEvent { keystroke });
    }

    pub fn simulate_input(&self, cx: &mut TestAppContext, text: &str) {
        cx.simulate_input(self.window, text);
        self.draw(cx);
    }

    pub fn simulate_event(&self, cx: &mut TestAppContext, event: impl InputEvent) {
        cx.update_window(self.window, |_, window, cx| {
            window.dispatch_event(event.to_platform_input(), cx);
        })
        .expect("dispatch to the GPUI test window");
        self.draw(cx);
    }

    pub fn dispatch_action(&self, cx: &mut TestAppContext, action: &dyn Action) {
        cx.update_window(self.window, |_, window, cx| {
            window.dispatch_action(action.boxed_clone(), cx);
        })
        .expect("dispatch to the GPUI test window");
        self.draw(cx);
    }

    pub fn resize(&self, cx: &mut TestAppContext, width: f32, height: f32) {
        cx.simulate_window_resize(self.window, size(px(width), px(height)));
        self.draw(cx);
    }

    pub fn click_at(&self, cx: &mut TestAppContext, position: Point<Pixels>) {
        self.click_position(cx, position, MouseButton::Left, 1, Modifiers::default());
    }

    pub fn secondary_click_at(&self, cx: &mut TestAppContext, position: Point<Pixels>) {
        self.click_position(cx, position, MouseButton::Left, 1, Modifiers::secondary_key());
    }

    fn click_times(
        &self,
        cx: &mut TestAppContext,
        selector: &'static str,
        button: MouseButton,
        count: usize,
    ) -> Result<(), HarnessError> {
        let position = self.center(cx, selector)?;
        self.click_position(cx, position, button, count, Modifiers::default());
        Ok(())
    }

    fn click_position(
        &self,
        cx: &mut TestAppContext,
        position: Point<Pixels>,
        button: MouseButton,
        count: usize,
        modifiers: Modifiers,
    ) {
        let mut visual = VisualTestContext::from_window(self.window, cx);
        for click_count in 1..=count {
            visual.simulate_event(MouseDownEvent { position, modifiers, button, click_count, first_mouse: false });
            visual.simulate_event(MouseUpEvent { position, modifiers, button, click_count });
        }
        self.draw(cx);
    }
}

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
        Self { width: 800.0, height: 600.0, interface_size: BASE_SIZE }
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
            cx.open_window(options, |_, cx| cx.new(|_| HarnessRoot { render: Box::new(render) }))
                .expect("open GPUI test window")
        });
        let harness = ComponentHarness(WindowDriver::new(window));
        harness.draw(cx);
        harness
    }
}

pub struct ComponentHarness(WindowDriver);

impl Deref for ComponentHarness {
    type Target = WindowDriver;

    fn deref(&self) -> &WindowDriver {
        &self.0
    }
}

pub struct ViewHarness<T: 'static, U: 'static> {
    driver: WindowDriver,
    window: WindowHandle<ViewRoot<T, U>>,
    view: Entity<T>,
}

impl<T: Render + EventEmitter<U>, U: Clone + 'static> ViewHarness<T, U> {
    pub fn open(
        cx: &mut TestAppContext,
        window_options: WindowOptions,
        view: impl FnOnce(&mut Window, &mut App) -> Entity<T>,
    ) -> Self {
        let window = cx.update(|cx| {
            cx.open_window(window_options, |window, cx| {
                let view = view(window, cx);
                cx.new(|cx| {
                    cx.subscribe(&view, |root: &mut ViewRoot<T, U>, _, event: &U, _| {
                        root.events.push(event.clone());
                    })
                    .detach();
                    ViewRoot { view, events: Vec::new() }
                })
            })
            .expect("open GPUI test window")
        });
        cx.run_until_parked();
        let view = window.read_with(cx, |root, _| root.view.clone()).expect("read GPUI test root");
        Self { driver: WindowDriver::new(window), window, view }
    }

    pub fn run(
        &self,
        cx: &mut TestAppContext,
        test: impl FnOnce(&Self, &mut TestAppContext) -> Result<(), Box<dyn Error>>,
    ) {
        if let Err(error) = test(self, cx) {
            panic!("{error}");
        }
    }

    #[must_use]
    pub fn events(&self, cx: &TestAppContext) -> Vec<U> {
        self.window.read_with(cx, |root, _| root.events.clone()).expect("read recorded GPUI events")
    }

    #[must_use]
    pub const fn view(&self) -> &Entity<T> {
        &self.view
    }

    pub fn read<V>(&self, cx: &TestAppContext, read: impl FnOnce(&T, &App) -> V) -> V {
        self.view.read_with(cx, read)
    }

    pub fn update<V>(&self, cx: &mut TestAppContext, update: impl FnOnce(&mut T, &mut Context<T>) -> V) -> V {
        self.view.update(cx, update)
    }

    pub fn run_until(&self, cx: &mut TestAppContext, condition: impl Fn(&T, &App) -> bool) {
        loop {
            cx.run_until_parked();
            if self.read(cx, &condition) {
                return;
            }
            std::thread::yield_now();
        }
    }
}

impl<T, U> Deref for ViewHarness<T, U> {
    type Target = WindowDriver;

    fn deref(&self) -> &WindowDriver {
        &self.driver
    }
}

struct ViewRoot<T: 'static, U: 'static> {
    view: Entity<T>,
    events: Vec<U>,
}

impl<T: Render, U: 'static> Render for ViewRoot<T, U> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        InterfaceSize::apply(window, cx);
        div().size_full().child(self.view.clone())
    }
}
