use clankerdiff_git::GitError;

#[test]
fn command_failures_classify_pathspec_errors_without_exposing_stderr() {
    let error = GitError::CommandFailed {
        operation: "stage paths",
        status: Some(128),
        stderr: "fatal: pathspec 'sensitive/file.rs' did not match any files".to_owned(),
    };
    let message = error.to_string();
    assert!(message.contains("stage paths"));
    assert!(message.contains("128"));
    assert!(message.contains("refresh and retry"));
    assert!(!message.contains("sensitive"));
}

#[test]
fn command_failures_classify_index_locks_without_exposing_stderr() {
    let error = GitError::CommandFailed {
        operation: "unstage paths",
        status: Some(128),
        stderr: "fatal: Unable to create '/sensitive/repo/.git/index.lock': File exists."
            .to_owned(),
    };
    let message = error.to_string();
    assert!(message.contains("Git index is locked"));
    assert!(!message.contains("sensitive"));
}

#[test]
fn unrecognized_command_failures_keep_stderr_private() {
    let error = GitError::CommandFailed {
        operation: "commit",
        status: Some(1),
        stderr: "sensitive hook output".to_owned(),
    };
    assert_eq!(
        error.to_string(),
        "git operation `commit` failed with status Some(1)"
    );
}
