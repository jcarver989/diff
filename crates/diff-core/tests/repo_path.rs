use diff_core::{RepoPath, RepoPathError};

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
