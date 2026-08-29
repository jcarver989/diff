use diff_core::DiffTheme;
use std::{env, fs, io, path::PathBuf};

fn path() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library/Application Support/ClankerDiff/theme"))
    } else if cfg!(target_os = "windows") {
        env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("ClankerDiff/theme"))
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|root| root.join("clankerdiff/theme"))
    }
}

pub(crate) fn load_theme() -> DiffTheme {
    path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|id| DiffTheme::builtin(id.trim()).ok())
        .unwrap_or_default()
}

pub(crate) fn save_theme(id: &str) -> io::Result<()> {
    let Some(path) = path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format!("{id}\n"))
}
