use clankerdiff_core::{
    DiffError, FileStatus, PatchLineKind, RepoPath, RepoPathError, git_patch_from_texts,
    parse_git_diff, parse_git_diff_with_path_mapper,
};

#[test]
fn absolute_patch_paths_are_decoded_before_mapping() -> Result<(), DiffError> {
    let paths = ["/workspace/a file.rs", "/workspace/quoted\"\\界\tfile.rs"];
    let patch = paths
        .iter()
        .map(|path| changed_patch(path, Some("-- /old\n"), Some("++ /new\n")))
        .collect::<String>();
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
    assert!(matches!(
        parse_git_diff(patch.as_bytes()),
        Err(DiffError::InvalidPath(RepoPathError::Absolute))
    ));
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

fn workspace_path(path: &str) -> Result<RepoPath, RepoPathError> {
    RepoPath::new(path.strip_prefix("/workspace/").unwrap_or(path))
}

fn changed_patch(path: &str, old: Option<&str>, new: Option<&str>) -> String {
    git_patch_from_texts(path, old, new)
        .expect("valid patch fixture")
        .expect("changed content")
}
