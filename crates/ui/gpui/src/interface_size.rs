use gpui::{App, Global, KeyBinding, Window, actions, px};

actions!(interface_size, [IncreaseInterfaceSize, DecreaseInterfaceSize, ResetInterfaceSize]);

pub const DEFAULT_INTERFACE_SIZE: f32 = if cfg!(target_os = "macos") { 16.0 } else { 18.0 };
pub const MIN_INTERFACE_SIZE: f32 = 10.0;
pub const MAX_INTERFACE_SIZE: f32 = 32.0;
const STEP: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InterfaceSize {
    size: f32,
    default: f32,
}

impl Global for InterfaceSize {}

impl InterfaceSize {
    pub fn init(default: f32, cx: &mut App) {
        let default = clamp(default);
        cx.set_global(Self { size: default, default });
        cx.bind_keys([
            KeyBinding::new("secondary-=", IncreaseInterfaceSize, None),
            KeyBinding::new("secondary-+", IncreaseInterfaceSize, None),
            KeyBinding::new("secondary--", DecreaseInterfaceSize, None),
            KeyBinding::new("secondary-0", ResetInterfaceSize, None),
        ]);
        cx.on_action(|_: &IncreaseInterfaceSize, cx| Self::set(Self::get(cx) + STEP, cx));
        cx.on_action(|_: &DecreaseInterfaceSize, cx| Self::set(Self::get(cx) - STEP, cx));
        cx.on_action(|_: &ResetInterfaceSize, cx| {
            let default = cx.try_global::<Self>().map_or(DEFAULT_INTERFACE_SIZE, |size| size.default);
            Self::set(default, cx);
        });
    }

    #[must_use]
    pub fn get(cx: &App) -> f32 {
        cx.try_global::<Self>().map_or(DEFAULT_INTERFACE_SIZE, |size| size.size)
    }

    pub fn set(size: f32, cx: &mut App) {
        let size = clamp(size);
        let default = cx.try_global::<Self>().map_or(DEFAULT_INTERFACE_SIZE, |current| current.default);
        if cx.try_global::<Self>().is_some_and(|current| (current.size - size).abs() < f32::EPSILON) {
            return;
        }
        cx.set_global(Self { size, default });
        cx.refresh_windows();
    }

    pub fn apply(window: &mut Window, cx: &App) {
        window.set_rem_size(px(Self::get(cx)));
    }
}

fn clamp(size: f32) -> f32 {
    size.clamp(MIN_INTERFACE_SIZE, MAX_INTERFACE_SIZE).round()
}
