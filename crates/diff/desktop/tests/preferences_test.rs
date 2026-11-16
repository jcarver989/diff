use clankerdiff_core::ViewMode;
use clankerdiff_desktop::preferences::Preferences;
use clankerdiff_ui_core::{Appearance, ReviewTheme, ThemeId, ThemePair};
use rouge::{
    core::{
        appearance, div,
        testing::{TestResult, TestUi, TestUiBuilder},
    },
    persist::{FileStorage, MemoryStorage, Storage, provide_storage},
    reactive::Runtime,
};
use rouge_widgets::{DEFAULT_INTERFACE_SIZE, InterfaceSize};
use std::{fs, rc::Rc};

#[test]
fn missing_preferences_offer_defaults() {
    let _entered = Runtime::new().enter();
    let (_ui, preferences, size, appearance) = PreferencesBuilder::default().storage(MemoryStorage::default()).build();
    assert_eq!(preferences.theme().id(), &ThemeId::Builtin(ThemePair::default().id(appearance).to_owned()));
    assert_eq!(preferences.view_mode(), ViewMode::Auto);
    assert!(preferences.wrap());
    assert_eq!(size.committed(), DEFAULT_INTERFACE_SIZE);
}

#[test]
fn unreadable_preferences_offer_defaults() {
    let _entered = Runtime::new().enter();
    let storage = MemoryStorage::default();
    for key in ["theme", "view_mode", "interface_size", "wrap"] {
        storage.insert(key, "not json");
    }
    let (_ui, preferences, size, appearance) = PreferencesBuilder::default().storage(storage).build();
    assert_eq!(preferences.theme().id(), &ThemeId::Builtin(ThemePair::default().id(appearance).to_owned()));
    assert_eq!(preferences.view_mode(), ViewMode::Auto);
    assert!(preferences.wrap());
    assert_eq!(size.committed(), DEFAULT_INTERFACE_SIZE);
}

#[test]
fn remembering_a_theme_keeps_the_other_appearances_theme() -> TestResult {
    let _entered = Runtime::new().enter();
    let storage = MemoryStorage::default();
    let initial = ThemePair { light: "alabaster".to_owned(), dark: "nord".to_owned() };
    storage.insert("theme", serde_json::to_string(&initial)?);
    let (mut ui, preferences, _, appearance) = PreferencesBuilder::default().storage(storage.clone()).build();
    preferences.remember_theme("ayu-dark");
    ui.advance(0.0);
    assert_eq!(preferences.theme().id(), ReviewTheme::builtin("ayu-dark")?.id());
    drop(ui);
    let mut expected = initial;
    expected.set(appearance, "ayu-dark");
    assert_eq!(serde_json::from_str::<ThemePair>(&storage.get("theme").ok_or("saved theme")?)?, expected);
    Ok(())
}

#[test]
fn an_unknown_theme_id_resolves_to_the_default_theme() {
    let _entered = Runtime::new().enter();
    let (_ui, preferences, _, _) = PreferencesBuilder::default().build();
    preferences.remember_theme("no-such-theme");
    assert_eq!(preferences.theme().id(), ReviewTheme::default().id());
}

#[test]
fn preferences_without_storage_remain_usable_but_are_not_saved() {
    let _entered = Runtime::new().enter();
    let (ui, preferences, _, _) = PreferencesBuilder::default().build();
    preferences.remember_mode(ViewMode::Split);
    preferences.remember_wrap(false);
    assert_eq!(preferences.view_mode(), ViewMode::Split);
    assert!(!preferences.wrap());
    drop(ui);
    let (_ui, preferences, _, _) = PreferencesBuilder::default().build();
    assert_eq!(preferences.view_mode(), ViewMode::Auto);
    assert!(preferences.wrap());
}

#[test]
fn closing_a_window_flushes_preferences_and_the_next_window_restores_them() -> TestResult {
    let _entered = Runtime::new().enter();
    let storage = MemoryStorage::default();
    let (ui, preferences, size, _) = PreferencesBuilder::default().storage(storage.clone()).build();
    preferences.remember_theme("nord");
    preferences.remember_mode(ViewMode::Split);
    preferences.remember_wrap(false);
    size.set(23.0);
    drop(ui);
    assert_eq!(serde_json::from_str::<ViewMode>(&storage.get("view_mode").ok_or("saved mode")?)?, ViewMode::Split);
    assert_eq!(storage.get("wrap").as_deref(), Some("false"));
    assert_eq!(storage.get("interface_size").as_deref(), Some("23.0"));
    let (_ui, restored, size, _) = PreferencesBuilder::default().storage(storage).build();
    assert_eq!(restored.theme().id(), &ThemeId::Builtin("nord".to_owned()));
    assert_eq!(restored.view_mode(), ViewMode::Split);
    assert!(!restored.wrap());
    assert_eq!(size.committed(), 23.0);
    Ok(())
}

#[test]
fn interface_size_previews_are_not_saved() {
    let _entered = Runtime::new().enter();
    let storage = MemoryStorage::default();
    let (mut ui, _, size, _) = PreferencesBuilder::default().storage(storage.clone()).build();
    size.set(21.0);
    ui.advance(0.0);
    size.preview(27.0);
    ui.advance(0.0);
    assert_eq!(size.get(), 27.0);
    assert_eq!(size.committed(), 21.0);
    drop(ui);
    assert_eq!(storage.get("interface_size").as_deref(), Some("21.0"));
}

#[test]
fn existing_json_files_restore_all_preferences_asynchronously() -> TestResult {
    let _entered = Runtime::new().enter();
    let dir = tempfile::tempdir()?;
    let pair = ThemePair { light: "alabaster".to_owned(), dark: "nord".to_owned() };
    fs::write(dir.path().join("theme.json"), serde_json::to_string(&pair)?)?;
    fs::write(dir.path().join("view_mode.json"), serde_json::to_string(&ViewMode::Unified)?)?;
    fs::write(dir.path().join("interface_size.json"), "22.0")?;
    fs::write(dir.path().join("wrap.json"), "false")?;
    let (mut ui, preferences, size, appearance) =
        PreferencesBuilder::default().storage(FileStorage::new(dir.path())).build();
    assert!(ui.wait_until(|_| {
        preferences.theme().id() == &ThemeId::Builtin(pair.id(appearance).to_owned())
            && preferences.view_mode() == ViewMode::Unified
            && !preferences.wrap()
            && (size.committed() - 22.0).abs() < f32::EPSILON
    }));
    ui.run(0.6);
    drop(ui);
    assert_eq!(fs::read_to_string(dir.path().join("interface_size.json"))?, "22.0");
    Ok(())
}

#[test]
fn file_storage_flushes_to_the_existing_filenames_on_close() -> TestResult {
    let _entered = Runtime::new().enter();
    let dir = tempfile::tempdir()?;
    let path = dir.path();
    let pair = ThemePair { light: "alabaster".to_owned(), dark: "alabaster".to_owned() };
    fs::write(path.join("theme.json"), serde_json::to_string(&pair)?)?;
    fs::write(path.join("view_mode.json"), serde_json::to_string(&ViewMode::Unified)?)?;
    fs::write(path.join("interface_size.json"), "20.0")?;
    fs::write(path.join("wrap.json"), "false")?;
    let (mut ui, preferences, size, _) = PreferencesBuilder::default().storage(FileStorage::new(path)).build();
    assert!(ui.wait_until(|_| {
        preferences.theme().id() == &ThemeId::Builtin("alabaster".to_owned())
            && preferences.view_mode() == ViewMode::Unified
            && !preferences.wrap()
            && (size.committed() - 20.0).abs() < f32::EPSILON
    }));
    preferences.remember_theme("nord");
    preferences.remember_mode(ViewMode::Split);
    preferences.remember_wrap(true);
    size.set(24.0);
    drop(ui);
    assert_eq!(serde_json::from_str::<ViewMode>(&fs::read_to_string(path.join("view_mode.json"))?)?, ViewMode::Split);
    assert_eq!(fs::read_to_string(path.join("wrap.json"))?, "true");
    assert_eq!(fs::read_to_string(path.join("interface_size.json"))?, "24.0");
    assert!(path.join("theme.json").is_file());
    Ok(())
}

#[derive(Default)]
struct PreferencesBuilder {
    storage: Option<Rc<dyn Storage>>,
}

impl PreferencesBuilder {
    fn storage(mut self, storage: impl Storage + 'static) -> Self {
        self.storage = Some(Rc::new(storage));
        self
    }

    fn build(self) -> (TestUi, Preferences, InterfaceSize, Appearance) {
        let (ui, (preferences, size, appearance)) = TestUiBuilder::default().build_with(move || {
            if let Some(storage) = self.storage {
                provide_storage(storage);
            }
            let preferences = Preferences::load();
            let size = preferences.interface_size();
            (div(), (preferences, size, appearance().get()))
        });
        (ui, preferences, size, appearance)
    }
}
