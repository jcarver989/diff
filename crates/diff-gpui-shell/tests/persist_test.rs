use clankerdiff_gpui_shell::persist;
use std::{collections::BTreeMap, error::Error, fs};

#[test]
fn written_values_read_back() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("nested/state.json");
    let value = BTreeMap::from([("theme".to_owned(), "nord".to_owned())]);

    persist::write(&file, &value)?;

    assert_eq!(persist::read(&file), Some(value));
    Ok(())
}

#[test]
fn a_missing_or_unreadable_file_reads_as_nothing() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("state.json");
    assert_eq!(persist::read::<BTreeMap<String, String>>(&file), None);

    fs::write(&file, "not json")?;

    assert_eq!(persist::read::<BTreeMap<String, String>>(&file), None);
    Ok(())
}
