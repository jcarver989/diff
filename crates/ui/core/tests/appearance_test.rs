use clankerdiff_ui_core::{Appearance, ReviewTheme, ThemePair};
use std::error::Error;

#[test]
fn every_built_in_theme_reports_the_appearance_its_catalog_entry_declares() -> Result<(), Box<dyn Error>> {
    for descriptor in ReviewTheme::catalog() {
        let expected = if descriptor.is_dark { Appearance::Dark } else { Appearance::Light };
        assert_eq!(ReviewTheme::builtin(&descriptor.id)?.appearance(), expected, "{}", descriptor.id);
    }
    Ok(())
}

#[test]
fn a_pair_sets_and_reads_back_the_theme_for_each_appearance() {
    let mut pair = ThemePair::default();
    pair.set(Appearance::Dark, "nord");
    assert_eq!(pair.id(Appearance::Dark), "nord");
    assert_eq!(pair.id(Appearance::Light), ThemePair::default().light);
}

#[test]
fn a_pair_round_trips_through_json() -> Result<(), Box<dyn Error>> {
    let pair = ThemePair { light: "alabaster".to_owned(), dark: "nord".to_owned() };
    let json = serde_json::to_string(&pair)?;
    assert_eq!(json, r#"{"light":"alabaster","dark":"nord"}"#);
    assert_eq!(serde_json::from_str::<ThemePair>(&json)?, pair);
    assert_eq!(serde_json::to_string(&Appearance::Dark)?, r#""dark""#);
    Ok(())
}

#[test]
fn a_pair_missing_a_theme_reads_the_default_for_it() -> Result<(), Box<dyn Error>> {
    let pair: ThemePair = serde_json::from_str(r#"{"dark":"nord"}"#)?;
    assert_eq!(pair.light, ThemePair::default().light);
    assert_eq!(pair.dark, "nord");
    Ok(())
}
