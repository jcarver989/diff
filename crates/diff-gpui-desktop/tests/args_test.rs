use clankerdiff_core::DiffScope;
use clankerdiff_gpui_desktop::args::{ArgsError, CliArgs};
use std::{error::Error, ffi::OsString};

#[test]
fn remote_arguments_do_not_select_a_local_repository() -> Result<(), Box<dyn Error>> {
    let args = CliArgs::parse_from(
        ["--connect", "wss://example.test/ws", "--scope", "staged"].map(OsString::from),
    )?;
    assert_eq!(args.connect.as_deref(), Some("wss://example.test/ws"));
    assert!(args.repository.as_os_str().is_empty());
    assert_eq!(args.scope, DiffScope::Staged);
    Ok(())
}

#[test]
fn remote_and_local_sources_conflict() {
    for args in [
        ["--connect", "ws://localhost/ws", "."],
        [".", "--connect", "ws://localhost/ws"],
    ] {
        assert!(matches!(
            CliArgs::parse_from(args.map(OsString::from)),
            Err(ArgsError::ConflictingSource)
        ));
    }
}

#[test]
fn remote_urls_require_websocket_scheme() {
    for url in ["", "https://example.test/ws", "/tmp/repo"] {
        assert!(matches!(
            CliArgs::parse_from(["--connect", url].map(OsString::from)),
            Err(ArgsError::InvalidConnect)
        ));
    }
}
