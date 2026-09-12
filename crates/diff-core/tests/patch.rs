use clankerdiff_core::patch::{PatchError, git_patch_from_texts};
use std::{
    fs,
    io::{self, Write},
    process::{Command, Stdio},
};
use tempfile::{TempDir, tempdir};
use thiserror::Error;

#[test]
fn patches_apply_and_reverse_to_exact_snapshots() -> Result<(), TestError> {
    let cases = [
        (None, Some("new\n")),
        (None, Some("new")),
        (None, Some("")),
        (Some("old\n"), None),
        (Some("old"), None),
        (Some(""), None),
        (Some(""), Some("new\n")),
        (Some("old\n"), Some("")),
        (Some("old\n"), Some("new\n")),
        (Some("old"), Some("new")),
        (Some("old\n"), Some("new")),
        (Some("old"), Some("new\n")),
        (Some("a\r\nb\r\n"), Some("a\r\nc\r\n")),
        (Some("a\r\nb\r\n"), Some("a\nb\n")),
        (Some("a\nb\r\nc"), Some("a\r\nb\nc\n")),
        (Some("old\nunchanged"), Some("new\nunchanged")),
        (Some("old\n\nend\n"), Some("new\n\nend\n")),
        (Some("雪\nold\n"), Some("雪\nnew\n")),
    ];
    for absolute in [false, true] {
        for name in ["file.txt", "space name.txt", "quote\"back\\tab\t雪\n.txt"] {
            for (old, new) in cases {
                PatchFixture::new()?
                    .name(name)
                    .absolute(absolute)
                    .check(old, new)?;
            }
        }
    }
    Ok(())
}

#[test]
fn no_patch_for_identical_snapshots() -> Result<(), PatchError> {
    for text in [None, Some(""), Some("same\n"), Some("same")] {
        assert_eq!(git_patch_from_texts("file.txt", text, text)?, None);
    }
    Ok(())
}

#[test]
fn file_existence_controls_git_headers() -> Result<(), TestError> {
    for (old, new, header) in [
        (None, Some(""), "new file mode 100644\n"),
        (Some(""), None, "deleted file mode 100644\n"),
    ] {
        let patch = git_patch_from_texts("file.txt", old, new)?.ok_or(TestError::MissingPatch)?;
        assert!(patch.contains(header));
        assert!(!patch.contains("@@"));
    }
    for (old, new) in [("", "text\n"), ("text\n", "")] {
        let patch = git_patch_from_texts("file.txt", Some(old), Some(new))?
            .ok_or(TestError::MissingPatch)?;
        assert!(!patch.contains("file mode"));
        assert!(!patch.contains("/dev/null"));
    }
    Ok(())
}

#[test]
fn distant_edits_have_compact_contextual_hunks() -> Result<(), TestError> {
    let mut old = String::new();
    for line in 0..1000 {
        old.extend(["line ", &line.to_string(), "\n"]);
    }
    let new = old
        .replace("line 10\n", "changed 10\n")
        .replace("line 990\n", "changed 990\n");
    let patch =
        git_patch_from_texts("file.txt", Some(&old), Some(&new))?.ok_or(TestError::MissingPatch)?;
    assert_eq!(
        patch.lines().filter(|line| line.starts_with("@@")).count(),
        2
    );
    assert!(patch.contains(" line 9\n"));
    assert!(patch.contains("-line 10\n+changed 10\n"));
    assert!(!patch.contains("line 500\n"));
    assert!(patch.lines().count() < 30);
    PatchFixture::new()?.check(Some(&old), Some(&new))
}

#[test]
fn eof_outside_changed_hunks_is_preserved() -> Result<(), TestError> {
    let old = format!("old\n{}last", "context\n".repeat(20));
    let new = old.replacen("old", "new", 1);
    let patch =
        git_patch_from_texts("file.txt", Some(&old), Some(&new))?.ok_or(TestError::MissingPatch)?;
    assert!(!patch.contains("No newline"));
    PatchFixture::new()?.check(Some(&old), Some(&new))
}

#[test]
fn rejects_invalid_paths_and_binary_text() {
    for path in ["", "bad\0path", "/dev/null"] {
        assert!(matches!(
            git_patch_from_texts(path, None, Some("new")),
            Err(PatchError::InvalidPath)
        ));
    }
    for (old, new) in [(Some("old\0"), Some("new")), (None, Some("new\0"))] {
        assert!(matches!(
            git_patch_from_texts("file.txt", old, new),
            Err(PatchError::BinaryText)
        ));
    }
}

#[derive(Debug, Error)]
enum TestError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Patch(#[from] PatchError),
    #[error("expected a patch")]
    MissingPatch,
}

struct PatchFixture {
    directory: TempDir,
    name: String,
    absolute: bool,
}

impl PatchFixture {
    fn new() -> Result<Self, TestError> {
        let directory = tempdir()?;
        fs::write(directory.path().join(".keep"), "")?;
        Ok(Self {
            directory,
            name: "file.txt".into(),
            absolute: false,
        })
    }

    fn name(mut self, name: &str) -> Self {
        self.name = name.into();
        self
    }

    fn absolute(mut self, absolute: bool) -> Self {
        self.absolute = absolute;
        self
    }

    fn check(self, old: Option<&str>, new: Option<&str>) -> Result<(), TestError> {
        let file_path = self.directory.path().canonicalize()?.join(&self.name);
        if let Some(old) = old {
            fs::write(&file_path, old)?;
        }
        let patch_path = if self.absolute {
            file_path.to_string_lossy().into_owned()
        } else {
            self.name.clone()
        };
        let patch = git_patch_from_texts(&patch_path, old, new)?.ok_or(TestError::MissingPatch)?;
        for (reverse, expected) in [(false, new), (true, old)] {
            let mut command = Command::new("git");
            command.current_dir(self.directory.path()).args([
                "apply",
                "--unsafe-paths",
                "--whitespace=nowarn",
                "-p0",
            ]);
            if reverse {
                command.arg("--reverse");
            }
            let mut child = command
                .arg("-")
                .stdin(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            child
                .stdin
                .take()
                .ok_or_else(|| io::Error::other("missing stdin"))?
                .write_all(patch.as_bytes())?;
            let output = child.wait_with_output()?;
            assert!(
                output.status.success(),
                "reverse={reverse}: {}\n{patch}",
                String::from_utf8_lossy(&output.stderr)
            );
            match expected {
                Some(text) => {
                    assert!(
                        file_path.try_exists()?,
                        "missing file after reverse={reverse}: {patch}"
                    );
                    assert_eq!(fs::read(&file_path)?, text.as_bytes());
                }
                None => assert!(!file_path.try_exists()?),
            }
        }
        Ok(())
    }
}
