use clankerdiff_core::{RepoPath, RepoPathError};

#[test]
fn validates_relative_utf8_paths() -> Result<(), RepoPathError> {
    assert!(RepoPath::new("src/é file.rs").is_ok());
    assert_eq!(RepoPath::new("../secret"), Err(RepoPathError::Traversal));
    assert_eq!(RepoPath::new("/absolute"), Err(RepoPathError::Absolute));
    assert_eq!(RepoPath::new("a//b"), Err(RepoPathError::Empty));
    assert_eq!(RepoPath::new("a\0b"), Err(RepoPathError::Nul));
    assert_eq!(RepoPath::new("a\\\\b")?.as_str(), "a\\\\b");
    Ok(())
}

#[test]
fn splits_file_names_from_their_directories() -> Result<(), RepoPathError> {
    let nested = RepoPath::new("crates/desktop/src/code.rs")?;
    assert_eq!(nested.file_name(), "code.rs");
    assert_eq!(nested.directory(), Some("crates/desktop/src"));
    let root = RepoPath::new("README.md")?;
    assert_eq!(root.file_name(), "README.md");
    assert_eq!(root.directory(), None);
    Ok(())
}
