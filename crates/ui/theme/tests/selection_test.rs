use clankerdiff_theme::{Appearance, ReviewTheme, ThemeChoice, ThemeSelection};
use std::{error::Error, sync::Arc};

#[test]
fn preview_cancel_and_commit_use_the_same_selection_model() -> Result<(), Box<dyn Error>> {
    let original = ReviewTheme::default();
    let choices = ThemeChoice::catalog();
    let mut selection =
        ThemeSelection::new(&original, Arc::clone(&choices)).ok_or("empty catalog")?;
    let preview = selection.select_relative(isize::MAX);
    assert_eq!(preview.id(), choices[choices.len() - 1].theme.id());
    assert_eq!(selection.cancel().id(), original.id());
    let mut selection =
        ThemeSelection::new(&original, Arc::clone(&choices)).ok_or("empty catalog")?;
    assert!(selection.select(usize::MAX).is_none());
    selection.select(0).ok_or("missing first theme")?;
    assert_eq!(selection.commit().id(), choices[0].theme.id());
    assert!(ThemeSelection::new(&original, Arc::from([])).is_none());
    Ok(())
}

#[test]
fn position_finds_a_theme_by_id() -> Result<(), Box<dyn Error>> {
    let choices = ThemeChoice::catalog();
    let selection = ThemeSelection::new(&ReviewTheme::default(), Arc::clone(&choices))
        .ok_or("empty catalog")?;
    let last = choices.len() - 1;
    assert_eq!(
        selection.position(&choices[last].theme.id().to_string()),
        Some(last)
    );
    assert_eq!(selection.position("missing"), None);
    Ok(())
}

#[test]
fn catalog_for_offers_only_themes_with_that_appearance() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let choices = ThemeChoice::catalog_for(appearance);
        assert!(!choices.is_empty());
        assert!(
            choices
                .iter()
                .all(|choice| choice.theme.appearance() == appearance)
        );
    }
    assert_eq!(
        ThemeChoice::catalog_for(Appearance::Light).len()
            + ThemeChoice::catalog_for(Appearance::Dark).len(),
        ThemeChoice::catalog().len()
    );
}
