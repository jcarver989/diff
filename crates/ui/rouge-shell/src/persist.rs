use serde::{Serialize, de::DeserializeOwned};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::sync::watch;

#[derive(Clone)]
pub struct Store<T> {
    values: watch::Sender<T>,
}

impl<T: Serialize + DeserializeOwned + Clone + Default + Send + Sync + 'static> Store<T> {
    #[must_use]
    pub fn load(
        file: Option<PathBuf>,
        runtime: &tokio::runtime::Handle,
    ) -> (Self, tokio::task::JoinHandle<io::Result<()>>) {
        let state: T = file.as_deref().and_then(read).unwrap_or_default();
        let (values, mut changes) = watch::channel(state);
        let flush = runtime.spawn(async move {
            let Some(file) = file else { return Ok(()) };
            while changes.changed().await.is_ok() {
                loop {
                    let timeout = tokio::time::sleep(Duration::from_millis(400));
                    tokio::select! {
                        result = changes.changed() => if result.is_err() { break; },
                        () = timeout => break,
                    }
                }
                let state = changes.borrow_and_update().clone();
                let path = file.clone();
                tokio::task::spawn_blocking(move || write(&path, &state)).await??;
            }
            Ok(())
        });
        (Self { values }, flush)
    }

    #[must_use]
    pub fn state(&self) -> T {
        self.values.borrow().clone()
    }

    pub fn update(&self, change: impl FnOnce(&mut T) -> bool) {
        self.values.send_if_modified(change);
    }
}

#[must_use]
pub fn config_dir(name: &str) -> Option<PathBuf> {
    if let Some(dir) = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|dir| dir.is_absolute()) {
        return Some(dir.join(name.to_lowercase()));
    }
    if cfg!(target_os = "macos") {
        env::home_dir().map(|home| home.join("Library/Application Support").join(name))
    } else if cfg!(target_os = "windows") {
        env::var_os("APPDATA").map(|root| PathBuf::from(root).join(name))
    } else {
        env::home_dir().map(|home| home.join(".config").join(name.to_lowercase()))
    }
}

#[must_use]
pub fn read<T: DeserializeOwned>(file: &Path) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(file).ok()?).ok()
}

pub fn write<T: Serialize>(file: &Path, value: &T) -> io::Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let temporary = file.with_extension("tmp");
    fs::write(&temporary, json)?;
    fs::rename(temporary, file)
}
