use clankerdiff_core::ViewMode;
use clankerdiff_theme::ReviewTheme;
use std::{env, fs, io, path::PathBuf};

const THEME_FILE: &str = "theme";
const VIEW_MODE_FILE: &str = "terminal_view_mode.json";

fn path(file: &str) -> Option<PathBuf> {
    let dir = if cfg!(target_os = "macos") {
        env::var_os("HOME").map(PathBuf::from).map(|home| home.join("Library/Application Support/ClankerDiff"))
    } else if cfg!(target_os = "windows") {
        env::var_os("APPDATA").map(PathBuf::from).map(|root| root.join("ClankerDiff"))
    } else {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|root| root.join("clankerdiff"))
    };
    dir.map(|dir| dir.join(file))
}

fn load(file: &str) -> Option<String> {
    path(file).and_then(|path| fs::read_to_string(path).ok())
}

fn save(file: &str, contents: &str) -> io::Result<()> {
    let Some(path) = path(file) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

pub fn load_theme() -> ReviewTheme {
    load(THEME_FILE).and_then(|id| ReviewTheme::builtin(id.trim()).ok()).unwrap_or_default()
}

pub fn save_theme(id: &str) -> io::Result<()> {
    save(THEME_FILE, &format!("{id}\n"))
}

pub fn load_view_mode() -> ViewMode {
    load(VIEW_MODE_FILE).and_then(|mode| serde_json::from_str(&mode).ok()).unwrap_or_default()
}

pub fn save_view_mode(mode: ViewMode) -> io::Result<()> {
    save(VIEW_MODE_FILE, &serde_json::to_string(&mode)?)
}
