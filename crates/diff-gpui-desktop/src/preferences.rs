use clankerdiff_core::ViewMode;
use clankerdiff_gpui_shell::persist::{self, config_dir};
use clankerdiff_theme::{Appearance, ReviewTheme, ThemePair};
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
        self.file
            .as_deref()
            .and_then(persist::read)
            .unwrap_or_default()
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
        self.file
            .as_deref()
            .and_then(persist::read)
            .unwrap_or_default()
    }

    pub fn remember(&self, mode: ViewMode) -> io::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        persist::write(file, &mode)
    }
}
