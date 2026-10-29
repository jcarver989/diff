use clankerdiff_core::ViewMode;
use clankerdiff_gpui_desktop::preferences::{ThemeStore, ViewModeStore};
use clankerdiff_ui_core::{Appearance, ThemeId, ThemePair};
use std::{error::Error, fs};

#[test]
fn a_missing_or_unreadable_file_offers_the_default_pair() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("theme.json");
    assert_eq!(ThemeStore::new(Some(file.clone())).pair(), ThemePair::default());
    fs::write(&file, "not json")?;
    assert_eq!(ThemeStore::new(Some(file)).pair(), ThemePair::default());
    assert_eq!(ThemeStore::new(None).pair(), ThemePair::default());
    Ok(())
}

#[test]
fn remembering_a_theme_keeps_the_other_appearances_theme() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let store = ThemeStore::new(Some(dir.path().join("nested/theme.json")));
    store.remember(Appearance::Dark, "nord")?;
    store.remember(Appearance::Light, "alabaster")?;
    assert_eq!(store.pair(), ThemePair { light: "alabaster".to_owned(), dark: "nord".to_owned() });
    assert_eq!(store.theme(Appearance::Dark).id(), &ThemeId::Builtin("nord".to_owned()));
    Ok(())
}

#[test]
fn an_unknown_theme_id_resolves_to_the_default_theme() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let store = ThemeStore::new(Some(dir.path().join("theme.json")));
    store.remember(Appearance::Light, "no-such-theme")?;
    assert_eq!(store.theme(Appearance::Light).id(), clankerdiff_ui_core::ReviewTheme::default().id());
    Ok(())
}

#[test]
fn a_store_without_a_file_saves_nothing() -> Result<(), Box<dyn Error>> {
    let store = ThemeStore::new(None);
    store.remember(Appearance::Dark, "nord")?;
    assert_eq!(store.pair(), ThemePair::default());
    Ok(())
}

#[test]
fn a_missing_or_unreadable_view_mode_file_offers_auto() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("view_mode.json");
    assert_eq!(ViewModeStore::new(Some(file.clone())).view_mode(), ViewMode::Auto);
    fs::write(&file, "not json")?;
    assert_eq!(ViewModeStore::new(Some(file)).view_mode(), ViewMode::Auto);
    Ok(())
}

#[test]
fn a_remembered_view_mode_is_offered_next_time() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("nested/view_mode.json");
    ViewModeStore::new(Some(file.clone())).remember(ViewMode::Split)?;
    assert_eq!(ViewModeStore::new(Some(file)).view_mode(), ViewMode::Split);
    Ok(())
}

#[test]
fn a_view_mode_store_without_a_file_saves_nothing() -> Result<(), Box<dyn Error>> {
    let store = ViewModeStore::new(None);
    store.remember(ViewMode::Unified)?;
    assert_eq!(store.view_mode(), ViewMode::Auto);
    Ok(())
}
