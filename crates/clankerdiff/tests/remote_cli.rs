use std::{error::Error, process::Command};

#[test]
fn remote_commands_are_available_in_executable() -> Result<(), Box<dyn Error>> {
    for command in ["serve", "connect"] {
        let output = Command::new(env!("CARGO_BIN_EXE_clankerdiff"))
            .args([command, "--help"])
            .output()?;
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout)?;
        assert!(text.contains(command));
        if command == "serve" {
            assert!(text.contains("127.0.0.1:7331"));
        } else {
            assert!(text.contains("--ui"));
            assert!(text.contains("--scope"));
        }
    }
    Ok(())
}

#[cfg(not(feature = "desktop"))]
#[test]
fn headless_desktop_request_fails_before_network_connection() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_clankerdiff"))
        .args(["connect", "ws://127.0.0.1:1/ws", "--ui", "desktop"])
        .output()?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr)?.contains("desktop"));
    Ok(())
}
