use clankerdiff_core::{DiffReviewEvent, DiffScope, ParseDiffScopeError};

#[test]
fn set_scope_events_round_trip() -> Result<(), serde_json::Error> {
    for scope in [DiffScope::Unstaged, DiffScope::Staged, DiffScope::Both] {
        let event = DiffReviewEvent::SetScope(scope);
        let json = serde_json::to_string(&event)?;
        assert_eq!(serde_json::from_str::<DiffReviewEvent>(&json)?, event);
    }
    Ok(())
}

#[test]
fn scope_cycles_unstaged_staged_both() {
    assert_eq!(DiffScope::Unstaged.next(), DiffScope::Staged);
    assert_eq!(DiffScope::Staged.next(), DiffScope::Both);
    assert_eq!(DiffScope::Both.next(), DiffScope::Unstaged);
}

#[test]
fn parses_and_renders_scopes() -> Result<(), ParseDiffScopeError> {
    assert_eq!("staged".parse::<DiffScope>()?, DiffScope::Staged);
    assert_eq!(DiffScope::Both.to_string(), "both");
    assert!("nope".parse::<DiffScope>().is_err());
    assert_eq!(DiffScope::Both.next(), DiffScope::Unstaged);
    Ok(())
}
