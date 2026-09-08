use clankerdiff_core::{DiffScope, ParseDiffScopeError};

#[test]
fn parses_and_renders_scopes() -> Result<(), ParseDiffScopeError> {
    assert_eq!("staged".parse::<DiffScope>()?, DiffScope::Staged);
    assert_eq!(DiffScope::Both.to_string(), "both");
    assert!("nope".parse::<DiffScope>().is_err());
    assert_eq!(DiffScope::Both.next(), DiffScope::Unstaged);
    Ok(())
}
