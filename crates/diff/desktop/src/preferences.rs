use clankerdiff_core::ViewMode;
use clankerdiff_rouge_shell::persist::{self, config_dir};
use clankerdiff_ui_core::{Appearance, ReviewTheme, ThemePair};
use std::{io, path::PathBuf};

pub struct ThemeStore {
    file: Option<PathBuf>,
}

impl ThemeStore {
    #[must_use]
    pub const fn new(file: Option<PathBuf>) -> Self {
        Self { file }
    }

    #[must_use]
    pub fn user() -> Self {
        Self::new(config_dir("ClankerDiff").map(|dir| dir.join("theme.json")))
    }

    #[must_use]
    pub fn pair(&self) -> ThemePair {
        self.file.as_deref().and_then(persist::read).unwrap_or_default()
    }

    #[must_use]
    pub fn theme(&self, appearance: Appearance) -> ReviewTheme {
        ReviewTheme::builtin(self.pair().id(appearance)).unwrap_or_default()
    }

    pub fn remember(&self, appearance: Appearance, id: &str) -> io::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        let mut pair = self.pair();
        pair.set(appearance, id);
        persist::write(file, &pair)
    }
}

pub struct ViewModeStore {
    file: Option<PathBuf>,
}

impl ViewModeStore {
    #[must_use]
    pub const fn new(file: Option<PathBuf>) -> Self {
        Self { file }
    }

    #[must_use]
    pub fn user() -> Self {
        Self::new(config_dir("ClankerDiff").map(|dir| dir.join("view_mode.json")))
    }

    #[must_use]
    pub fn view_mode(&self) -> ViewMode {
        self.file.as_deref().and_then(persist::read).unwrap_or_default()
    }

    pub fn remember(&self, mode: ViewMode) -> io::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        persist::write(file, &mode)
    }
}

#[derive(Clone)]
pub(crate) struct Preferences {
    themes: persist::Store<ThemePair>,
    modes: persist::Store<ViewMode>,
    sizes: persist::Store<Option<f32>>,
}

impl Preferences {
    pub(crate) fn load(handle: &tokio::runtime::Handle) -> (Self, impl Future<Output = ()> + use<>) {
        let (themes, theme_flush) = persist::Store::load(ThemeStore::user().file, handle);
        let (modes, mode_flush) = persist::Store::load(ViewModeStore::user().file, handle);
        let (sizes, size_flush) =
            persist::Store::load(config_dir("ClankerDiff").map(|dir| dir.join("interface_size.json")), handle);
        (Self { themes, modes, sizes }, async {
            let _ = tokio::join!(theme_flush, mode_flush, size_flush);
        })
    }

    pub(crate) fn theme(&self) -> ReviewTheme {
        let appearance = clankerdiff_rouge::ui::theme::window_appearance();
        ReviewTheme::builtin(self.themes.state().id(appearance)).unwrap_or_default()
    }

    pub(crate) fn remember_theme(&self, id: &str) {
        let appearance = clankerdiff_rouge::ui::theme::window_appearance();
        self.themes.update(|pair| {
            if pair.id(appearance) == id {
                return false;
            }
            pair.set(appearance, id);
            true
        });
    }

    pub(crate) fn view_mode(&self) -> ViewMode {
        self.modes.state()
    }
    pub(crate) fn remember_mode(&self, mode: ViewMode) {
        self.modes.update(|value| {
            if *value == mode {
                false
            } else {
                *value = mode;
                true
            }
        });
    }

    pub(crate) fn interface_size(&self) -> clankerdiff_rouge::ui::InterfaceSize {
        use clankerdiff_rouge::ui::{InterfaceSize, interface_size::DEFAULT_INTERFACE_SIZE};
        let size = InterfaceSize::install(DEFAULT_INTERFACE_SIZE);
        if let Some(saved) = self.sizes.state() {
            size.set(saved);
        }
        let store = self.sizes.clone();
        clankerdiff_rouge_shell::rouge::reactive::effect(move || {
            let committed = size.committed();
            store.update(|saved| {
                if *saved == Some(committed) {
                    false
                } else {
                    *saved = Some(committed);
                    true
                }
            });
        });
        size
    }
}
