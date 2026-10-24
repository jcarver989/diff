use clankerdiff_theme::{Appearance, ReviewTheme, ThemePair};
use std::{env, fs, io, path::PathBuf};

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
        Self::new(user_file())
    }

    #[must_use]
    pub fn pair(&self) -> ThemePair {
        self.file
            .as_ref()
            .and_then(|file| fs::read(file).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
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
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            file,
            serde_json::to_vec_pretty(&pair).map_err(io::Error::other)?,
        )
    }
}

fn user_file() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Application Support/ClankerDiff/theme.json"))
    } else if cfg!(target_os = "windows") {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("ClankerDiff/theme.json"))
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|root| root.join("clankerdiff/theme.json"))
    }
}
