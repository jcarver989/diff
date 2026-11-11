use crate::keymap::key_binding;
use rouge_core::{Div, root_font_size, set_design_unit};
use rouge_reactive::{Signal, effect, expect_context, provide_context, signal};

pub const DEFAULT_INTERFACE_SIZE: f32 = if cfg!(target_os = "macos") { 16.0 } else { 18.0 };
pub const MIN_INTERFACE_SIZE: f32 = 10.0;
pub const MAX_INTERFACE_SIZE: f32 = 32.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeAction {
    Increase,
    Decrease,
    Reset,
}

#[derive(Debug, Clone, Copy)]
pub struct InterfaceSize {
    saved: Signal<f32>,
    preview: Signal<Option<f32>>,
    default: f32,
}

impl InterfaceSize {
    #[must_use]
    pub fn install(default: f32) -> Self {
        let default = clamp(default);
        set_design_unit(Some(crate::tokens::BASE_SIZE));
        let size = Self { saved: signal(default), preview: signal(None), default };
        let root = root_font_size();
        effect(move || {
            root.set_if_changed(size.get());
        });
        provide_context(size);
        size
    }

    #[must_use]
    pub fn current() -> Self {
        expect_context()
    }
    #[must_use]
    pub fn get(self) -> f32 {
        self.preview.get().unwrap_or_else(|| self.saved.get())
    }
    #[must_use]
    pub fn committed(self) -> f32 {
        self.saved.get()
    }
    pub fn set(self, value: f32) {
        rouge_reactive::batch(|| {
            self.saved.set_if_changed(clamp(value));
            self.cancel();
        });
    }
    pub fn preview(self, value: f32) {
        self.preview.set_if_changed(Some(clamp(value)));
    }
    pub fn cancel(self) {
        self.preview.set_if_changed(None);
    }
    pub fn commit(self) {
        self.set(self.get());
    }
    pub fn apply(self, action: SizeAction) {
        self.set(match action {
            SizeAction::Increase => self.get() + 1.0,
            SizeAction::Decrease => self.get() - 1.0,
            SizeAction::Reset => self.default,
        });
    }
    #[must_use]
    pub fn bind(self, root: Div) -> Div {
        use SizeAction::{Decrease, Increase, Reset};
        [("secondary-=", Increase), ("secondary-shift-=", Increase), ("secondary--", Decrease), ("secondary-0", Reset)]
            .into_iter()
            .fold(root, |root, (keys, action)| {
                key_binding(keys)
                    .map_or_else(|_| unreachable!("built-in chord: {keys}"), |chord| root.bind_key(chord, action))
            })
            .on_action(move |action: &SizeAction| self.apply(*action))
    }
}

fn clamp(value: f32) -> f32 {
    if value.is_nan() { DEFAULT_INTERFACE_SIZE } else { value.clamp(MIN_INTERFACE_SIZE, MAX_INTERFACE_SIZE).round() }
}
