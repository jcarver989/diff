use clankerdiff_core::ViewMode;
use clankerdiff_ui_core::{Appearance, ReviewTheme, ThemePair};
use rouge::{
    core::appearance,
    persist::persisted,
    reactive::{ReadSignal, Signal, effect, signal},
};
use rouge_widgets::{DEFAULT_INTERFACE_SIZE, InterfaceSize};

#[derive(Clone, Copy)]
pub struct Preferences {
    appearance: ReadSignal<Appearance>,
    themes: Signal<ThemePair>,
    modes: Signal<ViewMode>,
    sizes: Signal<Option<f32>>,
    wraps: Signal<Option<bool>>,
}

impl Preferences {
    #[must_use]
    pub fn load() -> Self {
        Self {
            appearance: appearance(),
            themes: persisted("theme", signal(ThemePair::default())),
            modes: persisted("view_mode", signal(ViewMode::default())),
            sizes: persisted("interface_size", signal(None)),
            wraps: persisted("wrap", signal(None)),
        }
    }

    #[must_use]
    pub fn theme(&self) -> ReviewTheme {
        ReviewTheme::builtin(self.themes.get().id(self.appearance.get())).unwrap_or_default()
    }

    pub fn remember_theme(&self, id: &str) {
        let appearance = self.appearance.get();
        if self.themes.with_untracked(|pair| pair.id(appearance) != id) {
            self.themes.update(|pair| pair.set(appearance, id));
        }
    }

    #[must_use]
    pub fn view_mode(&self) -> ViewMode {
        self.modes.get()
    }

    pub fn remember_mode(&self, mode: ViewMode) {
        self.modes.set_if_changed(mode);
    }

    #[must_use]
    pub fn wrap(&self) -> bool {
        self.wraps.get().unwrap_or(true)
    }

    pub fn remember_wrap(&self, wrap: bool) {
        self.wraps.set_if_changed(Some(wrap));
    }

    #[must_use]
    pub fn interface_size(&self) -> InterfaceSize {
        let size = InterfaceSize::install(DEFAULT_INTERFACE_SIZE);
        let saved = self.sizes;
        effect(move || {
            if let Some(value) = saved.get() {
                size.set(value);
            }
        });
        effect(move || {
            saved.set_if_changed(Some(size.committed()));
        });
        size
    }
}
