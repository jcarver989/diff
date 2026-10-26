use gpui::{App, AppContext, Context, Entity, Subscription, Task};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

const SAVE_DELAY: Duration = Duration::from_millis(400);

pub struct Store<T> {
    state: T,
    file: Option<PathBuf>,
    save: Option<Task<()>>,
    _quit: Subscription,
}

impl<T: Serialize + DeserializeOwned + Default + Clone + PartialEq + Send + 'static> Store<T> {
    #[must_use]
    pub fn load(file: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let state = file.as_deref().and_then(read).unwrap_or_default();
        let quit = cx.on_app_quit(|this: &mut Self, _| {
            this.save = None;
            if let Some(file) = &this.file {
                let _ = write(file, &this.state);
            }
            async {}
        });
        Self {
            state,
            file,
            save: None,
            _quit: quit,
        }
    }

    pub fn state(&self) -> &T {
        &self.state
    }

    pub fn update(this: &Entity<Self>, cx: &mut App, change: impl FnOnce(&mut T)) {
        this.update(cx, |store, cx| {
            let before = store.state.clone();
            change(&mut store.state);
            if store.state != before {
                store.schedule_save(cx);
                cx.notify();
            }
        });
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.file.clone() else {
            return;
        };
        self.save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let Ok(state) = this.read_with(cx, |store, _| store.state.clone()) else {
                return;
            };
            let _ = cx
                .background_spawn(async move { write(&file, &state) })
                .await;
        }));
    }
}

#[must_use]
pub fn config_dir(name: &str) -> Option<PathBuf> {
    if let Some(dir) = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
    {
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
