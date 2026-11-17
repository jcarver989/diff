use clankerdiff_core::{
    DiffError, FileStatus, PatchLineKind, RepoPath, RepoPathError, git_patch_from_texts, parse_detached_git_diff,
    parse_git_diff, parse_git_diff_with_path_mapper,
};

#[test]
fn absolute_patch_paths_are_decoded_before_mapping() -> Result<(), DiffError> {
    let paths = ["/workspace/a file.rs", "/workspace/quoted\"\\界\tfile.rs"];
    let patch = paths.iter().map(|path| changed_patch(path, Some("-- /old\n"), Some("++ /new\n"))).collect::<String>();
    let mut decoded = Vec::new();
    let files = parse_git_diff_with_path_mapper(patch.as_bytes(), |path| {
        decoded.push(path.to_owned());
        workspace_path(path)
    })?;
    assert_eq!(files.len(), 2);
    assert_eq!(decoded, [paths[0], paths[0], paths[1], paths[1]]);
    for (file, path) in files.iter().zip(paths) {
        assert_eq!(file.path.as_str(), path.trim_start_matches("/workspace/"));
        assert_eq!(file.old_path.as_ref(), Some(&file.path));
        assert_eq!(file.status, FileStatus::Modified);
        assert_eq!(file.hunks[0].lines[0].kind, PatchLineKind::Removed);
        assert_eq!(file.hunks[0].lines[0].text.as_ref(), "-- /old");
        assert_eq!(file.hunks[0].lines[1].text.as_ref(), "++ /new");
    }
    Ok(())
}

#[test]
fn mapping_is_explicit_and_keeps_repository_path_validation() {
    let patch = changed_patch("/workspace/file.rs", Some("old\n"), Some("new\n"));
    assert!(matches!(parse_git_diff(patch.as_bytes()), Err(DiffError::InvalidPath(RepoPathError::Absolute))));
    for (path, expected) in [
        ("/still/absolute", RepoPathError::Absolute),
        ("../escape", RepoPathError::Traversal),
        ("", RepoPathError::Empty),
        ("nul\0path", RepoPathError::Nul),
    ] {
        let result = parse_git_diff_with_path_mapper(patch.as_bytes(), |_| RepoPath::new(path));
        assert!(matches!(result, Err(DiffError::InvalidPath(error)) if error == expected));
    }
}

#[test]
fn detached_patch_paths_accept_relative_and_absolute_names() -> Result<(), DiffError> {
    for path in ["src/main.rs", "/src/main.rs"] {
        let patch = changed_patch(path, Some("old\n"), Some("new\n"));
        let files = parse_detached_git_diff(patch.as_bytes())?;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path.as_str(), "src/main.rs");
        assert_eq!(files[0].old_path.as_ref(), Some(&files[0].path));
    }
    Ok(())
}

#[test]
fn patch_lines_keep_their_text_and_final_newline_status() -> Result<(), DiffError> {
    for ending in ["\n", "\r\n", ""] {
        let patch = changed_patch("main.rs", Some(&format!("old{ending}")), Some(&format!("new{ending}")));
        let files = parse_git_diff(patch.as_bytes())?;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].no_newline_at_end, ending.is_empty());
        let lines = &files[0].hunks[0].lines;
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text.as_ref(), "old");
        assert_eq!(lines[1].text.as_ref(), "new");
        assert!(lines.iter().all(|line| line.no_newline == ending.is_empty()));
    }
    Ok(())
}

fn workspace_path(path: &str) -> Result<RepoPath, RepoPathError> {
    RepoPath::new(path.trim_prefix("/workspace/"))
}

fn changed_patch(path: &str, old: Option<&str>, new: Option<&str>) -> String {
    git_patch_from_texts(path, old, new).expect("valid patch fixture").expect("changed content")
}
