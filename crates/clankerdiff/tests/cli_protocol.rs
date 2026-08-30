use clankerdiff_protocol::{CapabilityResponse, PROTOCOL_VERSION};
use std::process::Command;

fn clankerdiff() -> Command {
    Command::new(env!("CARGO_BIN_EXE_clankerdiff"))
}

#[test]
fn capabilities_are_one_clean_json_line() {
    let output = clankerdiff()
        .args(["capabilities", "--format=json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        output
            .stdout
            .split(|byte| *byte == b'\n')
            .count()
            .saturating_sub(1),
        1
    );
    assert_eq!(output.stdout.last(), Some(&b'\n'));

    let capabilities: CapabilityResponse = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(capabilities.protocol_version, PROTOCOL_VERSION);
    assert_eq!(capabilities.supported_protocol_versions, [PROTOCOL_VERSION]);
    assert!(capabilities.current_terminal_tui);
}

#[test]
fn process_failures_keep_stdout_protocol_clean() {
    let output = clankerdiff()
        .args(["markdown", "this-file-does-not-exist.md", "--format=json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
}
