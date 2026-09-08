use clankerdiff_git::GitRepository;
use std::path::{Component, Path, PathBuf};

const GIT_METADATA_ENTRIES: [&str; 7] = [
    "index",
    "HEAD",
    "ORIG_HEAD",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REBASE_HEAD",
    "packed-refs",
];

/// What a watched path means for the next snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PathClass {
    /// Git metadata: reload without consulting `git check-ignore`.
    GitMetadata,
    /// A repository-relative, `/`-separated worktree path. An empty path means
    /// the event could not be resolved, so it is treated conservatively.
    Worktree(String),
    /// Git scratch state that never changes a snapshot on its own.
    Ignored,
}

pub(crate) async fn should_refresh(
    repository: &GitRepository,
    metadata_directories: &[PathBuf],
    paths: Vec<PathBuf>,
) -> bool {
    let mut worktree = Vec::new();
    for path in paths {
        match classify(repository.root(), metadata_directories, &path) {
            PathClass::Ignored => {}
            PathClass::GitMetadata => return true,
            PathClass::Worktree(relative) if relative.is_empty() => return true,
            PathClass::Worktree(relative) => worktree.push(relative),
        }
    }
    if worktree.is_empty() {
        return false;
    }
    let Ok(ignored) = repository.ignored_paths(&worktree).await else {
        return true;
    };
    worktree.iter().any(|path| !ignored.contains(path))
}

fn classify(root: &Path, metadata_directories: &[PathBuf], path: &Path) -> PathClass {
    for directory in metadata_directories {
        if let Ok(relative) = path.strip_prefix(directory) {
            return classify_metadata(&normal_components(relative));
        }
    }
    let Ok(relative) = path.strip_prefix(root) else {
        return PathClass::Worktree(String::new());
    };
    let components = normal_components(relative);
    let Some(first) = components.first() else {
        return PathClass::Worktree(String::new());
    };
    if first != ".git" {
        return PathClass::Worktree(components.join("/"));
    }
    classify_metadata(&components[1..])
}

fn normal_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

fn classify_metadata(components: &[String]) -> PathClass {
    // Git writes `*.lock` siblings for every ref and index update it makes, and
    // renames them into place, so only the renamed target is interesting.
    if components.last().is_some_and(|name| {
        Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lock"))
    }) {
        return PathClass::Ignored;
    }
    match components.first().map(String::as_str) {
        None | Some("refs") => PathClass::GitMetadata,
        Some(name) if components.len() == 1 && GIT_METADATA_ENTRIES.contains(&name) => {
            PathClass::GitMetadata
        }
        Some(_) => PathClass::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn class(relative: &str) -> PathClass {
        classify(
            Path::new("/repo"),
            &[],
            &PathBuf::from("/repo").join(relative),
        )
    }

    #[test]
    fn worktree_paths_become_repository_relative_strings() {
        assert_eq!(
            class("src/lib.rs"),
            PathClass::Worktree("src/lib.rs".to_owned())
        );
        assert_eq!(
            class("Cargo.lock"),
            PathClass::Worktree("Cargo.lock".to_owned()),
            "tracked lock files in the worktree are reviewable content"
        );
    }

    #[test]
    fn git_metadata_is_allowlisted_and_scratch_state_is_dropped() {
        assert_eq!(class(".git/index"), PathClass::GitMetadata);
        assert_eq!(class(".git/HEAD"), PathClass::GitMetadata);
        assert_eq!(class(".git/packed-refs"), PathClass::GitMetadata);
        assert_eq!(class(".git/refs/heads/main"), PathClass::GitMetadata);
        assert_eq!(class(".git"), PathClass::GitMetadata);

        assert_eq!(class(".git/index.lock"), PathClass::Ignored);
        assert_eq!(class(".git/refs/heads/main.lock"), PathClass::Ignored);
        assert_eq!(class(".git/objects/ab/cdef"), PathClass::Ignored);
        assert_eq!(class(".git/logs/HEAD"), PathClass::Ignored);
    }

    #[test]
    fn paths_outside_the_root_are_treated_conservatively() {
        assert_eq!(
            classify(Path::new("/repo"), &[], Path::new("/elsewhere/file.rs")),
            PathClass::Worktree(String::new())
        );
    }
}
