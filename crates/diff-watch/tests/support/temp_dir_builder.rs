use std::{collections::BTreeMap, fs, io, path::PathBuf};
use tempfile::TempDir;

pub struct TempDirBuilder {
    entries: BTreeMap<PathBuf, String>,
}

impl TempDirBuilder {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub fn entries<T: Into<PathBuf>, U: Into<String>>(
        mut self,
        entries: impl IntoIterator<Item = (T, U)>,
    ) -> Self {
        self.entries.extend(
            entries
                .into_iter()
                .map(|(path, contents)| (path.into(), contents.into())),
        );
        self
    }

    pub fn build(self) -> io::Result<TempDir> {
        let directory = tempfile::tempdir()?;
        for (path, contents) in self.entries {
            let path = directory.path().join(path);
            if contents.is_empty() {
                fs::create_dir_all(path)?;
            } else {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, contents)?;
            }
        }
        Ok(directory)
    }
}
