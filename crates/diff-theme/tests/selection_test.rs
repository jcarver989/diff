use clankerdiff_theme::{ReviewTheme, ThemeChoice, ThemeSelection};
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
